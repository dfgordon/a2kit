//! # Support for WOZ v2 disk images
//! This uses the nibble machinery in module `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.

use log::info;
use std::str::FromStr;
use crate::disk_base;
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::disk525;
use crate::img_woz;
use crate::img_woz::{INFO_ID,TMAP_ID,TRKS_ID,META_ID,WRIT_ID};

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
    fn create(kind: disk_base::DiskKind) -> Self {

        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        let mut creator: [u8;32] = [0x20;32];
        for i in 0..creator_str.len() {
            creator[i] = creator_str.as_bytes()[i];
        }
        Self {
            id: u32::to_le_bytes(INFO_ID),
            size: u32::to_le_bytes(60),
            vers: 2,
            disk_type: match kind { disk_base::DiskKind::A2_35 => 2, _ => 1 },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            disk_sides: 1,
            boot_sector_format: match kind {
                disk_base::DiskKind::A2_35 => 0,
                disk_base::DiskKind::A2_525_13 => 2,
                disk_base::DiskKind::A2_525_16 => 1,
                _ => panic!("WOZ received hard drive")
            },
            optimal_bit_timing: match kind { disk_base::DiskKind::A2_35 => 16, _ => 32 },
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
    fn create(kind: disk_base::DiskKind) -> Self {
        let mut map: [u8;160] = [0xff;160];
        match kind {
            disk_base::DiskKind::A2_35 => {
                panic!("3.5 inch disk not supported");
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
    fn create(kind: disk_base::DiskKind) -> Self {
        let mut ans = Trks::new();
        if kind!=disk_base::DiskKind::A2_525_16 {
            panic!("only 16 sector 5.25 disks allowed");
        }
        ans.id = u32::to_le_bytes(TRKS_ID);

        // WARNING: this offset relies on a specific chunk order : INFO, TMAP, TRKS
        // So long as we are the creator we can make it so.
        let mut block_offset: usize = 3;
        let mut chunk_size: usize = 0;
        for track in 0..35 {
            // prepare the track bits
            let track_obj = disk525::create_std_track(254, track, MAX_TRACK_BLOCKS as usize*512);
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
        for _track in 35..160 {
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
            track_bits_offset: 0,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            trks: Trks::new(),
            meta: None,
            writ: None
        }
    }
    pub fn create(kind: disk_base::DiskKind) -> Self {
        if kind!=disk_base::DiskKind::A2_525_16 {
            panic!("only 16 sector 5.25 disks allowed");
        }
        Self {
            track_bits_offset: 1536,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(kind),
            meta: None,
            writ: None
        }
    }
    fn get_trk_struct(&self,track: u8) -> Trk {
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
        if ptr==0xff {
            panic!("WOZ track not found");
        }
        return self.trks.tracks[ptr as usize];
    }
    fn get_track_obj(&self,track: u8) -> disk525::TrackBits {
        let trk = self.get_trk_struct(track);
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        let buf = self.trks.bits[begin..end].to_vec();
        let bit_count = u32::from_le_bytes(trk.bit_count) as usize;
        return disk525::TrackBits::create(buf,bit_count);
    }
    fn update_track(&mut self,track_obj: &mut disk525::TrackBits,track: u8) {
        let trk = self.get_trk_struct(track);
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        track_obj.reset();
        track_obj.read(&mut self.trks.bits[begin..end],track_obj.bit_count());
    }
}

impl disk_base::DiskImage for Woz2 {
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
            let (next,id,maybe_chunk) = img_woz::get_next_chunk(ptr, buf);
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
            return Some(ans);
        }
        return None;
    }
    fn update_from_do(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        for track in 0..35 {
            let mut track_obj = self.get_track_obj(track);
            track_obj.update_track_with_do(dsk, track as u8);
            self.update_track(&mut track_obj,track);
        }
        return Ok(());
    }
    fn update_from_po(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        return self.update_from_do(&disk525::reorder_po_to_do(dsk, 16));
    }
    fn to_do(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let mut ans: Vec<u8> = [0;512*280].to_vec();
        for track in 0..35 {
            let mut track_obj = self.get_track_obj(track);
            track_obj.update_do_with_track(&mut ans, track as u8);
        }
        return Ok(ans);
    }
    fn to_po(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        match self.to_do() {
            Ok(v) => Ok(disk525::reorder_do_to_po(&v, 16)),
            Err(e) => Err(e)
        }
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
        let crc = u32::to_le_bytes(img_woz::crc32(0, &ans[12..].to_vec()));
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
            _ => Err(Box::new(disk_base::CommandError::OutOfRange))
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
            _ => Err(Box::new(disk_base::CommandError::OutOfRange))
        }
    }
}
