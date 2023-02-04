//! ## Support for IMD disk images
//!
//! Although this is not an Apple II format, it is included because a lot of
//! CP/M disk images are available in this format.  By supporting the IMD format
//! we allow, for instance, transfer of files from 8 inch IBM disks to A2 disks.
//! 
//! We cannot read an arbitrary CP/M disk in IMD format.  The problem is, to do so,
//! we need the DPB and skew tables for every kind of disk we wish to support.

use chrono;
use num_traits::FromPrimitive;
use num_derive::FromPrimitive;
use log::{warn,info,trace,debug,error};
use a2kit_macro::DiskStruct;
use crate::fs::cpm::types::RECORD_SIZE;
use crate::img;
use crate::bios::skew;
use crate::fs::Block;
use crate::{STDRESULT,DYNERR};

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

/// Take a logical track-sector list and produce a hybrid cylinder-head-sector list.
/// Hybrid means the sector order is logical while the size is physical.
/// Remember CP/M logical sectors are numbered from 1.
/// This assumes the mapping track = cyl*heads + head.
pub fn cpm_blocking(ts_list: Vec<[usize;2]>,sec_shift: u8,heads: usize) -> Result<Vec<[usize;3]>,img::Error> {
    trace!("ts list {:?} (logical deblocked)",ts_list);
    if (ts_list.len() % (1 << sec_shift) != 0) || ((ts_list[0][1]-1) % (1 << sec_shift) != 0) {
        info!("CP/M blocking was misaligned, start {}, length {}",ts_list[0][1],ts_list.len());
        return Err(img::Error::SectorAccess);
    }
    if heads<1 {
        error!("CP/M blocking was passed 0 heads");
        return Err(img::Error::SectorAccess);
    }
    let mut ans: Vec<[usize;3]> = Vec::new();
    let mut track = 0;
    for i in 0..ts_list.len() {
        let lsec = ts_list[i][1];
        if (lsec-1)%(1<<sec_shift) == 0 {
            track = ts_list[i][0];
        }
        if lsec%(1<<sec_shift) == 0 {
            let cyl = track/heads;
            let head = match heads { 1 => 0, _ => track%heads };
            ans.push([cyl,head,1+(lsec-1)/(1<<sec_shift)]);
        } else if ts_list[i][0]!=track {
            info!("CP/M blocking failed, sector crossed track {}",track);
            return Err(img::Error::SectorAccess);
        }
    }
    trace!("ts list {:?} (logical blocked)",ans);
    return Ok(ans);
}

pub struct Track {
    mode: u8,
    cylinder: u8,
    head: u8,
    sectors: u8,
    sector_shift: u8,
    /// order is not important (maybe geometrical), value is physical sector address
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
        let mode = match *layout {
            super::names::CPM_1 => Mode::Fm500Kbps as u8,
            super::names::KAYPROII => Mode::Fm250Kbps as u8,
            super::names::KAYPRO4 => Mode::Mfm250Kbps as u8,
            super::names::OSBORNE1_SD => Mode::Fm250Kbps as u8,
            super::names::OSBORNE1_DD => Mode::Mfm250Kbps as u8,
            super::names::TRS80_M2_CPM => match track_num {
                0 => Mode::Fm500Kbps as u8,
                _ => Mode::Mfm500Kbps as u8
            },
            super::names::NABU_CPM => match track_num {
                0 => Mode::Fm500Kbps as u8,
                1 => Mode::Fm500Kbps as u8,
                _ => Mode::Mfm500Kbps as u8
            },
            _ => panic!("unhandled track layout")
        };
        let sector_map: Vec<u8> = match *layout {
            super::names::CPM_1 => (1..27).collect(),
            super::names::KAYPROII => (0..10).collect(),
            super::names::KAYPRO4 => match track_num%2 {
                0 => (0..10).collect(),
                _ => (10..20).collect(),
            },
            super::names::OSBORNE1_SD => (1..11).collect(),
            super::names::OSBORNE1_DD => [1,2,3,4,5].to_vec(),
            super::names::TRS80_M2_CPM => match track_num {
                0 => (1..27).collect(),
                _ => (1..17).collect(),
            },
            super::names::NABU_CPM => (1..27).collect(),
            _ => panic!("unhandled track layout")
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
            mode,
            cylinder: (track_num / layout.sides[zone]) as u8,
            head,
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
                trace!("compressing cyl {} head {} sec {}",self.cylinder,self.head & HEAD_MASK,self.sector_map[isec as usize]);
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
            head: self.head,
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
                trace!("expanding cyl {} head {} sec {}",self.cylinder,self.head & HEAD_MASK,self.sector_map[isec as usize]);
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
            head: self.head,
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
            head: 0,
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
            vec![self.mode,self.cylinder,self.head,self.sectors,self.sector_shift],
            self.sector_map.clone(),
            self.cylinder_map.clone(),
            self.head_map.clone(),
            self.track_buf.clone()
        ].concat()
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        let check = |buf: &Vec<u8>,min_len: usize| {
            if buf.len()<min_len {
                error!("unexpected end of data at {}",buf.len());
                panic!("cannot form IMD");
            }
        };
        check(bytes,5);
        self.mode = bytes[0];
        self.cylinder = bytes[1];
        self.head = bytes[2];
        self.sectors = bytes[3];
        self.sector_shift = bytes[4];
        debug!("Cylinder {}, Head {}: {} sectors x {} bytes",self.cylinder,self.head & HEAD_MASK,self.sectors,SECTOR_SIZE_BASE << self.sector_shift);
        let mut ptr: usize = 5;
        check(bytes,ptr+self.sectors as usize);
        self.sector_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
        trace!("sector map {:?}",self.sector_map);
        ptr += self.sectors as usize;
        if self.head & CYL_MAP_FLAG == CYL_MAP_FLAG {
            check(bytes,ptr+self.sectors as usize);
            self.cylinder_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
            debug!("found cylinder map {:?}",self.cylinder_map);
            ptr += self.sectors as usize;
        } else {
            self.cylinder_map = Vec::new();
        }
        if self.head & HEAD_MAP_FLAG == HEAD_MAP_FLAG {
            check(bytes,ptr+self.sectors as usize);
            self.head_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
            debug!("found head map {:?}",self.head_map);
            ptr += self.sectors as usize;
        } else {
            self.head_map = Vec::new();
        }
        self.track_buf = Vec::new();
        for _lsec in 0..self.sectors {
            let sec_size = self.get_sec_buf_size(bytes[ptr]);
            check(bytes,ptr+sec_size);
            self.track_buf.append(&mut bytes[ptr..ptr+sec_size].to_vec());
            ptr += sec_size;
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Track::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
}

impl Imd {
    pub fn create(kind: img::DiskKind) -> Self {
        let now = chrono::Local::now().naive_local();
        let header = "IMD 1.19: ".to_string() + &now.format("%d-%m-%Y %H:%M:%S").to_string();
        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        debug!("header {}",header);
        let (heads,tracks) = match kind {
            img::DiskKind::D525(layout) | img::DiskKind::D8(layout) => {
                let mut ans: Vec<Track> = Vec::new();
                for track in 0..layout.track_count() {
                    ans.push(Track::create(track,&layout));
                }
                (layout.sides(),ans)
            }
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
    pub fn num_heads(&self) -> usize {
        self.heads
    }
    fn get_track_mut(&mut self,cyl: usize,head: usize) -> Result<&mut Track,img::Error> {
        for trk in &mut self.tracks {
            if trk.cylinder as usize==cyl && (trk.head & HEAD_MASK) as usize==head {
                return Ok(trk);
            }
        }
        debug!("cannot find cyl {} head {}",cyl,head);
        Err(img::Error::SectorAccess)
    }
    fn check_user_area_up_to_cyl(&self,cyl: usize,off: u16) -> STDRESULT {
        let sectors = self.tracks[off as usize].sectors;
        let sector_shift = self.tracks[off as usize].sector_shift;
        for i in off as usize..cyl*self.heads+1 {
            let trk = &self.tracks[i];
            if trk.sectors!=sectors || trk.sector_shift!=sector_shift {
                warn!("heterogeneous layout in user tracks");
                return Err(Box::new(super::Error::ImageTypeMismatch));
            }
        }
        Ok(())
    }
    fn get_skew(&self,head: usize) -> Result<Vec<u8>,DYNERR> {
        match (self.kind,head) {
            (super::names::IBM_CPM1_KIND,_) => Ok(skew::CPM_1_LSEC_TO_PSEC.to_vec()),
            (super::names::OSBORNE1_SD_KIND,_) => Ok(skew::CPM_LSEC_TO_OSB1_PSEC.to_vec()),
            (super::names::OSBORNE1_DD_KIND,_) => Ok(vec![1,2,3,4,5]),
            (super::names::KAYPROII_KIND,_) => Ok((0..10).collect()),
            (super::names::KAYPRO4_KIND,0) => Ok((0..10).collect()),
            (super::names::KAYPRO4_KIND,_) => Ok((10..20).collect()),
            (super::names::TRS80_M2_CPM_KIND,_) => Ok((1..17).collect()),
            (super::names::NABU_CPM_KIND,_) => Ok(skew::CPM_LSEC_TO_NABU_PSEC.to_vec()),
            _ => {
                warn!("could not find skew table");
                return Err(Box::new(super::Error::ImageTypeMismatch))
            }
        }
    }
}

impl img::DiskImage for Imd {
    fn track_count(&self) -> usize {
        self.tracks.len()
    }
    fn byte_capacity(&self) -> usize {
        let mut ans = 0;
        for trk in &self.tracks {
            let mut idx = 0;
            let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
            for curr in &trk.sector_map {
                //trace!("sizing cyl {} head {} sector {}",trk.cylinder,trk.head & HEAD_MASK,curr);
                ans += match SectorData::from_u8(trk.track_buf[idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) => psec_size,
                    Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => psec_size,
                    _ => {
                        debug!("cyl {} head {} sector {} is marked unreadable, not counted",trk.cylinder,trk.head & HEAD_MASK,curr);
                        0
                    }
                };
                idx += trk.get_sec_buf_size(trk.track_buf[idx]);
            }
        }
        ans
    }
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR> {
        trace!("reading {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let sectors = self.tracks[off as usize].sectors;
                let sector_shift = self.tracks[off as usize].sector_shift;
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs((sectors << sector_shift) as usize);
                let chs_list = cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                for [cyl,head,lsec] in chs_list {
                    self.check_user_area_up_to_cyl(cyl, off)?;
                    let skew_table = self.get_skew(head)?;
                    match self.read_sector(cyl,head,skew_table[lsec-1] as usize) {
                        Ok(mut slice) => {
                            ans.append(&mut slice);
                        },
                        Err(e) => return Err(e)
                    }
                }
                Ok(ans)
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT {
        trace!("writing {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let sectors = self.tracks[off as usize].sectors;
                let sector_shift = self.tracks[off as usize].sector_shift;
                let deblocked_ts_list = addr.get_lsecs((sectors << sector_shift) as usize);
                let chs_list = cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                let mut src_offset = 0;
                let psec_size = SECTOR_SIZE_BASE << sector_shift;
                let padded = super::quantize_block(dat, chs_list.len()*psec_size);
                for [cyl,head,lsec] in chs_list {
                    self.check_user_area_up_to_cyl(cyl, off)?;
                    let skew_table = self.get_skew(head)?;
                    match self.write_sector(cyl,head,skew_table[lsec-1] as usize,&padded[src_offset..src_offset+psec_size].to_vec()) {
                        Ok(_) => src_offset += SECTOR_SIZE_BASE << sector_shift,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        trace!("seeking sector {} (R)",sec);
        let trk = self.get_track_mut(cyl,head)?;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        // advance to the requested sector
        for _i in 0..trk.sector_map.len() {
            let (sec_idx,buf_idx) = trk.adv_sector();
            let curr = trk.sector_map[sec_idx] as usize;
            if sec==curr {
                trace!("reading sector {}",sec);
                return match SectorData::from_u8(trk.track_buf[buf_idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) => Ok(trk.track_buf[buf_idx+1..buf_idx+1+psec_size].to_vec()),
                    Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => Ok(trk.track_buf[buf_idx+1..buf_idx+1+psec_size].to_vec()),
                    _ => {
                        debug!("cyl {} head {} sector {}: data type {} not expected",cyl,head,sec,trk.track_buf[buf_idx]);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            trace!("skip sector {}",curr);
        }
        error!("sector {} not found",sec);
        debug!("sector map {:?}",trk.sector_map);
        Err(Box::new(img::Error::SectorAccess))
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        trace!("seeking sector {} (W)",sec);
        let trk = self.get_track_mut(cyl,head)?;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        let padded = super::quantize_block(dat, psec_size);
        // advance to the requested sector
        for _i in 0..trk.sector_map.len() {
            let (sec_idx,buf_idx) = trk.adv_sector();
            let curr = trk.sector_map[sec_idx] as usize;
            if sec==curr {
                trace!("writing sector {}",sec);
                return match SectorData::from_u8(trk.track_buf[buf_idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) | Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => {
                        trk.track_buf[buf_idx+1..buf_idx+1+psec_size].copy_from_slice(&padded);
                        Ok(())
                    },
                    _ => {
                        debug!("cyl {} head {} sector {}: data type {} not expected",cyl,head,sec,trk.track_buf[buf_idx]);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            trace!("skip sector {}",curr);
        }
        error!("sector {} not found",sec);
        Err(Box::new(img::Error::SectorAccess))
    }
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        if data.len()<29 {
            return None;
        }
        let header = data[0..29].to_vec();
        match header[0..6] {
            [73,77,68,32,48,46] => info!("identified IMD v0.x header"),
            [73,77,68,32,49,46] => info!("identified IMD v1.x header"),
            [73,77,68,32,x,y] => {
                warn!("IMD header found but with unknown major version {}.{}...",x-48,y-48);
                return None;
            }
            _ => return None
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
                let compressed = Track::from_bytes(&data[ptr..].to_vec());
                if compressed.sector_shift==0xff {
                    warn!("inhomogeneous sector sizes are not supported");
                    return None;
                }
                ptr += compressed.len();
                ans.tracks.push(compressed.expand());
            }
            // TODO: this works for now, but we should have the IMD object set up a pattern
            // that can be explicitly matched against the disk kind.
            ans.kind = match (ans.byte_capacity(),ans.tracks[0].sectors) {
                (256256,26) => img::names::IBM_CPM1_KIND,
                (102400,10) => img::names::OSBORNE1_SD_KIND,
                (204800,5) => img::names::OSBORNE1_DD_KIND,
                (204800,10) => img::names::KAYPROII_KIND,
                (409600,10) => img::names::KAYPRO4_KIND,
                (625920,26) => img::names::TRS80_M2_CPM_KIND,
                (1018368,26) => img::names::NABU_CPM_KIND,
                _ => img::DiskKind::Unknown
            };
            for trk in &ans.tracks {
                if (trk.head & HEAD_MASK) as usize >= ans.heads {
                    ans.heads = (trk.head & HEAD_MASK) as usize + 1;
                }
            }
            return Some(ans);
        }
        return None;
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
    fn get_track_buf(&mut self,_cyl: usize,_head: usize) -> Result<Vec<u8>,DYNERR> {
        error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn set_track_buf(&mut self,_cyl: usize,_head: usize,_dat: &[u8]) -> STDRESULT {
        error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_nibbles(&mut self,_cyl: usize,_head: usize) -> Result<Vec<u8>,DYNERR> {
        error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &[u8]) -> String {
        String::from("IMD images have no track bits to display")
    }
}