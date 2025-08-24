//! ## Support for IMD disk images
//!
//! This format provides access to a wide variety of CP/M and FAT file systems.
//! The FAT support should work with a wide variety of disk layouts since, except
//! for very early disks, the BPB is always discoverable in the boot sector.
//! For CP/M, only specific vendors are supported, due to the fact that the DPB
//! usually has to be supplied for each individual case.

use chrono;
use num_traits::FromPrimitive;
use num_derive::FromPrimitive;
use a2kit_macro::{DiskStructError,DiskStruct};
use crate::fs::cpm::types::RECORD_SIZE;
use crate::img;
use crate::img::{DiskImage,meta};
use crate::bios::{skew,blocks};
use crate::bios::Block;
use crate::img::names::*;
use crate::{STDRESULT,DYNERR,putString};

#[derive(FromPrimitive)]
pub enum Mode {
    Fm500Kbps = 0,
    Fm300Kbps = 1,
    Fm250Kbps = 2,
    Mfm500Kbps = 3,
    Mfm300Kbps = 4,
    Mfm250Kbps = 5
}

pub const SECTOR_SIZE_BASE: usize = 128;
pub const CYL_MAP_FLAG: u8 = 0x80;
pub const HEAD_MAP_FLAG: u8 = 0x40;
pub const HEAD_MASK: u8 = 0b1111;

pub fn file_extensions() -> Vec<String> {
    vec!["imd".to_string()]
}

#[derive(FromPrimitive)]
pub enum SectorData {
    None = 0,
    Normal = 1,
    NormalCompressed = 2,
    NormalDeleted = 3,
    NormalCompressedDeleted = 4,
    Error = 5,
    ErrorCompressed = 6,
    ErrorDeleted = 7,
    ErrorCompressedDeleted = 8
}

pub fn is_slice_uniform(slice: &[u8]) -> bool {
    if slice.len()<1 {
        return true;
    }
    let test = slice[0];
    for i in 1..slice.len() {
        if slice[i]!=test {
            return false;
        }
    }
    true
}

pub struct Track {
    mode: u8,
    cylinder: u8,
    head_flags: u8,
    sectors: u8,
    sector_shift: u8,
    /// In these maps we suppose the order corresponds to geometry.
    /// The map entry then gives a CHS address that corresponds to an offset in
    /// the track buffer.
    sector_map: Vec<u8>,
    cylinder_map: Vec<u8>,
    head_map: Vec<u8>,
    track_buf: Vec<u8>,
    /// extensions (not part of IMD file)
    head_pos: usize,
    buf_offset: usize
}

/// There is a trivial compression scheme for the track data.
/// Compression happens when the structure is flattened.
/// Expansion happens when the structure is unflattened.
/// Hence while we are working with the disk it is always expanded.
pub struct Imd {
    kind: img::DiskKind,
    heads: usize,
    header: [u8;29],
    comment: String,
    terminator: u8,
    tracks: Vec<Track>
}

impl Track {
    fn create(track_num: usize, layout: &super::TrackLayout) -> Self {
        let zone = layout.zone(track_num);
        let mode = match (layout.flux_code[zone],layout.speed_kbps[zone]) {
            (super::FluxCode::FM,250) => Mode::Fm250Kbps,
            (super::FluxCode::FM,300) => Mode::Fm300Kbps,
            (super::FluxCode::FM,500) => Mode::Fm500Kbps,
            (super::FluxCode::MFM,250) => Mode::Mfm250Kbps,
            (super::FluxCode::MFM,300) => Mode::Mfm300Kbps,
            (super::FluxCode::MFM,500) => Mode::Mfm500Kbps,
            _ => panic!("unhandled track mode")
        };
        let default_map: Vec<u8> = (1..layout.sectors[0] as u8 + 1).collect();
        let sector_map: Vec<u8> = match *layout {
            super::names::KAYPROII => (0..10).collect(),
            super::names::KAYPRO4 => match track_num%2 {
                0 => (0..10).collect(),
                _ => (10..20).collect(),
            },
            super::names::TRS80_M2_CPM => match track_num {
                0 => (1..27).collect(),
                _ => (1..17).collect(),
            },
            _ => default_map
        };
        let cylinder_map: Vec<u8> = Vec::new();
        let head_map: Vec<u8> = match *layout {
            super::names::KAYPRO4 => match track_num%2 {
                0 => Vec::new(),
                _ => vec![0;10]
            },
            _ => Vec::new()
        };
        let zone = layout.zone(track_num);
        let mut sector_shift = 0;
        let mut temp = layout.sector_size[zone];
        while temp>128 {
            temp /= 2;
            sector_shift += 1;
        }
        let mut track_buf: Vec<u8> = vec![0;sector_map.len()*(layout.sector_size[zone]+1)];
        for i in 0..sector_map.len() {
            track_buf[i*(layout.sector_size[zone]+1)] = SectorData::Normal as u8;
        }
        let mut head = (track_num % layout.sides[zone]) as u8;
        if cylinder_map.len() > 0 {
            head += 0x80;
        }
        if head_map.len() > 0 {
            head += 0x40;
        }
        Self {
            mode: mode as u8,
            cylinder: (track_num / layout.sides[zone]) as u8,
            head_flags: head,
            sectors: layout.sectors[zone] as u8,
            sector_shift,
            sector_map,
            cylinder_map,
            head_map,
            track_buf,
            head_pos: 0,
            buf_offset: 0
        }
    }
    /// get the byte count of the sector buffer given the sector code
    fn get_sec_buf_size(&self,sector_code: u8) -> usize {
        let sec_size = SECTOR_SIZE_BASE << self.sector_shift;
        match SectorData::from_u8(sector_code) {
            Some(SectorData::None) => 1,
            Some(SectorData::Normal) => 1 + sec_size,
            Some(SectorData::NormalCompressed) => 2,
            Some(SectorData::NormalCompressedDeleted) => 2,
            Some(SectorData::NormalDeleted) => 1 + sec_size,
            Some(SectorData::Error) => 1 + sec_size,
            Some(SectorData::ErrorCompressed) => 2,
            Some(SectorData::ErrorCompressedDeleted) => 2,
            Some(SectorData::ErrorDeleted) => 1 + sec_size,
            _ => panic!("unexpected sector data type")
        }
    }
    /// compress sectors with uniform data
    fn compress(&self) -> Track {
        let mut track_buf: Vec<u8> = Vec::new();
        let mut ptr = 0;
        for isec in 0..self.sectors {
            let sec_size = self.get_sec_buf_size(self.track_buf[ptr]);
            let slice = &self.track_buf[ptr..ptr+sec_size];
            if sec_size > 2 && is_slice_uniform(&slice[1..]) {
                log::trace!("compressing cyl {} head {} sec {}",self.cylinder,self.head_flags & HEAD_MASK,self.sector_map[isec as usize]);
                track_buf.push(slice[0]+1); // adding 1 gives the id of the compressed data
                track_buf.push(slice[1]); // first element is all we need
            } else {
                track_buf.append(&mut slice.to_vec());
            }
            ptr += sec_size;
        }
        Self {
            mode: self.mode,
            cylinder: self.cylinder,
            head_flags: self.head_flags,
            sectors: self.sectors,
            sector_shift: self.sector_shift,
            sector_map: self.sector_map.clone(),
            cylinder_map: self.cylinder_map.clone(),
            head_map: self.head_map.clone(),
            track_buf,
            head_pos: 0,
            buf_offset: 0
        }
    }
    /// expand sectors with uniform data
    fn expand(&self) -> Track {
        let mut track_buf: Vec<u8> = Vec::new();
        let mut ptr = 0;
        for isec in 0..self.sectors {
            let sec_size = self.get_sec_buf_size(self.track_buf[ptr]);
            let slice = &self.track_buf[ptr..ptr+sec_size];
            if sec_size == 2 {
                log::trace!("expanding cyl {} head {} sec {}",self.cylinder,self.head_flags & HEAD_MASK,self.sector_map[isec as usize]);
                track_buf.push(slice[0]-1); // subtracting 1 gives the id of the expanded data
                for _i in 0..(1 << self.sector_shift) {
                    track_buf.append(&mut [slice[1];RECORD_SIZE].to_vec());
                }
            } else {
                track_buf.append(&mut slice.to_vec());
            }
            ptr += sec_size;
        }
        Self {
            mode: self.mode,
            cylinder: self.cylinder,
            head_flags: self.head_flags,
            sectors: self.sectors,
            sector_shift: self.sector_shift,
            sector_map: self.sector_map.clone(),
            cylinder_map: self.cylinder_map.clone(),
            head_map: self.head_map.clone(),
            track_buf,
            head_pos: 0,
            buf_offset: 0
        }
    }
    fn adv_sector(&mut self) -> (usize,usize) {
        self.head_pos += 1;
        if self.head_pos >= self.sector_map.len() {
            self.head_pos = 0;
            self.buf_offset = 0;
        } else {
            self.buf_offset += self.get_sec_buf_size(self.track_buf[self.buf_offset]);
        }
        (self.head_pos,self.buf_offset)
    }
}

impl DiskStruct for Track {
    fn new() -> Self where Self: Sized {
        Self {
            mode: 0,
            cylinder: 0,
            head_flags: 0,
            sectors: 0,
            sector_shift: 0,
            sector_map: Vec::new(),
            cylinder_map: Vec::new(),
            head_map: Vec::new(),
            track_buf: Vec::new(),
            head_pos: 0,
            buf_offset: 0
        }
    }
    fn len(&self) -> usize {
        5 + self.sector_map.len() + self.cylinder_map.len() + self.head_map.len() + self.track_buf.len()
    }
    fn to_bytes(&self) -> Vec<u8> {
        [
            vec![self.mode,self.cylinder,self.head_flags,self.sectors,self.sector_shift],
            self.sector_map.clone(),
            self.cylinder_map.clone(),
            self.head_map.clone(),
            self.track_buf.clone()
        ].concat()
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        let check = |buf: &[u8],min_len: usize| -> Result<(),DiskStructError> {
            if buf.len()<min_len {
                return Err(DiskStructError::OutOfData);
            }
            Ok(())
        };
        check(bytes,5)?;
        self.mode = bytes[0];
        self.cylinder = bytes[1];
        self.head_flags = bytes[2];
        self.sectors = bytes[3];
        self.sector_shift = bytes[4];
        log::debug!("Cylinder {}, Head {}: {} sectors x {} bytes",self.cylinder,self.head_flags & HEAD_MASK,self.sectors,SECTOR_SIZE_BASE << self.sector_shift);
        let mut ptr: usize = 5;
        check(bytes,ptr+self.sectors as usize)?;
        self.sector_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
        log::trace!("sector map {:?}",self.sector_map);
        ptr += self.sectors as usize;
        if self.head_flags & CYL_MAP_FLAG == CYL_MAP_FLAG {
            check(bytes,ptr+self.sectors as usize)?;
            self.cylinder_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
            log::debug!("found cylinder map {:?}",self.cylinder_map);
            ptr += self.sectors as usize;
        } else {
            self.cylinder_map = Vec::new();
        }
        if self.head_flags & HEAD_MAP_FLAG == HEAD_MAP_FLAG {
            check(bytes,ptr+self.sectors as usize)?;
            self.head_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
            log::debug!("found head map {:?}",self.head_map);
            ptr += self.sectors as usize;
        } else {
            self.head_map = Vec::new();
        }
        self.track_buf = Vec::new();
        for _lsec in 0..self.sectors {
            let sec_size = self.get_sec_buf_size(bytes[ptr]);
            check(bytes,ptr+sec_size)?;
            self.track_buf.append(&mut bytes[ptr..ptr+sec_size].to_vec());
            ptr += sec_size;
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Track::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
}

impl Imd {
    pub fn create(kind: img::DiskKind) -> Self {
        let now = chrono::Local::now().naive_local();
        let header = "IMD 1.19: ".to_string() + &now.format("%d-%m-%Y %H:%M:%S").to_string();
        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        log::debug!("header {}",header);
        let (heads,tracks) = match kind {
            img::DiskKind::D3(layout) |
            img::DiskKind::D35(layout) |
            img::DiskKind::D525(layout) |
            img::DiskKind::D8(layout) => {
                let mut ans: Vec<Track> = Vec::new();
                for track in 0..layout.track_count() {
                    ans.push(Track::create(track,&layout));
                }
                (layout.sides(),ans)
            },
            _ => panic!("cannot create this kind of disk in IMD format")
        };
        Self {
            kind,
            heads,
            header: header.as_bytes().try_into().expect("header did not fit"),
            comment: creator_str,
            terminator: 0x1a,
            tracks
        }
    }
    fn get_track_mut(&mut self,cyl: usize,head: usize) -> Result<&mut Track,img::Error> {
        for trk in &mut self.tracks {
            if trk.cylinder as usize==cyl && (trk.head_flags & HEAD_MASK) as usize==head {
                return Ok(trk);
            }
        }
        log::debug!("cannot find cyl {} head {}",cyl,head);
        Err(img::Error::SectorAccess)
    }
    fn check_user_area_up_to_cyl(&self,trk: super::Track,off: u16) -> STDRESULT {
        let [cyl,_] = self.get_rz(trk)?;
        let sectors = self.tracks[off as usize].sectors;
        let sector_shift = self.tracks[off as usize].sector_shift;
        if cyl*self.heads >= self.tracks.len() {
            log::error!("track {} was requested, max is {}",cyl*self.heads,self.tracks.len()-1);
            return Err(Box::new(super::Error::TrackNotFound));
        }
        for i in off as usize..cyl*self.heads+1 {
            let trk = &self.tracks[i];
            if trk.sectors!=sectors || trk.sector_shift!=sector_shift {
                log::warn!("heterogeneous layout in user tracks");
                return Err(Box::new(super::Error::GeometryMismatch));
            }
        }
        Ok(())
    }
    /// Apply the skew transformation for this disk, if the sector discriminant is
    /// an explicit address return an error.
    fn skew(&self,trk: super::Track,sec: super::Sector) -> Result<super::Sector,DYNERR> {
        let [_,head] = self.get_rz(trk)?;
        let table = match (self.kind,head) {
            (super::names::IBM_CPM1_KIND,_) => skew::CPM_1_LSEC_TO_PSEC.to_vec(),
            (super::names::AMSTRAD_SS_KIND,_) => (1..10).collect(),
            (super::DiskKind::D525(IBM_SSDD_9),_) => (1..10).collect(),
            (super::names::OSBORNE1_SD_KIND,_) => skew::CPM_LSEC_TO_OSB1_PSEC.to_vec(),
            (super::names::OSBORNE1_DD_KIND,_) => vec![1,2,3,4,5],
            (super::names::KAYPROII_KIND,_) => (0..10).collect(),
            (super::names::KAYPRO4_KIND,0) => (0..10).collect(),
            (super::names::KAYPRO4_KIND,_) => (10..20).collect(),
            (super::names::TRS80_M2_CPM_KIND,_) => (1..17).collect(),
            (super::names::NABU_CPM_KIND,_) => skew::CPM_LSEC_TO_NABU_PSEC.to_vec(),
            _ => {
                log::warn!("could not find skew table");
                return Err(Box::new(super::Error::DiskKindMismatch))
            }
        };
        match sec {
            super::Sector::Num(n) => Ok(super::Sector::Num(table[n-1] as usize)),
            _ => Err(Box::new(super::Error::BadContext))
        }
    }
    fn seek_sector(&mut self,trk: super::Track,sec: super::Sector) -> Result<usize,DYNERR> {
        let [cyl, head] = self.get_rz(trk)?;
        let track = self.get_track_mut(cyl,head)?;
        let cyl_map = match track.head_flags & CYL_MAP_FLAG > 0 {
            true => track.cylinder_map.clone(),
            false => vec![cyl as u8;track.sectors as usize]
        };
        let head_map = match track.head_flags & HEAD_MAP_FLAG > 0 {
            true => track.head_map.clone(),
            false => vec![head as u8;track.sectors as usize]
        };
        let chs = match sec {
            // seek using internal mappings
            super::Sector::Num(id) => {
                let mut ans = [0xff,0xff,id as u8,0xff];
                for i in 0..track.sector_map.len() {
                    if id as u8 == track.sector_map[i] {
                        ans[0] = cyl_map[i];
                        ans[1] = head_map[i];
                        ans[3] = track.sector_shift;
                    }
                }
                ans
            },
            // seek an explicit CHS provided by the caller
            super::Sector::Addr((_,v)) => {
                if v.len() < 4 {
                    log::error!("address is too short");
                    return Err(Box::new(img::Error::SectorAccess));
                }
                [v[0],v[1],v[2],v[3]]
            }
        };
        log::trace!("seeking sector {:02X}{:02X}{:02X}{:02X}",chs[0],chs[1],chs[2],chs[3]);
        // advance to the requested sector
        for _i in 0..track.sector_map.len() {
            let (sec_idx,buf_idx) = track.adv_sector();
            let curr = [cyl_map[sec_idx], head_map[sec_idx],track.sector_map[sec_idx], track.sector_shift];
            if chs == curr {
                log::trace!("found sector {:02X}{:02X}{:02X}{:02X}",chs[0],chs[1],chs[2],chs[3]);
                return match SectorData::from_u8(track.track_buf[buf_idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) => Ok(buf_idx),
                    Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => Ok(buf_idx),
                    _ => {
                        log::debug!("data type {} not expected",track.track_buf[buf_idx]);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            log::trace!("skip sector {:02X}{:02X}{:02X}{:02X}",curr[0],curr[1],curr[2],curr[3]);
        }
        log::error!("sector {:02X}{:02X}{:02X}{:02X} not found",chs[0],chs[1],chs[2],chs[3]);
        log::debug!("sector map {:?}",track.sector_map);
        Err(Box::new(img::Error::SectorAccess))
    }
}

impl img::DiskImage for Imd {
    fn track_count(&self) -> usize {
        self.tracks.len()
    }
    fn end_track(&self) -> usize {
        match self.tracks.last() {
            Some(track) => track.cylinder as usize * self.heads + (track.head_flags & HEAD_MASK) as usize + 1,
            None => 0
        }
    }
    fn num_heads(&self) -> usize {
        self.heads
    }
    fn nominal_capacity(&self) -> Option<usize> {
        let mut ans = 0;
        let normalized_count = match self.tracks.len() {
            41 => 40,
            81 | 82 => 80,
            161 | 162 => 160,
            c => c
        };
        for i in 0..normalized_count {
            let psec_size = SECTOR_SIZE_BASE << self.tracks[i].sector_shift;
            ans += self.tracks[i].sectors as usize * psec_size;
        }
        Some(ans)
    }
    fn actual_capacity(&mut self) -> Result<usize,DYNERR> {
        let mut ans = 0;
        for trk in &self.tracks {
            let mut idx = 0;
            let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
            for curr in &trk.sector_map {
                //log::trace!("sizing cyl {} head {} sector {}",trk.cylinder,trk.head_flags & HEAD_MASK,curr);
                ans += match SectorData::from_u8(trk.track_buf[idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) => psec_size,
                    Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => psec_size,
                    _ => {
                        log::debug!("cyl {} head {} sector {} is marked unreadable, not counted",trk.cylinder,trk.head_flags & HEAD_MASK,curr);
                        0
                    }
                };
                idx += trk.get_sec_buf_size(trk.track_buf[idx]);
            }
        }
        Ok(ans)
    }
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR> {
        log::trace!("reading {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let secs_per_track = self.tracks[off as usize].sectors;
                let sector_shift = self.tracks[off as usize].sector_shift;
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs((secs_per_track << sector_shift) as usize);
                let ts_list = blocks::cpm::std_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                for (trk,lsec) in ts_list {
                    self.check_user_area_up_to_cyl(trk, off)?;
                    match self.read_sector(trk,self.skew(trk,lsec)?) {
                        Ok(mut slice) => {
                            ans.append(&mut slice);
                        },
                        Err(e) => return Err(e)
                    }
                }
                Ok(ans)
            },
            Block::FAT((_sec1,_secs)) => {
                let secs_per_track = self.tracks[0].sectors;
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs(secs_per_track as usize);
                let ts_list = blocks::fat::std_blocking(deblocked_ts_list,self.heads)?;
                for (trk,lsec) in ts_list {
                    self.check_user_area_up_to_cyl(trk, 0)?;
                    match self.read_sector(trk,lsec) {
                        Ok(mut slice) => {
                            ans.append(&mut slice);
                        },
                        Err(e) => return Err(e)
                    }
                }
                Ok(ans)
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT {
        log::trace!("writing {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let secs_per_track = self.tracks[off as usize].sectors;
                let sector_shift = self.tracks[off as usize].sector_shift;
                let deblocked_ts_list = addr.get_lsecs((secs_per_track << sector_shift) as usize);
                let ts_list = blocks::cpm::std_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                let mut src_offset = 0;
                let psec_size = SECTOR_SIZE_BASE << sector_shift;
                let padded = super::quantize_block(dat, ts_list.len()*psec_size);
                for (trk,lsec) in ts_list {
                    self.check_user_area_up_to_cyl(trk, off)?;
                    match self.write_sector(trk,self.skew(trk,lsec)?,&padded[src_offset..src_offset+psec_size].to_vec()) {
                        Ok(_) => src_offset += SECTOR_SIZE_BASE << sector_shift,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            },
            Block::FAT((_sec1,_secs)) => {
                // TODO: do we need to handle variable sectors per track
                let secs_per_track = self.tracks[0].sectors;
                let sec_size = 128 << self.tracks[0].sector_shift as usize;
                let deblocked_ts_list = addr.get_lsecs(secs_per_track as usize);
                let ts_list = blocks::fat::std_blocking(deblocked_ts_list,self.heads)?;
                let mut src_offset = 0;
                let padded = super::quantize_block(dat, ts_list.len()*sec_size);
                for (trk,lsec) in ts_list {
                    self.check_user_area_up_to_cyl(trk, 0)?;
                    match self.write_sector(trk,lsec,&padded[src_offset..src_offset+sec_size].to_vec()) {
                        Ok(_) => src_offset += sec_size,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn read_sector(&mut self,trk: super::Track,sec: super::Sector) -> Result<Vec<u8>,DYNERR> {
        let buf_idx = self.seek_sector(trk,sec)?;
        let [cyl, head] = self.get_rz(trk)?;
        let trk = self.get_track_mut(cyl,head)?;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        Ok(trk.track_buf[buf_idx+1..buf_idx+1+psec_size].to_vec())
    }
    fn write_sector(&mut self,trk: super::Track,sec: super::Sector,dat: &[u8]) -> STDRESULT {
        let buf_idx = self.seek_sector(trk,sec)?;
        let [cyl, head] = self.get_rz(trk)?;
        let trk = self.get_track_mut(cyl,head)?;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        let padded = super::quantize_block(dat, psec_size);
        trk.track_buf[buf_idx+1..buf_idx+1+psec_size].copy_from_slice(&padded);
        Ok(())
    }
    fn from_bytes(data: &[u8]) -> Result<Self,DiskStructError> {
        if data.len()<29 {
            return Err(DiskStructError::UnexpectedSize);
        }
        let header = data[0..29].to_vec();
        match header[0..6] {
            [73,77,68,32,48,46] => log::info!("identified IMD v0.x header"),
            [73,77,68,32,49,46] => log::info!("identified IMD v1.x header"),
            [73,77,68,32,x,y] => {
                log::warn!("IMD header found but with unknown major version {}.{}...",x-48,y-48);
                return Err(DiskStructError::UnexpectedValue);
            }
            _ => return Err(DiskStructError::UnexpectedValue)
        }
        let mut ptr = 0;
        for i in 29..data.len() {
            if data[i]==0x1a {
                ptr = i;
                break;
            }
        }
        if let Ok(comment) = String::from_utf8(data[29..ptr].to_vec()) {
            let mut ans = Self {
                kind: img::DiskKind::Unknown,
                heads: 1, // updated below
                header: header.try_into().expect("unexpected header mismatch"),
                comment,
                terminator: 0x1a,
                tracks: Vec::new()
            };
            ptr += 1;
            while ptr<data.len() {
                let compressed = Track::from_bytes_adv(&data[ptr..],&mut ptr)?;
                if compressed.sector_shift==0xff {
                    log::warn!("inhomogeneous sector sizes are not supported");
                    return Err(DiskStructError::IllegalValue);
                }
                ans.tracks.push(compressed.expand());
            }
            // TODO: this works for now, but we should have the IMD object set up a pattern
            // that can be explicitly matched against the disk kind.
            match ans.nominal_capacity() {
                Some(cap) => log::debug!("disk capacity {}",cap),
                None => return Err(DiskStructError::UnexpectedValue)
            }
            ans.kind = match (ans.nominal_capacity().unwrap(),ans.tracks[0].sectors) {
                (l,8) if l==DSDD_77.byte_capacity() => img::DiskKind::D8(DSDD_77),
                (l,8) if l==IBM_SSDD_8.byte_capacity() => img::DiskKind::D525(IBM_SSDD_8),
                (l,9) if l==IBM_SSDD_9.byte_capacity() => img::DiskKind::D525(IBM_SSDD_9),
                (l,8) if l==IBM_DSDD_8.byte_capacity() => img::DiskKind::D525(IBM_DSDD_8),
                (l,9) if l==IBM_DSDD_9.byte_capacity() => img::DiskKind::D525(IBM_DSDD_9),
                (l,8) if l==IBM_SSQD.byte_capacity() => img::DiskKind::D525(IBM_SSQD),
                (l,8) if l==IBM_DSQD.byte_capacity() => img::DiskKind::D525(IBM_DSQD),
                (l,15) if l==IBM_DSHD.byte_capacity() => img::DiskKind::D525(IBM_DSHD),
                (l,9) if l==IBM_720.byte_capacity() => img::DiskKind::D35(IBM_720),
                (l,18) if l==IBM_1440.byte_capacity() => img::DiskKind::D35(IBM_1440),
                (l,21) if l==IBM_1680.byte_capacity() => img::DiskKind::D35(IBM_1680),
                (l,21) if l==IBM_1720.byte_capacity() => img::DiskKind::D35(IBM_1720),
                (l,36) if l==IBM_2880.byte_capacity() => img::DiskKind::D35(IBM_2880),
                (256256,26) => img::names::IBM_CPM1_KIND,
                (102400,10) => img::names::OSBORNE1_SD_KIND,
                (184320,9) => img::names::AMSTRAD_SS_KIND,
                (204800,5) => img::names::OSBORNE1_DD_KIND,
                (204800,10) => img::names::KAYPROII_KIND,
                (409600,10) => img::names::KAYPRO4_KIND,
                (625920,26) => img::names::TRS80_M2_CPM_KIND,
                (1018368,26) => img::names::NABU_CPM_KIND,
                _ => img::DiskKind::Unknown
            };
            for trk in &ans.tracks {
                if (trk.head_flags & HEAD_MASK) as usize >= ans.heads {
                    ans.heads = (trk.head_flags & HEAD_MASK) as usize + 1;
                }
            }
            return Ok(ans);
        }
        return Err(DiskStructError::IllegalValue);
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::IMD
    }
    fn file_extensions(&self) -> Vec<String> {
        file_extensions()
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_vec());
        ans.append(&mut self.comment.as_bytes().to_vec());
        ans.push(self.terminator);
        for trk in &self.tracks {
            let compressed = trk.compress();
            ans.append(&mut compressed.to_bytes());
        }
        return ans;
    }
    fn get_track_buf(&mut self,_trk: super::Track) -> Result<Vec<u8>,DYNERR> {
        log::error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn set_track_buf(&mut self,_trk: super::Track,_dat: &[u8]) -> STDRESULT {
        log::error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_solution(&mut self,trk: super::Track) -> Result<img::TrackSolution,DYNERR> {
        let trk = self.get_track(trk)?;
        let trk_obj = &self.tracks[trk];
        let (flux_code,speed_kbps) = match Mode::from_u8(trk_obj.mode) {
            Some(Mode::Fm250Kbps) => (img::FluxCode::FM,250),
            Some(Mode::Fm300Kbps) => (img::FluxCode::FM,300),
            Some(Mode::Fm500Kbps) => (img::FluxCode::FM,500),
            Some(Mode::Mfm250Kbps) => (img::FluxCode::MFM,250),
            Some(Mode::Mfm300Kbps) => (img::FluxCode::MFM,300),
            Some(Mode::Mfm500Kbps) => (img::FluxCode::MFM,500),
            None => (img::FluxCode::None,0)
        };
        let phys_head = (trk_obj.head_flags & HEAD_MASK) as usize;
        let mut addr_map: Vec<[u8;5]> = Vec::new();
        for i in 0..trk_obj.sectors as usize {
            let c = match trk_obj.cylinder_map.len()>i { true=>trk_obj.cylinder_map[i] as usize, false=>trk_obj.cylinder as usize };
            let h = match trk_obj.head_map.len()>i { true=>trk_obj.head_map[i] as usize, false=>phys_head };
            addr_map.push([c.try_into()?,h.try_into()?,trk_obj.sector_map[i],trk_obj.sector_shift,0]);
        }
        Ok(img::TrackSolution {
            cylinder: trk_obj.cylinder as usize,
            fraction: [0,1],
            head: phys_head,
            speed_kbps,
            flux_code,
            addr_code: img::FieldCode::None,
            data_code: img::FieldCode::None,
            addr_type: "CHSFK".to_string(),
            addr_mask: [255,255,255,0,0],
            addr_map,
            size_map: vec![SECTOR_SIZE_BASE << trk_obj.sector_shift;trk_obj.sectors as usize]
        })
    }
    fn get_track_nibbles(&mut self,_trk: super::Track) -> Result<Vec<u8>,DYNERR> {
        log::error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &[u8]) -> String {
        String::from("IMD images have no track bits to display")
    }
    fn get_metadata(&self,indent: Option<u16>) -> String {
        let imd = self.what_am_i().to_string();
        let mut root = json::JsonValue::new_object();
        root[&imd] = json::JsonValue::new_object();
        root[&imd]["header"] = json::JsonValue::String(String::from_utf8_lossy(&self.header).into());
        root[&imd]["comment"] = json::JsonValue::String(self.comment.clone());
        if let Some(spaces) = indent {
            json::stringify_pretty(root,spaces)
        } else {
            json::stringify(root)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            meta::test_metadata(key_path, self.what_am_i())?;
            let imd = self.what_am_i().to_string();
            if meta::match_key(key_path,&[&imd,"header"]) {
                log::warn!("skipping read-only `header`");
                return Ok(())
            }
            putString!(val,key_path,imd,self.comment);
        }
        log::error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}