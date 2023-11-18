//! ## Support for TD0 disk images
//!
//! The assumptions herein are largely based on Dave Dunfield's notes found in the
//! ImageDisk package.  This uses the `retrocompressor` crate to handle advanced TD0 compression.
//! As of this writing the creators of the TD0 format have never revealed its details.

use chrono::Timelike;
use num_traits::FromPrimitive;
use num_derive::FromPrimitive;
use log::{warn,info,trace,debug,error};
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use retrocompressor;
use crate::img;
use crate::img::meta;
use crate::img::names::*;
use crate::bios::skew;
use crate::fs::Block;
use crate::{STDRESULT,DYNERR,getByte,putByte,getByteEx};

macro_rules! verified_get_byte {
    ($slf:ident.$ibuf:ident,$ptr:ident,$loc:expr) => {
        match $ptr < $slf.$ibuf.len() {
            true => {
                $ptr += 1;
                $slf.$ibuf[$ptr-1]
            },
            false => {
                debug!("out of data in {}",$loc);
                return Err(Box::new(super::Error::SectorAccess));
            }
        }
    };
}

macro_rules! verified_get_slice {
    ($slf:ident.$ibuf:ident,$ptr:ident,$len:expr,$loc:expr) => {
        match $ptr + $len <= $slf.$ibuf.len() {
            true => {
                $ptr += $len;
                &$slf.$ibuf[$ptr-$len..$ptr]
            },
            false => {
                debug!("out of data in {}",$loc);
                return Err(Box::new(super::Error::SectorAccess));
            }
        }
    };
}

macro_rules! optional_get_slice {
    ($ibuf:ident,$ptr:ident,$len:expr,$loc:expr) => {
        match $ptr + $len <= $ibuf.len() {
            true => {
                $ptr += $len;
                &$ibuf[$ptr-$len..$ptr]
            },
            false => {
                debug!("out of data in {}",$loc);
                return None;
            }
        }
    };
}

/// from Dunfield's notes, never used
pub enum DriveType {
    D525in96tpi48tpi = 0x00,
    D525in360k = 0x01,
    D525in1200k = 0x02,
    D35in720k = 0x03,
    D35in1440k = 0x04,
    D8in = 0x05,
    D35in = 0x06
}

#[derive(FromPrimitive)]
pub enum SectorEncoding {
    Raw = 0,
    Repeated = 1,
    RunLength = 2
}

/// high bit indicates presence of comment block
pub enum Stepping {
    Single = 0x00,
    Double = 0x01,
    Even = 0x02
}

pub const SECTOR_SIZE_BASE: usize = 128;

const HEAD_MASK: u8 = 0x01;
const NO_DATA_MASK: u8 = 0x30;
const RATE_MASK: u8 = 0x03;
const FM_MASK: u8 = 0x80;
const STEPPING_MASK: u8 = 0x03;
const COMMENT_MASK: u8 = 0x80;

// const FLAG_DUP_SEC: u8 = 0x01;
// const FLAG_CRC_ERR: u8 = 0x02;
// const FLAG_DEL_DAT: u8 = 0x04;
// const FLAG_SKIPPED: u8 = 0x10;
// const FLAG_NO_DAT: u8 = 0x20;
// const FLAG_NO_ID: u8 = 0x40;

pub fn file_extensions() -> Vec<String> {
    vec!["td0".to_string()]
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

/// Calculate the checksum for the TD0 data in `buf`
pub fn crc16(crc_seed: u16, buf: &[u8]) -> u16
{
    let mut crc: u16 = crc_seed;
    for i in 0..buf.len() {
        crc ^= (buf[i] as u16) << 8;
        for _bit in 0..8 {
            crc = (crc << 1) ^ match crc & 0x8000 { 0 => 0, _ => 0xa097 };
        }
    }
    crc
}

#[derive(DiskStruct)]
pub struct ImageHeader {
    signature: [u8;2],
    sequence: u8, // usually 0, could increment for each disk in a set
    check_sequence: u8, // id for a set of disks?
    version: u8, // major version in high nibble, minor version in low nibble
    data_rate: u8, // 0=250kpbs,1=300kpbs,2=500kpbs, high bit off=MFM, high bit on=FM
    drive_type: u8, // see DriveType enum
    stepping: u8, // see Stepping enum, high bit indicates comment block
    dos_alloc_flag: u8, // if >0 DOS FAT table used to skip unallocated sectors
    sides: u8, // 1 => 1, not 1 => 2
    crc: [u8;2]
}

#[derive(DiskStruct)]
pub struct CommentHeader {
    crc: [u8;2],
    data_length: [u8;2],
    timestamp: [u8;6] // bytes: year since 1900,month 0-11, day 1-31, hour 0-23, minute, second
}

#[derive(DiskStruct)]
pub struct SectorHeader {
    cylinder: u8, // as encoded in the sector
    head: u8, // as encoded in the sector
    id: u8, // logical sector number
    sector_shift: u8, // length = 2^(7+sector_shift) 
    flags: u8, // see FLAG family of constants, 0x10 or 0x20 means no sector data follows
    crc: u8, // lower byte
}

#[derive(DiskStruct)]
pub struct TrackHeader {
    sectors: u8,
    cylinder: u8, // from 0
    head: u8, // 0 or 1 if track is MFM, 0x80 or 0x81 if track is FM
    crc: u8, // lower byte
}

pub struct Sector {
    header: SectorHeader,
    /// decode with Sector::unpack, encode with Sector::pack
    data: Vec<u8>
}

pub struct Track {
    header: TrackHeader,
    sectors: Vec<Sector>,
    /// extensions (not part of TD0 file)
    head_pos: usize,
}

pub struct Td0 {
    kind: img::DiskKind,
    heads: usize,
    header: ImageHeader,
    comment_header: Option<CommentHeader>,
    comment_data: Option<String>, // when flattening, newlines should be replaced by nulls
    tracks: Vec<Track>,
    end: u8 // 0xff
}

impl CommentHeader {
    fn pack_timestamp(maybe_time: Option<chrono::NaiveDateTime>) -> [u8;6] {
        let now = match maybe_time {
            Some(time) => time,
            _ => chrono::Local::now().naive_local()
        };
        let mut year = u32::from_str_radix(&now.format("%Y").to_string(),10).expect("date error");
        let month = u8::from_str_radix(&now.format("%m").to_string(),10).expect("date error");
        let day = u8::from_str_radix(&now.format("%d").to_string(),10).expect("date error");
        if year - 1900 > u8::MAX as u32 {
            warn!("timestamp is pegged at {} years after reference date",u8::MAX);
            year = 1900 + u8::MAX as u32;
        }
        if year < 1900 {
            warn!("year prior to reference date, pegging to reference date");
            year = 1900;
        }
        [(year-1900) as u8,month,day,now.hour() as u8,now.minute() as u8,now.second()as u8]
    }
    fn unpack_timestamp(&self) -> Option<chrono::NaiveDateTime> {
        match chrono::NaiveDate::from_ymd_opt(1900+self.timestamp[0] as i32,
            self.timestamp[1] as u32, self.timestamp[2] as u32) {
            Some(d) => d.and_hms_opt(self.timestamp[3] as u32,self.timestamp[4] as u32,self.timestamp[5] as u32),
            None => None
        }
    }
    fn pretty_timestamp(&self) -> String {
        match self.unpack_timestamp() {
            Some(ts) => ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => String::from("could not unpack")
        }
    }
}

impl Sector {
    /// Create the sector structure
    fn create(cylinder: u8,head: u8,id: u8,byte_count: usize) -> Self {
        match byte_count {
            128 | 256 | 512 | 1024 | 2048 | 4096 | 8192 => {},
            _ => panic!("sector size {} not allowed",byte_count)
        }
        let mut sector_shift = 0;
        let mut temp = byte_count;
        while temp > SECTOR_SIZE_BASE {
            temp /= 2;
            sector_shift += 1;
        }
        let header = SectorHeader {
            cylinder,
            head,
            id,
            sector_shift,
            flags: 0,
            crc: 0
        };
        let data = [
            vec![5,0], // length as LE u16
            vec![SectorEncoding::Repeated as u8],
            u16::to_le_bytes(byte_count as u16/2).to_vec(),
            vec![0,0]
        ].concat();
        Self {
            header,
            data
        }
    }
    /// Pack data into this sector.
    /// Only a uniform sector will be compressed at this level.
    fn pack(&mut self,dat: &[u8]) -> STDRESULT {
        trace!("packing sector {}",self.header.id);
        let sector_size: usize = SECTOR_SIZE_BASE << self.header.sector_shift;
        if dat.len() != sector_size {
            return Err(Box::new(super::Error::SectorAccess));
        }
        self.data = Vec::new();
        if self.header.flags & NO_DATA_MASK > 0 {
            warn!("changing no-data flags in sector {} and writing data",self.header.id);
            self.header.flags &= NO_DATA_MASK ^ u8::MAX;
        }
        if is_slice_uniform(dat) {
            self.data.append(&mut u16::to_le_bytes(5).to_vec());
            self.data.push(SectorEncoding::Repeated as u8);
            self.data.append(&mut u16::to_le_bytes(sector_size as u16/2).to_vec());
            self.data.push(dat[0]);
            self.data.push(dat[0]);
        } else {
            self.data.append(&mut u16::to_le_bytes(sector_size as u16 + 1).to_vec());
            self.data.push(SectorEncoding::Raw as u8);
            self.data.append(&mut dat.to_vec());
        }
        Ok(())
    }
    /// Unpack sector data as raw bytes.
    fn unpack(&self) -> Result<Vec<u8>,DYNERR> {
        trace!("unpacking sector {}",self.header.id);
        let mut ans = Vec::new();
        let loc = "sector ".to_string() + &u8::to_string(&self.header.id);
        let mut ptr: usize = 0;
        let sector_size: usize = SECTOR_SIZE_BASE << self.header.sector_shift;
        if self.header.flags & NO_DATA_MASK > 0 {
            debug!("cyl {} sec {} has no data",self.header.cylinder,self.header.id);
            return Err(Box::new(super::Error::SectorAccess))
        }
        let end = verified_get_slice!(self.data,ptr,2,&loc).to_vec();
        let expected_end = u16::from_le_bytes([end[0],end[1]]) as usize + 2;
        let encoding_code = verified_get_byte!(self.data,ptr,&loc);
        if let Some(encoding) = SectorEncoding::from_u8(encoding_code)
        {
            // TODO: not entirely clear how the repetitions are supposed to work.
            // Do we have [(encoding,entry),(encoding,entry)...] or [encoding,(entry,entry,...)]
            match encoding {
                SectorEncoding::Raw => {
                    trace!("found raw chunk");
                    ans.append(&mut verified_get_slice!(self.data,ptr,sector_size,&loc).to_vec());
                },
                SectorEncoding::Repeated => {
                    trace!("found repeating pattern chunk");
                    while ans.len() < sector_size {
                        let b = verified_get_slice!(self.data,ptr,4,&loc).to_vec();
                        let count = u16::from_le_bytes([b[0],b[1]]) as usize;
                        for _i in 0..count {
                            ans.push(b[2]);
                            ans.push(b[3]);
                        }
                    }
                },
                SectorEncoding::RunLength => {
                    trace!("found run length encoded chunk");
                    while ans.len() < sector_size {
                        let read_count = 2*(verified_get_byte!(self.data,ptr,&loc) as usize);
                        if read_count==0 {
                            let rw_count = verified_get_byte!(self.data,ptr,&loc) as usize;
                            ans.append(&mut verified_get_slice!(self.data,ptr,rw_count,&loc).to_vec());
                        } else {
                            let repeat = verified_get_byte!(self.data,ptr,&loc) as usize;
                            let buf = verified_get_slice!(self.data,ptr,read_count,&loc).to_vec();
                            for _i in 0..repeat {
                                ans.append(&mut buf.clone());
                            }
                        }
                    }
                }
            }
            if ans.len()==sector_size {
                if expected_end != ptr {
                    warn!("length in data header did not match result");
                }
                return Ok(ans);
            } else {
                debug!("sector decoded as wrong size {}",ans.len());
                return Err(Box::new(super::Error::SectorAccess));
            }
        }
        debug!("unknown encoding {} in cyl {} sec {}",encoding_code,self.header.cylinder,self.header.id);
        Err(Box::new(super::Error::SectorAccess))
    }
}

impl Track {
    fn create(track_num: usize, layout: &super::TrackLayout) -> Self {
        let zone = layout.zone(track_num);
        let head = (track_num % layout.sides[zone]) as u8;
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
        let head_map: Vec<u8> = match *layout {
            super::names::KAYPRO4 => match track_num%2 {
                0 => vec![head;10],
                _ => vec![0;10]
            },
            _ => vec![head;layout.sectors[zone]]
        };
        let head_ex = match layout.flux_code[zone] {
           super::FluxCode::FM => head | 0x80,
            _ => head
        };
        let header = TrackHeader {
            sectors: layout.sectors[zone] as u8,
            cylinder: (track_num / layout.sides[zone]) as u8,
            head: head_ex,
            crc: 0
        };
        let mut sectors: Vec<Sector> = Vec::new();
        for i in 0..header.sectors as usize {
            sectors.push(Sector::create(header.cylinder,head_map[i],sector_map[i],layout.sector_size[zone]));
        }
        Self {
            header,
            sectors,
            head_pos: 0
        }
    }
    fn adv_sector(&mut self) -> usize {
        self.head_pos += 1;
        if self.head_pos >= self.sectors.len() {
            self.head_pos = 0;
        }
        self.head_pos
    }
}

impl DiskStruct for Sector {
    fn new() -> Self where Self: Sized {
        Self {
            header: SectorHeader::new(),
            data: Vec::new()
        }
    }
    fn len(&self) -> usize {
        self.header.len() + self.data.len()
    }
    fn to_bytes(&self) -> Vec<u8> {
        let header = match self.unpack() {
            Ok(unpacked) => {
                let mut header = SectorHeader::from_bytes(&self.header.to_bytes());
                header.crc = (crc16(0,&unpacked) & 0xff) as u8;
                header
            },
            _ => {
                SectorHeader::from_bytes(&self.header.to_bytes())
            }
        };
        [
            header.to_bytes(),
            self.data.clone()
        ].concat()
    }
    fn update_from_bytes(&mut self,_bytes: &Vec<u8>) {
        panic!("unreachable was reached"); // do not use
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Sector::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
}

impl DiskStruct for Track {
    fn new() -> Self where Self: Sized {
        Self {
            header: TrackHeader::new(),
            sectors: Vec::new(),
            head_pos: 0
        }
    }
    fn len(&self) -> usize {
        let mut ans = self.header.len();
        for sec in &self.sectors {
            ans += sec.len();
        }
        ans + 1
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        let mut header_bytes = self.header.to_bytes();
        header_bytes[3] = (crc16(0,&header_bytes[0..3]) & 0xff) as u8;
        ans.append(&mut header_bytes);
        for sec in &self.sectors {
            ans.append(&mut sec.to_bytes());
        }
        ans
    }
    fn update_from_bytes(&mut self,_bytes: &Vec<u8>) {
        panic!("unreachable was reached"); // do not use
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Track::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
}

impl Td0 {
    /// Creates a "normal" compression TD0.
    /// If we want advanced compression we can transform the flattened image
    /// with retrocompressor::td0::compress at some later point.
    pub fn create(kind: img::DiskKind) -> Self {
        let comment_string = "created by a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        let layout = match kind {
            img::DiskKind::D3(layout) => layout,
            img::DiskKind::D35(layout) => layout,
            img::DiskKind::D525(layout) => layout,
            img::DiskKind::D8(layout) => layout,
            _ => panic!("cannot create this kind of disk in TD0 format")
        };
        let heads = layout.sides();
        // The following applies to the whole disk, yet the flux code is also packed in
        // a high bit in each track header - how to resolve if there is a conflict
        // is not strictly known.
        let encoded_rate = match (layout.data_rate[0],layout.flux_code[0]) {
            (super::DataRate::R250Kbps,super::FluxCode::FM) => 0x80,
            (super::DataRate::R300Kbps,super::FluxCode::FM) => 0x81,
            (super::DataRate::R500Kbps,super::FluxCode::FM) => 0x82,
            (super::DataRate::R250Kbps,super::FluxCode::MFM) => 0x00,
            (super::DataRate::R300Kbps,super::FluxCode::MFM) => 0x01,
            (super::DataRate::R500Kbps,super::FluxCode::MFM) => 0x02,
            _ => {
                panic!("unsupported data rate and flux encoding");
            }
        };
        let drive_type = match kind {
            img::DiskKind::D3(_) => 3,
            img::DiskKind::D35(_) => 4,
            img::DiskKind::D525(_) => 1,
            img::DiskKind::D8(_) => 5,
            _ => panic!("cannot create this kind of disk in TD0 format")
        };
        let mut tracks: Vec<Track> = Vec::new();
        for track in 0..layout.track_count() {
            tracks.push(Track::create(track,&layout));
        }
        Self {
            kind,
            heads,
            header: ImageHeader {
                signature: [b'T',b'D'],
                sequence: 0,
                check_sequence: 0,
                version: 1*16+5,
                data_rate: encoded_rate,
                drive_type,
                stepping: 0x80,
                dos_alloc_flag: 0,
                sides: heads as u8,
                crc: [0,0]
            },
            comment_header: Some(CommentHeader {
                crc: [0,0],
                data_length: u16::to_le_bytes(comment_string.len() as u16),
                timestamp: CommentHeader::pack_timestamp(None)
            }),
            comment_data: Some(comment_string),
            tracks,
            end: 0xff
        }
    }
    pub fn num_heads(&self) -> usize {
        self.heads
    }
    fn get_track_mut(&mut self,cyl: usize,head: usize) -> Result<&mut Track,img::Error> {
        for trk in &mut self.tracks {
            if trk.header.cylinder as usize==cyl && (trk.header.head & HEAD_MASK) as usize==head {
                return Ok(trk);
            }
        }
        debug!("cannot find cyl {} head {}",cyl,head);
        Err(img::Error::SectorAccess)
    }
    /// This function is used if a CP/M block is requested.
    /// We can only comply if the user tracks are laid out homogeneously.
    fn check_user_area_up_to_cyl(&self,cyl: usize,off: u16) -> STDRESULT {
        let sector_count = self.tracks[off as usize].sectors.len();
        let mut sector_shift: Option<u8> = None;
        for i in off as usize..cyl*self.heads+1 {
            let trk = &self.tracks[i];
            if trk.sectors.len()!=sector_count {
                warn!("heterogeneous layout in user tracks");
                return Err(Box::new(super::Error::ImageTypeMismatch));
            }
            for sec in &trk.sectors {
                match sector_shift {
                    Some(ssh) => {
                        if ssh != sec.header.sector_shift {
                            warn!("heterogeneous sectors on track {}",i);
                            return Err(Box::new(super::Error::ImageTypeMismatch));
                        }
                    },
                    None => {
                        sector_shift = Some(sec.header.sector_shift)
                    }
                }
            }
        }
        Ok(())
    }
    fn get_skew(&self,head: usize) -> Result<Vec<u8>,DYNERR> {
        match (self.kind,head) {
            (super::names::IBM_CPM1_KIND,_) => Ok(skew::CPM_1_LSEC_TO_PSEC.to_vec()),
            (super::names::AMSTRAD_SS_KIND,_) => Ok((1..10).collect()),
            (super::DiskKind::D525(IBM_SSDD_9),_) => Ok((1..10).collect()),
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

impl img::DiskImage for Td0 {
    fn track_count(&self) -> usize {
        self.tracks.len()
    }
    fn byte_capacity(&self) -> usize {
        let mut ans = 0;
        for trk in &self.tracks {
            for sec in &trk.sectors {
                if sec.header.flags & NO_DATA_MASK > 0 {
                    debug!("cyl {} head {} sector {} is marked unreadable, not counted",trk.header.cylinder,trk.header.head & HEAD_MASK,sec.header.id);
                } else {
                    ans += SECTOR_SIZE_BASE << sec.header.sector_shift;
                }
            }
        }
        ans
    }
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR> {
        trace!("reading {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let secs_per_track = self.tracks[off as usize].sectors.len();
                let sector_shift = self.tracks[off as usize].sectors[0].header.sector_shift;
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs((secs_per_track << sector_shift) as usize);
                let chs_list = skew::cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
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
            },
            Block::FAT((_sec1,_secs)) => {
                let secs_per_track = self.tracks[0].sectors.len();
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs(secs_per_track);
                let chs_list = skew::fat_blocking(deblocked_ts_list,self.heads)?;
                for [cyl,head,lsec] in chs_list {
                    self.check_user_area_up_to_cyl(cyl, 0)?;
                    match self.read_sector(cyl,head,lsec) {
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
        trace!("writing {}",addr);
        match addr {
            Block::CPM((_block,_bsh,off)) => {
                let secs_per_track = self.tracks[off as usize].sectors.len();
                let sector_shift = self.tracks[off as usize].sectors[0].header.sector_shift;
                let deblocked_ts_list = addr.get_lsecs((secs_per_track << sector_shift) as usize);
                let chs_list = skew::cpm_blocking(deblocked_ts_list, sector_shift,self.heads)?;
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
            },
            Block::FAT((_sec1,_secs)) => {
                // TODO: do we need to handle variable sectors per track
                let secs_per_track = self.tracks[0].sectors.len();
                let sector_shift = self.tracks[0].sectors[0].header.sector_shift;
                let sec_size = 128 << sector_shift;
                let deblocked_ts_list = addr.get_lsecs(secs_per_track);
                let chs_list = skew::fat_blocking(deblocked_ts_list,self.heads)?;
                let mut src_offset = 0;
                let padded = super::quantize_block(dat, chs_list.len()*sec_size);
                for [cyl,head,lsec] in chs_list {
                    self.check_user_area_up_to_cyl(cyl, 0)?;
                    match self.write_sector(cyl,head,lsec,&padded[src_offset..src_offset+sec_size].to_vec()) {
                        Ok(_) => src_offset += sec_size,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        trace!("seeking sector {} (R)",sec);
        let trk = self.get_track_mut(cyl,head)?;
        // advance to the requested sector
        for _i in 0..trk.sectors.len() {
            let sec_idx = trk.adv_sector();
            let curr = &trk.sectors[sec_idx];
            if sec==curr.header.id as usize {
                trace!("reading sector {}",sec);
                return match curr.header.flags & NO_DATA_MASK {
                    0 => Ok(curr.unpack()?),
                    _ => {
                        debug!("cyl {} head {} sector {}: no data available",cyl,head,sec);
                        Err(Box::new(img::Error::SectorAccess))
                    }
                };
            }
            trace!("skip sector {}",curr.header.id);
        }
        error!("sector {} not found",sec);
        Err(Box::new(img::Error::SectorAccess))
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        trace!("seeking sector {} (W)",sec);
        let trk = self.get_track_mut(cyl,head)?;
        // advance to the requested sector
        for _i in 0..trk.sectors.len() {
            let sec_idx = trk.adv_sector();
            let curr = &mut trk.sectors[sec_idx];
            if sec==curr.header.id as usize {
                trace!("writing sector {}",sec);
                let quantum = SECTOR_SIZE_BASE << curr.header.sector_shift;
                return curr.pack(&super::quantize_block(dat, quantum));
            }
            trace!("skip sector {}",curr.header.id);
        }
        error!("sector {} not found",sec);
        Err(Box::new(img::Error::SectorAccess))
    }
    fn from_bytes(compressed: &Vec<u8>) -> Option<Self> {
        let mut ptr: usize = 0;
        let mut header_slice = optional_get_slice!(compressed,ptr,12,"image header").to_vec();
        let test_header = ImageHeader::from_bytes(&header_slice);
        if &test_header.signature==b"td" {
            info!("TD0 signature found (advanced compression)");
        } else if &test_header.signature==b"TD" {
            info!("TD0 signature found (no advanced compression)");
        } else {
            return None;
        }
        // CRC of image header
        if u16::from_le_bytes(test_header.crc)!=crc16(0,&compressed[0..10]) {
            warn!("image header CRC mismatch");
            return None;
        }
        let expanded = match &test_header.signature {
            b"td" => {
                match retrocompressor::td0::expand_slice(&compressed) {
                    Ok(x) => x,
                    Err(_) => return None
                }
            },
            b"TD" => {
                compressed.clone()
            },
            _ => panic!("unreachable was reached")
        };
        let has_comment = test_header.stepping & COMMENT_MASK > 0;
        ptr = 0;
        header_slice = optional_get_slice!(expanded,ptr,12,"image header").to_vec();
        let header = ImageHeader::from_bytes(&header_slice);
        let mut ans = Self {
            kind: img::DiskKind::Unknown,
            heads: match header.sides { 1 => 1, _ => 2 },
            header,
            comment_header: None,
            comment_data: None,
            tracks: Vec::new(),
            end: 0xff
        };
        if has_comment {
            ans.comment_header = Some(CommentHeader::from_bytes(&optional_get_slice!(expanded,ptr,10,"comment header").to_vec()));
            let comment_len = u16::from_le_bytes(ans.comment_header.as_ref().unwrap().data_length) as usize;
            ans.comment_data = Some(String::from_utf8_lossy(&optional_get_slice!(expanded,ptr,comment_len,"comment data").to_vec()).to_string());
            debug!("comment data `{}`",ans.comment_data.as_ref().unwrap());
            // CRC of comment
            if u16::from_le_bytes(ans.comment_header.as_ref().unwrap().crc)!=crc16(0,&expanded[14..22+comment_len]) {
                warn!("comment area CRC mismatch");
                return None;
            }
        }
        // don't use Track::from_bytes because it may panic
        while expanded[ptr]!=0xff {
            let header = TrackHeader::from_bytes(&optional_get_slice!(expanded,ptr,4,"track header").to_vec());
            // CRC of track header
            // We will not stop for bad track CRC, but do warn
            let expected_track_crc = crc16(0,&header.to_bytes()[0..3]);
            if header.crc != (expected_track_crc & 0xff) as u8{
                warn!("track header CRC mismatch at cyl {} head {}",header.cylinder,header.head);
            }
            let mut trk = Track {
                header,
                sectors: Vec::new(),
                head_pos: 0
            };
            trace!("found cyl {} head {} with {} sectors",trk.header.cylinder,trk.header.head,trk.header.sectors);
            for i in 0..trk.header.sectors {
                let mut sec = Sector::new();
                sec.header = SectorHeader::from_bytes(&optional_get_slice!(expanded,ptr,6,"sector header").to_vec());
                trace!("get sector {}, size {}",sec.header.id,128 << sec.header.sector_shift);
                if sec.header.flags & NO_DATA_MASK == 0 {
                    let size_bytes = optional_get_slice!(expanded,ptr,2,"sector data header").to_vec();
                    let data_size = u16::from_le_bytes([size_bytes[0],size_bytes[1]]) as usize;
                    if ptr + data_size <= expanded.len() {
                        ptr -= 2; // keep the length bytes in the structure
                        sec.data.append(&mut expanded[ptr..ptr+2+data_size].to_vec());
                        ptr += 2 + data_size;
                    } else {
                        debug!("end of data in sector record {} with id {}",i,sec.header.id);
                        debug!("sector wants eof {}, actual {} ",ptr+data_size-1,expanded.len());
                        return None;
                    }
                }
                // CRC for sector data
                // We will not stop for bad sector CRC, but do warn
                if let Ok(unpacked_data) = sec.unpack() {
                    let expected_sector_crc = crc16(0,&unpacked_data);
                    if sec.header.crc != (expected_sector_crc & 0xff) as u8 {
                        warn!("sector CRC mismatch in sector record {} with id {}",i,sec.header.id);
                    }
                } else {
                    trace!("no sector data - skip CRC");
                }
                trk.sectors.push(sec);
            }
            ans.tracks.push(trk);
        }
        debug!("disk capacity {}",ans.byte_capacity());
        // TODO: this works for now, but we should have the TD0 object set up a pattern
        // that can be explicitly matched against the disk kind.
        ans.kind = match (ans.byte_capacity(),ans.tracks[0].header.sectors) {
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
        return Some(ans);
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        self.header.crc = u16::to_le_bytes(crc16(0,&self.header.to_bytes()[0..10]));
        ans.append(&mut self.header.to_bytes());
        match (self.comment_header.as_mut(),self.comment_data.as_ref()) {
            (Some(h),Some(d)) => {
                let encoded_string = d.replace("\r\n","\x00").replace("\n","\x00");
                let encoded_bytes = encoded_string.as_bytes();
                h.data_length = u16::to_le_bytes(encoded_bytes.len() as u16);
                h.crc = u16::to_le_bytes(crc16(0,&[
                    h.to_bytes()[2..].to_vec(),
                    encoded_bytes.to_vec()
                ].concat()));
                ans.append(&mut h.to_bytes());
                ans.append(&mut encoded_bytes.to_vec());
            },
            _ => {}
        }
        for trk in &self.tracks {
            ans.append(&mut trk.to_bytes());
        }
        ans.push(self.end);
        // Real teledisks have several trailing bytes, and some decoders will choke if
        // they are missing (notably MAME).  The value of the bytes is not important,
        // but they do need to be chosen to produce enough bits in the Huffman code so
        // that the decoder will not give up before the end of disk marker.  The following
        // is nothing special, just 7 randomly chosen bytes.
        ans.append(&mut vec![0x27,0x09,0xe1,0xc5,0x89,0x05,0x76]);
        // apply the advanced compression
        retrocompressor::td0::compress_slice(&ans).expect("advanced compression failed")
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::TD0
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
    fn get_track_buf(&mut self,_cyl: usize,_head: usize) -> Result<Vec<u8>,DYNERR> {
        error!("TD0 images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn set_track_buf(&mut self,_cyl: usize,_head: usize,_dat: &[u8]) -> STDRESULT {
        error!("TD0 images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_nibbles(&mut self,_cyl: usize,_head: usize) -> Result<Vec<u8>,DYNERR> {
        error!("TD0 images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &[u8]) -> String {
        String::from("TD0 images have no track bits to display")
    }
    fn get_metadata(&self,indent: u16) -> String {
        let td0 = self.what_am_i().to_string();
        let mut root = json::JsonValue::new_object();
        root[&td0] = json::JsonValue::new_object();
        root[&td0]["header"] = json::JsonValue::new_object();
        getByte!(root,td0,self.header.sequence);
        getByte!(root,td0,self.header.check_sequence);
        getByte!(root,td0,self.header.version);
        getByteEx!(root,td0,self.header.data_rate);
        root[&td0]["header"]["data_rate"]["_pretty"] = json::JsonValue::String(
            match (self.header.data_rate & RATE_MASK,self.header.data_rate & FM_MASK) {
                (0,0) => "MFM 250 kpbs".to_string(),
                (1,0) => "MFM 300 kbps".to_string(),
                (2,0) => "MFM 500 kbps".to_string(),
                (0,128) => "FM 250 kbps".to_string(),
                (1,128) => "FM 300 kbps".to_string(),
                (2,128) => "FM 500 kbps".to_string(),
                _ => "unexpected value".to_string()
            }
        );
        getByteEx!(root,td0,self.header.drive_type);
        root[&td0]["header"]["drive_type"]["_pretty"] = json::JsonValue::String(
            match self.header.drive_type {
                0 => "5.25in".to_string(),
                1 => "5.25in".to_string(),
                2 => "5.25in".to_string(),
                3 => "3.0in".to_string(),
                4 => "3.5in".to_string(),
                5 => "8.0in".to_string(),
                6 => "3.5in".to_string(),
                _ => "unexpected value".to_string()
            }
        );
        getByteEx!(root,td0,self.header.stepping);
        root[&td0]["header"]["stepping"]["_pretty"] = json::JsonValue::String(
            match self.header.stepping & STEPPING_MASK {
                0 => "single step".to_string(),
                1 => "double step".to_string(),
                2 => "even only step (96 tpi disk in 48 tpi drive)".to_string(),
                _ => "unexpected value".to_string()
            }
        );
        getByte!(root,td0,self.header.dos_alloc_flag);
        getByte!(root,td0,self.header.sides);
        match (&self.comment_header,&self.comment_data) {
            (Some(h),Some(d)) => {
                root[&td0]["comment"]["timestamp"]["_raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&h.timestamp));
                root[&td0]["comment"]["timestamp"]["_pretty"] = json::JsonValue::String(h.pretty_timestamp());
                root[&td0]["comment"]["notes"] = json::JsonValue::String(d.to_string());
            },
            _ => {}
        }
        if indent==0 {
            json::stringify(root)
        } else {
            json::stringify_pretty(root, indent)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            debug!("put key `{:?}` with val `{}`",key_path,val);
            let td0 = self.what_am_i().to_string();
            meta::test_metadata(key_path, self.what_am_i())?;
            if meta::match_key(key_path,&[&td0,"comment","timestamp"]) {
                warn!("skipping read-only `timestamp`");
                return Ok(());
            }
            putByte!(val,key_path,td0,self.header.sequence);
            putByte!(val,key_path,td0,self.header.check_sequence);
            putByte!(val,key_path,td0,self.header.version);
            putByte!(val,key_path,td0,self.header.data_rate);
            putByte!(val,key_path,td0,self.header.drive_type);
            putByte!(val,key_path,td0,self.header.stepping);
            putByte!(val,key_path,td0,self.header.dos_alloc_flag);
            putByte!(val,key_path,td0,self.header.sides);
            if meta::match_key(key_path, &[&td0,"comment","notes"]) {
                self.comment_data = Some(val.to_string());
                if self.comment_header.is_none() {
                    self.comment_header = Some(CommentHeader {
                        crc: [0,0], // computed in to_bytes
                        data_length: [0,0], // computed in to_bytes
                        timestamp: CommentHeader::pack_timestamp(None)
                    });
                }
                return Ok(());
            }
        }
        error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}