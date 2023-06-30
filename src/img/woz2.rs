//! ## Support for WOZ v2 disk images
//! 
//! This uses the nibble machinery in `disk35` and `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.

use log::{debug,info,warn,error};
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::img::{disk35,disk525};
use crate::img;
use crate::img::woz::{INFO_ID,TMAP_ID,TRKS_ID,META_ID,WRIT_ID,HeadCoords};
use crate::{STDRESULT,DYNERR};

const MAX_TRACK_BLOCKS_525: u16 = 13;
const MAX_TRACK_BLOCKS_35: u16 = 19;

pub fn file_extensions() -> Vec<String> {
    vec!["woz".to_string()]
}

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
    writ: Option<Vec<u8>>,
    head_coords: HeadCoords
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
                img::names::A2_DOS32_KIND => 1,
                img::names::A2_DOS33_KIND => 1,
                img::names::A2_400_KIND => 2,
                img::names::A2_800_KIND => 2,
                _ => panic!("WOZ rejected disk kind")
            },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            disk_sides: match kind {
                img::names::A2_400_KIND => 1,
                img::names::A2_800_KIND => 2,
                img::names::A2_DOS33_KIND => 1,
                img::names::A2_DOS32_KIND => 1,
                _ => panic!("WOZ rejected disk kind")
            },
            boot_sector_format: match kind {
                img::names::A2_400_KIND => 0,
                img::names::A2_800_KIND => 0,
                img::names::A2_DOS33_KIND => 1,
                img::names::A2_DOS32_KIND => 2,
                _ => panic!("WOZ rejected disk kind")
            },
            optimal_bit_timing: match kind {
                img::names::A2_400_KIND => 16,
                img::names::A2_800_KIND => 16,
                img::names::A2_DOS33_KIND => 32,
                img::names::A2_DOS32_KIND => 32,
                _ => panic!("WOZ rejected disk kind")
            },
            compatible_hardware: u16::to_le_bytes(0),
            required_ram: u16::to_le_bytes(0),
            largest_track: match kind {
                img::names::A2_DOS32_KIND => u16::to_le_bytes(MAX_TRACK_BLOCKS_525),
                img::names::A2_DOS33_KIND => u16::to_le_bytes(MAX_TRACK_BLOCKS_525),
                img::names::A2_400_KIND => u16::to_le_bytes(MAX_TRACK_BLOCKS_35),
                img::names::A2_800_KIND => u16::to_le_bytes(MAX_TRACK_BLOCKS_35),
                _ => panic!("WOZ rejected disk kind")
            },
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
            img::names::A2_400_KIND => {
                for i in 0 as u8..80 {
                    map[i as usize] = i;
                }
            },
            img::names::A2_800_KIND => {
                // WOZ2 mapping calls for cyl0,side0; cyl0,side1; etc..
                // We number tracks in this same sequence.
                for i in 0 as u8..160 {
                    map[i as usize] = i;
                }
            },
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
            _ => panic!("disk kind not supported")
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
            img::names::A2_DOS32_KIND => 35,
            img::names::A2_DOS33_KIND => 35,
            img::names::A2_400_KIND => 80,
            img::names::A2_800_KIND => 160,
            _ => panic!("WOZ v2 permits only physical Apple 3.5 or 5.25 inch kinds")
        };
        ans.id = u32::to_le_bytes(TRKS_ID);

        // WARNING: this offset relies on a specific chunk order : INFO, TMAP, TRKS
        // So long as we are the creator we can make it so.
        let mut block_offset: usize = 3;
        let mut chunk_size: usize = 0;
        for track in 0..tracks as u8 {
            // prepare the track bits
            let (mut bits_in_blocks,track_obj) = match kind {
                img::names::A2_DOS32_KIND => disk525::create_std13_track(vol, track, MAX_TRACK_BLOCKS_525 as usize*512),
                img::names::A2_DOS33_KIND => disk525::create_std16_track(vol, track, MAX_TRACK_BLOCKS_525 as usize*512),
                img::names::A2_400_KIND => {
                    let bytes = disk35::TRACK_BITS[track as usize/16] / 8;
                    disk35::create_std_track(track, 1, bytes + (512-bytes%512) + 512)
                },
                img::names::A2_800_KIND => {
                    let bytes = disk35::TRACK_BITS[track as usize/32] / 8;
                    disk35::create_std_track(track, 2, bytes + (512-bytes%512) + 512)
                },
                _ => panic!("unreachable")
            };
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
            writ: None,
            head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
        }
    }
    pub fn create(vol: u8,kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS33_KIND && kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_400_KIND && kind!=img::names::A2_800_KIND {
            panic!("WOZ v2 permits only physical Apple 3.5 or 5.25 inch kinds");
        }
        Self {
            kind,
            track_bits_offset: 1536,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(vol,kind),
            meta: None,
            writ: None,
            head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
        }
    }
    /// Get index to the `Trk` structure, searching main track and nearby quarter-tracks.
    /// If no data this will panic.
    fn get_trk_idx(&self,track: u8) -> usize {
        match self.kind {
            img::names::A2_400_KIND => {
                let key_idx = track as usize;
                if self.tmap.map[key_idx]<80 {
                    return self.tmap.map[key_idx] as usize;
                }
            },
            img::names::A2_800_KIND => {
                let key_idx = track as usize;
                if self.tmap.map[key_idx]<160 {
                    return self.tmap.map[key_idx] as usize;
                }
            },
            _ => {
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
            }
        }
        error!("This image has a missing track; cannot be handled in general");
        panic!("WOZ track not found");
    }
    /// Find track and get a reference
    fn get_trk_ref(&self,track: u8) -> &Trk {
        return &self.trks.tracks[self.get_trk_idx(track)];
    }
    /// Get a reference to the track bits
    fn get_trk_bits_ref(&self,track: u8) -> &[u8] {
        let trk = self.get_trk_ref(track);
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        &self.trks.bits[begin..end]
    }
    /// Get a mutable reference to the track bits
    fn get_trk_bits_mut(&mut self,track: u8) -> &mut [u8] {
        let trk = self.get_trk_ref(track);
        let begin = u16::from_le_bytes(trk.starting_block) as usize*512 - self.track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize*512;
        &mut self.trks.bits[begin..end]
    }
    /// Create a lightweight trait object to read/write the bits.  The nibble format will be
    /// determined by the image's underlying `DiskKind`.
    fn new_rw_obj(&mut self,track: u8) -> Box<dyn super::TrackBits> {
        if self.head_coords.track != track as usize {
            debug!("goto track {} of {}",track,self.kind);
            self.head_coords.track = track as usize;
        }
        let bit_count_le = self.get_trk_ref(track).bit_count;
        let bit_count = u32::from_le_bytes(bit_count_le) as usize;
        let mut ans: Box<dyn super::TrackBits> = match self.kind {
            super::names::A2_DOS32_KIND => Box::new(disk525::TrackBits::create(
                track as usize,
                bit_count,
                disk525::SectorAddressFormat::create_std13(),
                disk525::SectorDataFormat::create_std13())),
            super::names::A2_DOS33_KIND => Box::new(disk525::TrackBits::create(
                track as usize,
                bit_count,
                disk525::SectorAddressFormat::create_std16(),
                disk525::SectorDataFormat::create_std16())),
            super::names::A2_400_KIND => Box::new(disk35::TrackBits::create(
                track as usize,
                bit_count,
                1)),
            super::names::A2_800_KIND => Box::new(disk35::TrackBits::create(
                track as usize,
                bit_count,
                2)),
            _ => panic!("incompatible disk")
        };
        if self.head_coords.bit_ptr < bit_count {
            ans.set_bit_ptr(self.head_coords.bit_ptr);
        }
        return ans;
    }
}

impl img::woz::WozUnifier for Woz2 {
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn num_tracks(&self) -> usize {
        self.trks.num_tracks()
    }
    fn read_sector(&mut self,track: u8,sector: u8) -> Result<Vec<u8>,img::NibbleError> {
        let mut reader = self.new_rw_obj(track);
        let ans = reader.read_sector(self.get_trk_bits_ref(track),track,sector)?;
        self.head_coords.bit_ptr = reader.get_bit_ptr();
        Ok(ans)
    }
    fn write_sector(&mut self,dat: &[u8],track: u8,sector: u8) -> Result<(),img::NibbleError> {
        let mut writer = self.new_rw_obj(track);
        writer.write_sector(self.get_trk_bits_mut(track),dat,track,sector)?;
        self.head_coords.bit_ptr = writer.get_bit_ptr();
        Ok(())
    }
}

impl img::DiskImage for Woz2 {
    fn track_count(&self) -> usize {
        match self.info.disk_type {
            1 => 35,
            2 => match self.info.disk_sides {
                1 => 80,
                2 => 160,
                _ => panic!("sides must be 1 or 2")
            },
            _ => panic!("disk type not supported")
        }
    }
    fn byte_capacity(&self) -> usize {
        match self.info.disk_type {
            1 => self.track_count()*16*256,
            2 => match self.info.disk_sides {
                1 => 800*512,
                2 => 1600*512,
                _ => panic!("sides must be 1 or 2")
            },
            _ => panic!("disk type not supported")
        }
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::WOZ2
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
    fn read_block(&mut self,addr: crate::fs::Block) -> Result<Vec<u8>,DYNERR> {
        super::woz::read_block(self, addr)
    }
    fn write_block(&mut self, addr: crate::fs::Block, dat: &[u8]) -> STDRESULT {
        super::woz::write_block(self, addr, dat)
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        super::woz::read_sector(self,cyl,head,sec)
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        super::woz::write_sector(self, cyl, head, sec, dat)
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
                _ => if id!=0 {
                    info!("unprocessed chunk with id {:08X}/{}",id,String::from_utf8_lossy(&u32::to_le_bytes(id)))
                }
            }
            ptr = next;
        }
        if ans.info.vers>3 {
            error!("cannot process INFO chunk version {}",ans.info.vers);
            return None;
        }
        if ans.info.flux_block!=[0,0] && ans.info.largest_flux_track!=[0,0] {
            error!("WOZ uses flux data (not supported)");
            return None;
        }
        if u32::from_le_bytes(ans.info.id)>0 && u32::from_le_bytes(ans.tmap.id)>0 && u32::from_le_bytes(ans.trks.id)>0 {
            ans.kind = match (ans.info.disk_type,ans.info.boot_sector_format,ans.info.disk_sides) {
                (1,0,1) => img::names::A2_DOS33_KIND,
                (1,1,1) => img::names::A2_DOS33_KIND,
                (1,2,1) => img::names::A2_DOS32_KIND,
                (1,3,1) => img::names::A2_DOS33_KIND,
                (2,_,1) => img::names::A2_400_KIND,
                (2,_,2) => img::names::A2_800_KIND,
                _ => img::DiskKind::Unknown
            };
            debug!("setting disk kind to {}",ans.kind);
            return Some(ans);
        }
        return None;
    }
    fn to_bytes(&mut self) -> Vec<u8> {
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
    fn get_track_buf(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        Ok(self.get_trk_bits_ref(track_num as u8).to_vec())
    }
    fn set_track_buf(&mut self,cyl: usize,head: usize,dat: &[u8]) -> STDRESULT {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let bits = self.get_trk_bits_mut(track_num as u8);
        if bits.len()!=dat.len() {
            error!("source track buffer is {} bytes, destination track buffer is {} bytes",dat.len(),bits.len());
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        bits.copy_from_slice(dat);
        Ok(())
    }
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let mut reader = self.new_rw_obj(track_num as u8);
        Ok(reader.to_nibbles(self.get_trk_bits_ref(track_num as u8)))
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        super::woz::display_track(self, 0, &bytes)
    }
    fn get_metadata(&self,indent: u16) -> String {
        let mut json = json::JsonValue::new_object();
        json["image_type"] = json::JsonValue::String("woz2".to_string());

        json["info"] = json::JsonValue::new_object();
        json["meta"] = json::JsonValue::new_object();
        json["info"]["creator"] = json::JsonValue::String(String::from_utf8_lossy(&self.info.creator).trim_end().to_string());
        json["info"]["disk_type"] = json::JsonValue::new_object();
        json["info"]["disk_type"]["raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.disk_type]));
        json["info"]["disk_type"]["pretty"] = json::JsonValue::String(match self.info.disk_type {
            1 => "Apple 5.25 inch".to_string(),
            2 => "Apple 3.5 inch".to_string(),
            _ => "Unexpected value".to_string()
        });
        json["info"]["write_protected"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.write_protected]));
        json["info"]["cleaned"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.cleaned]));
        json["info"]["synchronized"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.synchronized]));
        json["info"]["sides"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.disk_sides]));
        json["info"]["boot_sector_format"] = json::JsonValue::new_object();
        json["info"]["boot_sector_format"]["raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.boot_sector_format]));
        json["info"]["boot_sector_format"]["pretty"] = json::JsonValue::String(match self.info.boot_sector_format {
            0 => "Unknown".to_string(),
            1 => "Boots 16-sector".to_string(),
            2 => "Boots 13-sector".to_string(),
            3 => "Boots both".to_string(),
            _ => "Unexpected value".to_string()
        });
        json["info"]["optimal_bit_timing"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![self.info.optimal_bit_timing]));
        json["info"]["compatible_hardware"] = json::JsonValue::new_object();
        json["info"]["compatible_hardware"]["raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&self.info.compatible_hardware));
        let mut hardware = String::new();
        if self.info.compatible_hardware[0] & 1 > 0 {
            hardware += "Apple ][, ";
        }
        if self.info.compatible_hardware[0] & 2 > 0  {
            hardware += "Apple ][ Plus, ";
        }
        if self.info.compatible_hardware[0] & 4 > 0  {
            hardware += "Apple //e (unenhanced), ";
        }
        if self.info.compatible_hardware[0] & 8 > 0  {
            hardware += "Apple //c, ";
        }
        if self.info.compatible_hardware[0] & 16 > 0  {
            hardware += "Apple //e Enhanced, ";
        }
        if self.info.compatible_hardware[0] & 32 > 0  {
            hardware += "Apple IIgs, ";
        }
        if self.info.compatible_hardware[0] & 64 > 0  {
            hardware += "Apple //c Plus, ";
        }
        if self.info.compatible_hardware[0] & 128 > 0  {
            hardware += "Apple ///, ";
        }
        if self.info.compatible_hardware[1] & 1 > 0  {
            hardware += "Apple /// Plus, ";
        }
        if hardware.len()==0 {
            json["info"]["compatible_hardware"]["pretty"] = json::JsonValue::String("unknown".to_string());
        } else {
            json["info"]["compatible_hardware"]["pretty"] = json::JsonValue::String(hardware);
        }

        if let Some(meta) = &self.meta {
            let meta_str = String::from_utf8_lossy(&meta[8..]);
            let lines = meta_str.lines();
            for line in lines {
                let cols: Vec<&str> = line.split('\t').collect();
                if cols.len()==2 {
                    json["meta"][cols[0]] = json::JsonValue::String(cols[1].to_string());
                } else {
                    warn!("bad META record in WOZ {}",line);
                }
            }
        }
        if indent==0 {
            json::stringify(json)
        } else {
            json::stringify_pretty(json, indent)
        }
    }
    fn put_metadata(&mut self,key_path: &str,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            match key_path {
                "/image_type" => img::test_metadata(val, self.what_am_i()),
                "/info/creator" => img::set_metadata_hex(val, &mut self.info.creator),
                "/info/disk_type" | "/info/disk_type/raw" => {warn!("request to change disk type ignored"); Ok(()) },
                "/info/write_protected" | "/info/write_protected/raw" => img::set_metadata_byte(val, &mut self.info.write_protected),
                "/info/cleaned" | "/info/cleaned/raw" => img::set_metadata_byte(val, &mut self.info.cleaned),
                "/info/synchronized" | "/info/synchronized/raw" => img::set_metadata_byte(val, &mut self.info.synchronized),
                _ => Err(Box::new(img::Error::MetaDataMismatch))
            }
        } else {
            Err(Box::new(img::Error::MetaDataMismatch))
        }
    }
}
