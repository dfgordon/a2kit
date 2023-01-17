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
use crate::fs::Chunk;

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
/// This assumes the mapping track = cyl*heads + head.
pub fn cpm_blocking(ts_list: Vec<[usize;2]>,sec_shift: u8,heads: usize) -> Result<Vec<[usize;3]>,img::Error> {
    trace!("ts list {:?} (logical deblocked)",ts_list);
    if (ts_list.len() % (1 << sec_shift) != 0) || (ts_list[0][1] % (1 << sec_shift) != 0) {
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
        if lsec%(1<<sec_shift) == 0 {
            track = ts_list[i][0];
        }
        if (lsec+1)%(1<<sec_shift) == 0 {
            let cyl = track/heads;
            let head = match heads { 1 => 0, _ => track%heads };
            ans.push([cyl,head,lsec/(1<<sec_shift)]);
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
    track_buf: Vec<u8>
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
                trace!("compressing sector at index {}",isec);
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
            track_buf
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
                trace!("expanding sector at index {}",isec);
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
            track_buf
        }
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
            track_buf: Vec::new()
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
        debug!("Cylinder {}, Head {}: {} sectors x {} bytes",self.cylinder,self.head,self.sectors,SECTOR_SIZE_BASE << self.sector_shift);
        let mut ptr: usize = 5;
        check(bytes,ptr+self.sectors as usize);
        self.sector_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
        trace!("sector map {:?}",self.sector_map);
        ptr += self.sectors as usize;
        if self.head & CYL_MAP_FLAG == CYL_MAP_FLAG {
            debug!("track has cylinder map");
            check(bytes,ptr+self.sectors as usize);
            self.cylinder_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
            ptr += self.sectors as usize;
        } else {
            self.cylinder_map = Vec::new();
        }
        if self.head & HEAD_MAP_FLAG == HEAD_MAP_FLAG {
            debug!("track has head map");
            check(bytes,ptr+self.sectors as usize);
            self.head_map = bytes[ptr..ptr+self.sectors as usize].to_vec();
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
        let tracks = match kind {
            img::names::IBM_CPM1_KIND => {
                let mut ans: Vec<Track> = Vec::new();
                let mut normal_track_buf: Vec<u8> = vec![0;129*26];
                for lsec in 0..26 {
                    normal_track_buf[lsec*129] = SectorData::Normal as u8;
                }
                for track in 0..77 {
                    let trk = Track {
                        mode: Mode::Fm250Kbps as u8,
                        cylinder: track,
                        head: 0,
                        sectors: 26,
                        sector_shift: 0,
                        sector_map: [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26].to_vec(),
                        cylinder_map: vec![],
                        head_map: vec![],
                        track_buf: normal_track_buf.clone()
                    };
                    ans.push(trk);
                }
                ans
            },
            img::names::OSBORNE_KIND => {
                let mut ans: Vec<Track> = Vec::new();
                let mut normal_track_buf: Vec<u8> = vec![0;1025*5];
                for lsec in 0..5 {
                    normal_track_buf[lsec*1025] = SectorData::Normal as u8;
                }
                for track in 0..40 {
                    let trk = Track {
                        mode: Mode::Mfm300Kbps as u8,
                        cylinder: track,
                        head: 0,
                        sectors: 5,
                        sector_shift: 3,
                        sector_map: [1,2,3,4,5].to_vec(),
                        cylinder_map: vec![],
                        head_map: vec![],
                        track_buf: normal_track_buf.clone()
                    };
                    ans.push(trk);
                }
                ans
            },
            _ => panic!("cannot create this kind of disk in IMD format")
        };
        Self {
            kind,
            heads: 1,
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
            if trk.cylinder as usize==cyl && trk.head as usize==head {
                return Ok(trk);
            }
        }
        debug!("cannot find cyl {} head {}",cyl,head);
        Err(img::Error::SectorAccess)
    }
    fn get_track(&self,cyl: usize,head: usize) -> Result<&Track,img::Error> {
        for trk in &self.tracks {
            if trk.cylinder as usize==cyl && trk.head as usize==head {
                return Ok(trk);
            }
        }
        debug!("cannot find cyl {} head {}",cyl,head);
        Err(img::Error::SectorAccess)
    }
}

impl img::DiskImage for Imd {
    fn track_count(&self) -> usize {
        self.tracks.len()
    }
    fn byte_capacity(&self) -> usize {
        let mut ans = 0;
        for trk in &self.tracks {
            ans += trk.sectors as usize * (SECTOR_SIZE_BASE << trk.sector_shift as usize);
        }
        ans
    }
    fn read_chunk(&self,addr: Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        trace!("reading {}",addr);
        match addr {
            Chunk::CPM((_block,_bsh,_off)) => {
                let sectors = self.tracks[0].sectors;
                let sector_shift = self.tracks[0].sector_shift;
                for trk in &self.tracks {
                    if trk.sectors!=sectors || trk.sector_shift!=sector_shift {
                        warn!("cannot handle variable sector layout");
                        return Err(Box::new(super::Error::ImageTypeMismatch));
                    }
                }
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs((sectors << sector_shift) as usize);
                let chs_list = cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                for [cyl,head,lsec] in chs_list {
                    let skew_table = match self.kind() {
                        super::names::IBM_CPM1_KIND => skew::CPM_1_LSEC_TO_PSEC.to_vec(),
                        super::names::OSBORNE_KIND => vec![1,2,3,4,5],
                        _ => return Err(Box::new(super::Error::ImageTypeMismatch))
                    };
                    match self.read_sector(cyl,head,skew_table[lsec] as usize) {
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
    fn write_chunk(&mut self, addr: Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        trace!("writing {}",addr);
        match addr {
            Chunk::CPM((_block,_bsh,_off)) => {
                let sectors = self.tracks[0].sectors;
                let sector_shift = self.tracks[0].sector_shift;
                for trk in &self.tracks {
                    if trk.sectors!=sectors || trk.sector_shift!=sector_shift {
                        warn!("cannot handle variable sector layout");
                        return Err(Box::new(super::Error::ImageTypeMismatch));
                    }
                }
                let deblocked_ts_list = addr.get_lsecs((sectors << sector_shift) as usize);
                let chs_list = cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
                let mut src_offset = 0;
                let psec_size = SECTOR_SIZE_BASE << sector_shift;
                let padded = super::quantize_chunk(dat, chs_list.len()*psec_size);
                for [cyl,head,lsec] in chs_list {
                    let skew_table = match self.kind() {
                        super::names::IBM_CPM1_KIND => skew::CPM_1_LSEC_TO_PSEC.to_vec(),
                        super::names::OSBORNE_KIND => vec![1,2,3,4,5],
                        _ => return Err(Box::new(super::Error::ImageTypeMismatch))
                    };
                    match self.write_sector(cyl,head,skew_table[lsec] as usize,&padded[src_offset..src_offset+psec_size].to_vec()) {
                        Ok(_) => src_offset += SECTOR_SIZE_BASE << sector_shift,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn read_sector(&self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let trk = self.get_track(cyl,head)?;
        let mut idx = 0;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        // advance to the requested sector
        for curr in &trk.sector_map {
            if sec==*curr as usize {
                trace!("found sector {}",sec);
                return match SectorData::from_u8(trk.track_buf[idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) => Ok(trk.track_buf[idx+1..idx+1+psec_size].to_vec()),
                    Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => Ok(trk.track_buf[idx+1..idx+1+psec_size].to_vec()),
                    _ => {
                        debug!("cyl {} head {} sector {}: data type {} not expected",cyl,head,sec,trk.track_buf[idx]);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            idx += trk.get_sec_buf_size(trk.track_buf[idx]);
            trace!("seeking sector {}, found sector {}",sec,curr);
        }
        error!("sector {} not found",sec);
        debug!("sector map {:?}",trk.sector_map);
        Err(Box::new(img::Error::SectorAccess))
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        let trk = self.get_track_mut(cyl,head)?;
        let mut idx = 0;
        let psec_size = SECTOR_SIZE_BASE << trk.sector_shift;
        let padded = super::quantize_chunk(dat, psec_size);
        // advance to the requested sector
        for curr in &trk.sector_map {
            if sec==*curr as usize {
                trace!("found sector {}",sec);
                return match SectorData::from_u8(trk.track_buf[idx]) {
                    Some(SectorData::Normal) | Some(SectorData::NormalDeleted) | Some(SectorData::Error) | Some(SectorData::ErrorDeleted) => {
                        trk.track_buf[idx+1..idx+1+psec_size].copy_from_slice(&padded);
                        Ok(())
                    },
                    _ => {
                        debug!("cyl {} head {} sector {}: data type {} not expected",cyl,head,sec,trk.track_buf[idx]);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            idx += trk.get_sec_buf_size(trk.track_buf[idx]);
            trace!("seeking sector {}, found sector {}",sec,curr);
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
            ans.kind = match ans.byte_capacity() {
                256256 => img::names::IBM_CPM1_KIND,
                204800 => img::names::OSBORNE_KIND,
                _ => img::DiskKind::Unknown
            };
            for trk in &ans.tracks {
                if trk.head as usize >= ans.heads {
                    ans.heads = trk.head as usize + 1;
                }
            }
            return Some(ans);
        }
        return None;
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::IMD
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn to_bytes(&self) -> Vec<u8> {
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
    fn get_track_buf(&self,_cyl: usize,_head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_nibbles(&self,_cyl: usize,_head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("IMD images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &Vec<u8>) -> String {
        String::from("IMD images have no track bits to display")
    }
}