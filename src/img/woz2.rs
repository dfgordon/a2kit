//! # Support for WOZ v2 disk images
//! This uses the nibble machinery in module `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.

use log::info;
use std::str::FromStr;
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::img::disk525;
use crate::img;
use crate::img::woz::{INFO_ID,TMAP_ID,TRKS_ID,META_ID,WRIT_ID};

const MAX_TRACK_BLOCKS: u16 = 13;

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
    disk_sides: u8,
    boot_sector_format: u8,
    optimal_bit_timing: u8,
    compatible_hardware: [u8;2],
    required_ram: [u8;2],
    largest_track: [u8;2],
    flux_block: [u8;2],
    largest_flux_track: [u8;2],
    pad: [u8;10]
}

#[derive(DiskStruct)]
pub struct TMap {
    id: [u8;4],
    size: [u8;4],
    map: [u8;160]
}

#[derive(DiskStruct,Clone,Copy)]
pub struct Trk {
    starting_block: [u8;2],
    block_count: [u8;2],
    bit_count: [u8;4]
}

pub struct Trks {
    id: [u8;4],
    size: [u8;4],
    tracks: Vec<Trk>,
    bits: Vec<u8>
}

pub struct Woz2 {
    kind: img::DiskKind,
    /// Track bit offsets are given with respect to start of file.
    /// After structuring the data this offset will be needed.
    track_bits_offset: usize,
    header: Header,
    info: Info,
    tmap: TMap,
    trks: Trks,
    meta: Option<Vec<u8>>,
    writ: Option<Vec<u8>>
}

impl Header {
    fn create() -> Self {
        Self {
            vers: [0x57,0x4f,0x5a,0x32],
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
            vers: 2,
            disk_type: match kind {
                img::DiskKind::A2_525_13 => 1,
                img::DiskKind::A2_525_16 => 1,
                img::DiskKind::A2_35 => 2,
                _ => panic!("WOZ rejected disk kind")
            },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            disk_sides: 1,
            boot_sector_format: match kind {
                img::DiskKind::A2_35 => 0,
                img::DiskKind::A2_525_16 => 1,
                img::DiskKind::A2_525_13 => 2,
                _ => panic!("WOZ rejected disk kind")
            },
            optimal_bit_timing: match kind { img::DiskKind::A2_35 => 16, _ => 32 },
            compatible_hardware: u16::to_le_bytes(0),
            required_ram: u16::to_le_bytes(0),
            largest_track: u16::to_le_bytes(MAX_TRACK_BLOCKS),
            flux_block: [0,0],
            largest_flux_track: [0,0],
            pad: [0;10]
        }
    }
}

impl TMap {
    fn create(kind: img::DiskKind) -> Self {
        let mut map: [u8;160] = [0xff;160];
        match kind {
            img::DiskKind::A2_35 => {
                panic!("3.5 inch disk not supported");
            },
            img::DiskKind::A2Max => {
                panic!("HD not supported");
            },
            _ => {
                for i in 0 as u8..139 {
                    map[i as usize] = match i {
                        x if x%4==0 => x/4,
                        x if x%4==1 => x/4,
                        x if x%4==2 => 0xff,
                        x => x/4 + 1
                    };
                }
            }
        }
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
}

impl Trks {
    fn create(vol: u8,kind: img::DiskKind) -> Self {
        let mut ans = Trks::new();
        let tracks: usize = match kind {
            img::DiskKind::A2_525_13 => 35,
            img::DiskKind::A2_525_16 => 35,
            img::DiskKind::A2_35 => panic!("3.5 inch disks not allowed"),
            img::DiskKind::A2Max => panic!("HD not allowed"),
            img::DiskKind::CPM1_8_26 => panic!("8 inch disks not allowed"),
            img::DiskKind::Unknown => panic!("Unknown disk kind not allowed")
        };
        ans.id = u32::to_le_bytes(TRKS_ID);

        // WARNING: this offset relies on a specific chunk order : INFO, TMAP, TRKS
        // So long as we are the creator we can make it so.
        let mut block_offset: usize = 3;
        let mut chunk_size: usize = 0;
        for track in 0..tracks as u8 {
            // prepare the track bits
            let track_obj = match kind {
                img::DiskKind::A2_525_13 => disk525::create_std13_track(vol, track, MAX_TRACK_BLOCKS as usize*512),
                img::DiskKind::A2_525_16 => disk525::create_std16_track(vol, track, MAX_TRACK_BLOCKS as usize*512),
                _ => panic!("unreachable")
            };
            let mut bits_in_blocks = track_obj.to_buffer();
            if bits_in_blocks.len()%512>0 {
                panic!("track bits buffer is not an even number of blocks");
            }
            let blocks = bits_in_blocks.len() / 512;
            // write the track metrics
            let mut trk = Trk::new();
            trk.starting_block = u16::to_le_bytes(block_offset as u16);
            trk.block_count = u16::to_le_bytes(blocks as u16);
            trk.bit_count = u32::to_le_bytes(track_obj.bit_count() as u32);
            ans.tracks.push(trk);
            chunk_size += Trk::new().len();
            // write track bits and advance block ptr
            ans.bits.append(&mut bits_in_blocks);
            block_offset += blocks;
            chunk_size += blocks*512;
        }
        // Pad the unused track metrics
        for _track in tracks..160 {
            ans.tracks.push(Trk::new());
            chunk_size += Trk::new().len();
        }
        ans.size = u32::to_le_bytes(chunk_size as u32);
        return ans;
    }
    fn num_tracks(&self) -> usize {
        let mut ans: usize = 0;
        for track in 0..160 {
            if self.tracks[track].bit_count!=[0,0,0,0] {
                ans += 1;
            }
        }
        return ans;
    }
}

impl DiskStruct for Trks {
    fn new() -> Self where Self: Sized {
        Self {
            id: [0,0,0,0],
            size: [0,0,0,0],
            tracks: Vec::new(),
            bits: Vec::new()
        }
    }
    fn len(&self) -> usize {
        8 + u32::from_le_bytes(self.size) as usize
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        self.id = [bytes[0],bytes[1],bytes[2],bytes[3]];
        self.size = [bytes[4],bytes[5],bytes[6],bytes[7]];
        self.tracks = Vec::new();
        self.bits = Vec::new();
        for track in 0..160 {
            let trk = Trk::from_bytes(&bytes[8+track*8..16+track*8].to_vec());
            self.tracks.push(trk);
        }
        let bitstream_bytes = u32::from_le_bytes(self.size) - 1280;
        if bitstream_bytes%512>0 {
            panic!("WOZ bitstream is not an even number of blocks");
        }
        self.bits.append(&mut bytes[1288..].to_vec());
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
        ans.append(&mut self.bits.clone());
        return ans;
    }
}

impl Woz2 {
    fn new() -> Self {
        Self {
            kind: img::DiskKind::Unknown,
            track_bits_offset: 0,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            trks: Trks::new(),
            meta: None,
            writ: None
        }
    }
    pub fn create(vol: u8,kind: img::DiskKind) -> Self {
        if kind!=img::DiskKind::A2_525_16 && kind!=img::DiskKind::A2_525_13 {
            panic!("only 5.25 disks allowed");
        }
        Self {
            kind,
            track_bits_offset: 1536,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(vol,kind),
            meta: None,
            writ: None
        }
    }
    /// Go through drive head positions to find the index to the track
    fn get_trk_idx(&self,track: u8) -> usize {
        let mut unique_count: usize = 0;
        let mut ptr = 0xff;
        // loop through the drive head positions
        for test in self.tmap.map {
            if test!=ptr && test!=0xff {
                ptr = test;
                unique_count += 1;
            }
            if unique_count>track as usize {
                break;
            }
        }
        if ptr==0xff || unique_count <= track as usize {
            panic!("WOZ track not found");
        }
        return ptr as usize;
    }
    /// Go through drive head positions to find track and get a copy
    fn get_trk_struct(&self,track: u8) -> Trk {
        return self.trks.tracks[self.get_trk_idx(track)];
    }
    /// Get a track with the default formatting protocol.
    /// Caller can use `set_format_protocol` on the result to adjust.
    fn get_track_obj(&self,track: u8) -> disk525::TrackBits {
        let trk = self.get_trk_struct(track);
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        let buf = self.trks.bits[begin..end].to_vec();
        let bit_count = u32::from_le_bytes(trk.bit_count) as usize;
        return disk525::TrackBits::create(buf,bit_count);
    }
    fn update_track(&mut self,track_obj: &mut disk525::TrackBits,track: u8) {
        let idx = self.get_trk_idx(track);
        let trk = &mut self.trks.tracks[idx];
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        track_obj.reset();
        track_obj.read(&mut self.trks.bits[begin..end],track_obj.bit_count());
    }
}

impl img::woz::WozConverter for Woz2 {
    fn num_tracks(&self) -> usize {
        return self.trks.num_tracks();
    }
    fn get_track_obj(&self,track: u8) -> disk525::TrackBits {
        return self.get_track_obj(track);
    }
    fn update_track(&mut self,track_obj: &mut disk525::TrackBits,track: u8) {
        self.update_track(track_obj, track);
    }
}

impl img::DiskImage for Woz2 {
    fn track_count(&self) -> usize {
        match self.info.disk_type {
            1 => 35,
            _ => panic!("disk type not supported")
        }
    }
    fn byte_capacity(&self) -> usize {
        match self.info.disk_type {
            1 => self.track_count()*16*256,
            _ => panic!("disk type not supported")
        }
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::WOZ2
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn read_chunk(&self,addr: crate::fs::Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        super::woz::read_chunk(self, addr)
    }
    fn write_chunk(&mut self, addr: crate::fs::Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        super::woz::write_chunk(self, addr, dat)
    }
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized {
        if buf.len()<12 {
            return None;
        }
        let mut ans = Woz2::new();
        ans.header.update_from_bytes(&buf[0..12].to_vec());
        if ans.header.vers!=[0x57,0x4f,0x5a,0x32] {
            return None;
        }
        info!("identified WOZ v2 header");
        let mut ptr: usize= 12;
        while ptr>0 {
            let (next,id,maybe_chunk) = img::woz::get_next_chunk(ptr, buf);
            match (id,maybe_chunk) {
                (INFO_ID,Some(chunk)) => ans.info.update_from_bytes(&chunk),
                (TMAP_ID,Some(chunk)) => ans.tmap.update_from_bytes(&chunk),
                (TRKS_ID,Some(chunk)) => {
                    ans.track_bits_offset = ptr + 1288;
                    ans.trks.update_from_bytes(&chunk)
                },
                (META_ID,Some(chunk)) => ans.meta = Some(chunk),
                (WRIT_ID,Some(chunk)) => ans.writ = Some(chunk),
                _ => info!("unprocessed chunk with id {:08X}",id)
            }
            ptr = next;
        }
        if ans.info.vers>2 {
            eprintln!("cannot process INFO chunk version {}",ans.info.vers);
            return None;
        }
        if u32::from_le_bytes(ans.info.id)>0 && u32::from_le_bytes(ans.tmap.id)>0 && u32::from_le_bytes(ans.trks.id)>0 {
            ans.kind = match (ans.info.disk_type,ans.info.boot_sector_format) {
                (1,0) => img::DiskKind::A2_525_16,
                (1,1) => img::DiskKind::A2_525_16,
                (1,2) => img::DiskKind::A2_525_13,
                (1,3) => img::DiskKind::A2_525_16,
                (2,_) => img::DiskKind::A2_35,
                _ => img::DiskKind::Unknown
            };
            return Some(ans);
        }
        return None;
    }
    fn to_bytes(&self) -> Vec<u8> {
        if self.track_bits_offset!=1536 {
            panic!("track bits at a nonstandard offset");
        }
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_bytes());
        ans.append(&mut self.info.to_bytes());
        ans.append(&mut self.tmap.to_bytes());
        ans.append(&mut self.trks.to_bytes());
        if let Some(mut meta) = self.meta.clone() {
            ans.append(&mut meta);
        }
        if let Some(mut writ) = self.writ.clone() {
            ans.append(&mut writ);
        }
        let crc = u32::to_le_bytes(img::woz::crc32(0, &ans[12..].to_vec()));
        ans[8] = crc[0];
        ans[9] = crc[1];
        ans[10] = crc[2];
        ans[11] = crc[3];
        return ans;
    }
    fn get_track_buf(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(track) {
            Ok(track_num) if track_num<self.trks.num_tracks() => {
                let track_obj = self.get_track_obj(track_num as u8);
                Ok((0,track_obj.to_buffer()))
            },
            Err(e) => Err(Box::new(e)),
            _ => Err(Box::new(crate::commands::CommandError::OutOfRange))
        }
    }
    fn get_track_bytes(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(track) {
            Ok(track_num) if track_num<self.trks.num_tracks() => {
                let mut ans: Vec<u8> = Vec::new();
                let mut byte: [u8;1] = [0;1];
                let mut track_obj = self.get_track_obj(track_num as u8);
                track_obj.reset();
                for _i in 0..track_obj.len() {
                    track_obj.read_latch(&mut byte,1);
                    ans.push(byte[0]);
                    if track_obj.get_bit_ptr()+8 > track_obj.bit_count() {
                        break;
                    }
                }
                Ok((0,ans))
            },
            Err(e) => Err(Box::new(e)),
            _ => Err(Box::new(crate::commands::CommandError::OutOfRange))
        }
    }
}
