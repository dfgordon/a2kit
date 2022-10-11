//! # Support for WOZ disk images
//! This uses the nibble machinery in module `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.

use crate::disk_base;
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::disk525;

const TRACK_BLOCKS: u16 = 13;
const INFO_ID: u32 = 0x4f464e49;
const TMAP_ID: u32 = 0x50414d54;
const TRKS_ID: u32 = 0x534b5254;

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
    largest_flux_track: [u8;2]
}

#[derive(DiskStruct)]
pub struct TMap {
    id: [u8;4],
    size: [u8;4],
    map: [u8;160]
}

#[derive(DiskStruct)]
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

pub struct WozImage {
    header: Header,
    info: Info,
    tmap: TMap,
    trks: Trks
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
    fn create(kind: &disk_base::DiskKind) -> Self {

        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        let mut creator: [u8;32] = [0x20;32];
        for i in 0..creator_str.len() {
            creator[i] = creator_str.as_bytes()[i];
        }
        Self {
            id: u32::to_le_bytes(INFO_ID),
            size: u32::to_le_bytes(60),
            vers: 3,
            disk_type: match kind { disk_base::DiskKind::A2_35 => 2, _ => 1 },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            disk_sides: 1,
            boot_sector_format: match kind { disk_base::DiskKind::A2_35 => 0, disk_base::DiskKind::A2_525_13 => 2, disk_base::DiskKind::A2_525_16 => 1 },
            optimal_bit_timing: match kind { disk_base::DiskKind::A2_35 => 16, _ => 32 },
            compatible_hardware: u16::to_le_bytes(0),
            required_ram: u16::to_le_bytes(0),
            largest_track: u16::to_le_bytes(TRACK_BLOCKS),
            flux_block: u16::to_le_bytes(0),
            largest_flux_track: u16::to_le_bytes(TRACK_BLOCKS)
        }
    }
}

impl TMap {
    fn create(kind: &disk_base::DiskKind) -> Self {
        let mut map: [u8;160] = [0xff;160];
        match kind {
            disk_base::DiskKind::A2_35 => {
                panic!("3.5 inch disk not supported");
            },
            _ => {
                for i in 0 as u8..140 {
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
    fn create(kind: &disk_base::DiskKind) -> Self {
        let mut ans = Trks::new();
        if *kind!=disk_base::DiskKind::A2_525_16 {
            panic!("only 16 sector 5.25 disks allowed");
        }
        // Write the track metrics
        for track in 0..35 {
            let mut trk = Trk::new();
            trk.starting_block = u16::to_le_bytes(13*track);
            trk.block_count = u16::to_le_bytes(13);
            trk.bit_count = u32::to_le_bytes(13*512*8);
            ans.tracks.push(trk);
        }
        // Pad the unused track metrics
        for _track in 35..160 {
            ans.tracks.push(Trk::new());
        }
        // Write the track bitstreams
        let adr_fmt = disk525::SectorAddressFormat::create_std();
        let dat_fmt = disk525::SectorDataFormat::create_std();
        let special = disk525::NibbleSpecial::None;
        for track in 0..35 {
            disk525::create_track(254, track, &adr_fmt, &dat_fmt, &special);
        }
        return ans;
    }
}

impl DiskStruct for Trks {
    fn new() -> Self where Self: Sized {
        Self {
            id: u32::to_le_bytes(TRKS_ID),
            size: u32::to_le_bytes(1280 + TRACK_BLOCKS as u32*512*35),
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

impl WozImage {
    fn create(kind: &disk_base::DiskKind) -> Self {
        if *kind!=disk_base::DiskKind::A2_525_16 {
            panic!("only 16 sector 5.25 disks allowed");
        }
        Self {
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(kind)
        }
    }
}