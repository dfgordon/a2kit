//! ## Support for WOZ v1 disk images
//! 
//! This uses the nibble machinery in module `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.
//! WOZ v1 cannot handle actual 3.5 inch disk tracks.  Therefore the top level `create`
//! function is set to panic if a 3.5 inch disk is requested.  You can use WOZ v2 for
//! 3.5 inch disks.

use log::{debug,info,error};
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::img::disk525;
use crate::img;
use crate::img::woz::{TMAP_ID,TRKS_ID,INFO_ID,META_ID};

const TRACK_BYTE_CAPACITY: usize = 6646;

#[derive(DiskStruct)]
pub struct Header {
    vers: [u8;4],
    high_bits: u8,
    lfcrlf: [u8;3],
    crc32: [u8;4]
}

#[derive(DiskStruct)]
pub struct Info {
    id: [u8;4],
    size: [u8;4],
    vers: u8,
    disk_type: u8,
    write_protected: u8,
    synchronized: u8,
    cleaned: u8,
    creator: [u8;32],
    pad: [u8;23]
}

#[derive(DiskStruct)]
pub struct TMap {
    id: [u8;4],
    size: [u8;4],
    map: [u8;160]
}

#[derive(DiskStruct,Clone,Copy)]
pub struct Trk {
    bits: [u8;TRACK_BYTE_CAPACITY],
    bytes_used: [u8;2],
    bit_count: [u8;2],
    splice_point: [u8;2],
    splice_nib: u8,
    splice_bit_count: u8,
    pad: [u8;2]
}

pub struct Trks {
    id: [u8;4],
    size: [u8;4],
    tracks: Vec<Trk>
}

pub struct Woz1 {
    kind: img::DiskKind,
    header: Header,
    info: Info,
    tmap: TMap,
    trks: Trks,
    meta: Option<Vec<u8>>
}

impl Header {
    fn create() -> Self {
        Self {
            vers: [0x57,0x4f,0x5a,0x31],
            high_bits: 0xff,
            lfcrlf: [0x0a,0x0d,0x0a],
            crc32: [0,0,0,0]
        }
    }
}

impl Info {
    fn create(kind: img::DiskKind) -> Self {

        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        let mut creator: [u8;32] = [0x20;32];
        for i in 0..creator_str.len() {
            creator[i] = creator_str.as_bytes()[i];
        }
        Self {
            id: u32::to_le_bytes(INFO_ID),
            size: u32::to_le_bytes(60),
            vers: 1,
            disk_type: match kind {
                img::names::A2_DOS32_KIND => 1,
                img::names::A2_DOS33_KIND => 1,
                _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
            },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            pad: [0;23]
        }
    }
}

impl TMap {
    fn create(kind: img::DiskKind) -> Self {
        let mut map: [u8;160] = [0xff;160];
        match kind {
            img::names::A2_DOS32_KIND | img::names::A2_DOS33_KIND => {
                for i in 0 as u8..139 {
                    map[i as usize] = match i {
                        x if x%4==0 => x/4,
                        x if x%4==1 => x/4,
                        x if x%4==2 => 0xff,
                        x => x/4 + 1
                    };
                }
            }
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        }
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
}

impl Trk {
    fn create(vol: u8,track: u8,kind: img::DiskKind) -> Self {
        let padding_byte = 0x00;
        let mut bits: [u8;TRACK_BYTE_CAPACITY] = [padding_byte;TRACK_BYTE_CAPACITY];
        let track_obj = match kind {
            img::names::A2_DOS32_KIND => disk525::create_std13_track(vol,track,TRACK_BYTE_CAPACITY),
            img::names::A2_DOS33_KIND => disk525::create_std16_track(vol,track,TRACK_BYTE_CAPACITY),
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        };
        bits.copy_from_slice(&track_obj.to_buf());
        Self {
            bits,
            bytes_used: u16::to_le_bytes(track_obj.len() as u16),
            bit_count: u16::to_le_bytes(track_obj.bit_count() as u16),
            splice_point: u16::to_le_bytes(0xffff),
            splice_nib: 0,
            splice_bit_count: 0,
            pad: [0,0]
        }
    }
}

impl Trks {
    fn create(vol: u8,kind: img::DiskKind) -> Self {
        let tracks: usize = match kind {
            img::names::A2_DOS32_KIND => 35,
            img::names::A2_DOS33_KIND => 35,
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        };
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        ans.size = u32::to_le_bytes(tracks as u32 * Trk::new().len() as u32);
        for track in 0..tracks {
            let trk = Trk::create(vol,track as u8,kind);
            ans.tracks.push(trk);
        }
        return ans;
    }
    fn num_tracks(&self) -> usize {
        // would this list ever be padded?
        return self.tracks.len();
    }
}

impl DiskStruct for Trks {
    fn new() -> Self where Self: Sized {
        Self {
            id: [0,0,0,0],
            size: [0,0,0,0],
            tracks: Vec::new()
        }
    }
    fn len(&self) -> usize {
        8 + u32::from_le_bytes(self.size) as usize
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        let sz = Trk::new().len();
        self.id = [bytes[0],bytes[1],bytes[2],bytes[3]];
        self.size = [bytes[4],bytes[5],bytes[6],bytes[7]];
        let num_tracks = u32::from_le_bytes(self.size) as usize / sz;
        let mut off = 8;
        self.tracks = Vec::new();
        for _track in 0..num_tracks {
            let trk = Trk::from_bytes(&bytes[off..off+sz].to_vec());
            self.tracks.push(trk);
            off += sz;
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Trks::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.id.to_vec());
        ans.append(&mut self.size.to_vec());
        for trk in &self.tracks {
            ans.append(&mut trk.to_bytes());
        }
        return ans;
    }
}

impl Woz1 {
    fn new() -> Self {
        Self {
            kind: img::DiskKind::Unknown,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            trks: Trks::new(),
            meta: None
        }
    }
    /// Create the image of a specific kind of disk (panics if unsupported disk kind).
    /// The volume is used to format the address fields on the tracks.
    pub fn create(vol: u8,kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_DOS33_KIND {
            panic!("WOZ v1 can only accept 5.25 inch Apple formats")
        }
        Self {
            kind,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(vol,kind),
            meta: None
        }
    }
    /// Get index to the `Trk` structure, searching main track and nearby quarter-tracks.
    /// If no data this will panic.
    fn get_trk_idx(&self,track: u8) -> usize {
        let key_idx = track as usize*4;
        if self.tmap.map[key_idx]!=0xff {
            return self.tmap.map[key_idx] as usize;
        }
        if key_idx!=0 {
            if self.tmap.map[key_idx-1]!=0xff {
                return self.tmap.map[key_idx-1] as usize;
            }
        }
        if key_idx!=self.tmap.map.len() {
            if self.tmap.map[key_idx+1]!=0xff {
                return self.tmap.map[key_idx+1] as usize;
            }
        }
        error!("This image has a missing track; cannot be handled in general");
        panic!("WOZ track not found");
    }
    /// Find track and get a copy
    fn get_trk_struct(&self,track: u8) -> Trk {
        return self.trks.tracks[self.get_trk_idx(track)];
    }
    /// Get the trait object for the track bits.  The nibble format will be
    /// determined by the image's underlying `DiskKind`.
    fn get_track_obj(&self,track: u8) -> Box<dyn super::TrackBits> {
        let trk = self.get_trk_struct(track);
        let buf = trk.bits.to_vec();
        let bit_count = u16::from_le_bytes(trk.bit_count) as usize;
        let ans: Box<dyn super::TrackBits> = match self.kind {
            super::names::A2_DOS32_KIND => Box::new(disk525::TrackBits::create(
                buf,bit_count,
                disk525::SectorAddressFormat::create_std13(),
                disk525::SectorDataFormat::create_std13())),
            super::names::A2_DOS33_KIND => Box::new(disk525::TrackBits::create(
                buf,bit_count,
                disk525::SectorAddressFormat::create_std16(),
                disk525::SectorDataFormat::create_std16())),
            _ => panic!("incompatible disk")
        };
        return ans;
    }
}

impl img::woz::WozConverter for Woz1 {
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn num_tracks(&self) -> usize {
        self.trks.num_tracks()
    }
    fn get_track_obj(&self,track: u8) -> Box<dyn super::TrackBits> {
        self.get_track_obj(track)
    }
    fn update_track(&mut self,track_obj: &mut Box<dyn super::TrackBits>,track: u8) {
        let idx = self.get_trk_idx(track);
        self.trks.tracks[idx].bits.copy_from_slice(&track_obj.to_buf());
    }
}

impl img::DiskImage for Woz1 {
    fn track_count(&self) -> usize {
        match self.info.disk_type {
            1 => 35,
            2 => 160,
            _ => panic!("disk type not supported")
        }
    }
    fn byte_capacity(&self) -> usize {
        match self.info.disk_type {
            1 => self.track_count()*16*256,
            2 => 1600*512,
            _ => panic!("disk type not supported")
        }
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::WOZ1
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn read_chunk(&self,addr: crate::fs::Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        super::woz::read_chunk(self, addr)
    }
    fn write_chunk(&mut self, addr: crate::fs::Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        super::woz::write_chunk(self, addr, dat)
    }
    fn read_sector(&self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        super::woz::read_sector(self,cyl,head,sec)
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        super::woz::write_sector(self, cyl, head, sec, dat)
    }
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized {
        if buf.len()<12 {
            return None;
        }
        let mut ans = Woz1::new();
        ans.header.update_from_bytes(&buf[0..12].to_vec());
        if ans.header.vers!=[0x57,0x4f,0x5a,0x31] {
            return None;
        }
        info!("identified WOZ v1 header");
        let mut ptr: usize= 12;
        while ptr>0 {
            let (next,id,maybe_chunk) = img::woz::get_next_chunk(ptr, buf);
            match (id,maybe_chunk) {
                (INFO_ID,Some(chunk)) => ans.info.update_from_bytes(&chunk),
                (TMAP_ID,Some(chunk)) => ans.tmap.update_from_bytes(&chunk),
                (TRKS_ID,Some(chunk)) => ans.trks.update_from_bytes(&chunk),
                (META_ID,Some(chunk)) => ans.meta = Some(chunk),
                _ => info!("unprocessed chunk with id {:08X}",id)
            }
            ptr = next;
        }
        if u32::from_le_bytes(ans.info.id)>0 && u32::from_le_bytes(ans.tmap.id)>0 && u32::from_le_bytes(ans.trks.id)>0 {
            // TODO: can we figure if this is a 13 sector disk at this point?
            ans.kind = match ans.info.disk_type {
                1 => img::names::A2_DOS33_KIND,
                _ => panic!("WOZ v1 encountered unexpected disk type in INFO chunk")
            };
            debug!("setting disk kind to {}",ans.kind);
            return Some(ans);
        }
        return None;
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_bytes());
        ans.append(&mut self.info.to_bytes());
        ans.append(&mut self.tmap.to_bytes());
        ans.append(&mut self.trks.to_bytes());
        if let Some(mut meta) = self.meta.clone() {
            ans.append(&mut meta);
        }
        let crc = u32::to_le_bytes(img::woz::crc32(0, &ans[12..].to_vec()));
        ans[8] = crc[0];
        ans[9] = crc[1];
        ans[10] = crc[2];
        ans[11] = crc[3];
        return ans;
    }
    fn get_track_buf(&self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let track_obj = self.get_track_obj(track_num as u8);
        Ok(track_obj.to_buf())
    }
    fn get_track_nibbles(&self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let mut track_obj = self.get_track_obj(track_num as u8);
        Ok(track_obj.to_nibbles())
    }
    fn display_track(&self,bytes: &Vec<u8>) -> String {
        super::woz::display_track(self, 0, &bytes)
    }
}
