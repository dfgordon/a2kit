//! ## Support for WOZ v2 disk images
//! 
//! This uses the nibble machinery in `tracks::gcr` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.

use std::collections::HashMap;
use regex;
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;
use crate::img;
use crate::img::{Track,Sector,SectorHood,meta};
use crate::img::tracks::{Method,FluxCells};
use crate::img::tracks::gcr::TrackEngine;
use crate::img::woz::{INFO_ID,TMAP_ID,TRKS_ID,FLUX_ID,META_ID,WRIT_ID};
use crate::bios::blocks::apple;
use crate::{STDRESULT,DYNERR,getByte,getByteEx,getHexEx,putByte,putHex,putStringBuf};

const MAX_TRACK_BLOCKS_525: u16 = 13;
const MAX_TRACK_BLOCKS_35: u16 = 19;

// calculation of 3.5 inch disk sector bits
// const SYNC_TRACK_HEADER_NIBS: usize = 36;
// const SYNC_GAP_NIBS: usize = 6;
// const SYNC_CLOSE_NIBS: usize = 36;
// const DATA_NIBS: usize = 699; // nibbles of data, checksum follows 
// const CHK_NIBS: usize = 4; // how many checksum nibbles after data
// const ADDRESS_FULL_SEGMENT: usize = 3 + 5 + 2; // prolog,cyl,sec,side,format,chk,epilog
// const DATA_FULL_SEGMENT: usize = 3 + 1 + DATA_NIBS + CHK_NIBS + 2; // prolog,sec,data+chk,epilog
// const SECTOR_BITS: usize = ADDRESS_FULL_SEGMENT*8 + SYNC_GAP_NIBS*10 + DATA_FULL_SEGMENT*8 + SYNC_CLOSE_NIBS*10;

/// Form regex to match patterns like `a|c|b` (order deliberately scrambled).
/// Expansion of `metaOptions!("a","b","c")` looks like this: `^(a|b|c)(\|(a|b|c))*$`
macro_rules! metaOptions {
    ($x:literal,$($y:literal),+) => {
        concat!("^(",$x,$("|",$y),+,r")(\|(",$x,$("|",$y),+,"))*$")
    }
}

/// Tuple (key,regex), where regex matches to an allowed pattern.
/// The regex will not forbid redundant repetitions.
/// The regex will not match to an empty string.
/// Do not confuse the `|` appearing in the regex with the one in the metadata value.
const STD_META_OPTIONS: [(&str,&str);5] = [
    (
        "language",
        metaOptions!(
            "English","Spanish","French","German","Chinese","Japanese","Italian","Dutch",
            "Portuguese","Danish","Finnish","Norwegian","Swedish","Russian","Polish","Turkish",
            "Arabic","Thai","Czech","Hungarian","Catalan","Croatian","Greek","Hebrew","Romanian",
            "Slovak","Ukrainian","Indonesian","Malay","Vietnamese","Other"
        )
    ),
    ("requires_ram",r"^(16K|24K|32K|48K|64K|128K|256K|512K|768K|1M|1\.25M|1\.5M\+|Unknown)$"),
    ("requires_rom",r"^(Any|Integer|Applesoft|IIgs ROM0|IIgs ROM0\+1|IIgs ROM1|IIgs ROM1\+3|IIgs ROM3)$"),
    ("requires_machine",metaOptions!("2",r"2\+","2e","2c",r"2e\+","2gs",r"2c\+","3",r"3\+")),
    ("side",r"^Disk [0-9]+, Side [A-B]$")
];

const STD_META_KEYS: [&str;16] = [
    "title","subtitle","publisher","developer","copyright","version","language","requires_ram",
    "requires_rom","requires_machine","apple2_requires","notes","side","side_name","contributor","image_date"
];

/// These are all in the INFO chunk
const RO_META_ITEMS: [&str;5] = [
    "disk_type",
    "disk_sides",
    "largest_track",
    "flux_block",
    "largest_flux_block"
];

const COMPATIBLE_HARDWARE_OPT: [&str;9] = [
    "Apple ][",
    "Apple ][ Plus",
    "Apple //e (unenhanced)",
    "Apple //c",
    "Apple //e Enhanced",
    "Apple IIgs",
    "Apple //c Plus",
    "Apple ///",
    "Apple /// Plus"
];

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

/// TMap struct is used for both TMAP and FLUX chunks
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

pub struct Meta {
    id: [u8;4],
    size: [u8;4],
    recs: Vec<(String,String)>
}

pub struct Woz2 {
    kind: img::DiskKind,
    fmt: Option<img::tracks::DiskFormat>,
    /// Track bit offsets are given with respect to start of file.
    /// After structuring the data this offset will be needed.
    track_bits_offset: usize,
    header: Header,
    info: Info,
    tmap: TMap,
    flux: Option<TMap>,
    trks: Trks,
    meta: Option<Meta>,
    writ: Option<Vec<u8>>,
    /// state: controller
    engine: TrackEngine,
    /// state: current track data and angle
    cells: Option<FluxCells>,
    /// state: current index into the TMAP
    tmap_pos: usize,
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
    fn blank(kind: img::DiskKind) -> Self {
        let mut ans = Self::create(kind);
        ans.boot_sector_format = 0;
        ans
    }
    /// in general this should be followed up with `update` to account for track hints
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
    /// If there has been a change to the tracks, this can be called to update the hints in the INFO chunk
    fn update(&mut self,bmap: &TMap,fmap: &Option<TMap>,trks: &Trks) {
        self.largest_track = u16::to_le_bytes(trks.largest_track_in_blocks(&bmap));
        match &fmap {
            Some(map) => {
                self.flux_block = u16::to_le_bytes((3 + trks.bits.len() / 512) as u16);
                self.largest_flux_track = u16::to_le_bytes(trks.largest_track_in_blocks(map));
                self.vers = 3;
            },
            None => {
                self.flux_block = [0,0];
                self.largest_flux_track = [0,0];
                self.vers = 2;
            }
        }
    }
    fn verify_value(&self,key: &str,hex_str: &str) -> bool {
        match key {
            stringify!(disk_type) => hex_str=="01" || hex_str=="02",
            stringify!(write_protected) => hex_str=="00" || hex_str=="01",
            stringify!(synchronized) => hex_str=="00" || hex_str=="01",
            stringify!(cleaned) => hex_str=="00" || hex_str=="01",
            stringify!(disk_sides) => hex_str=="01" || hex_str=="02",
            stringify!(boot_sector_format) => hex_str=="00" || hex_str=="01" || hex_str=="02" || hex_str=="03",
            stringify!(compatible_hardware) => {
                if hex_str.len()!=4 {
                    return false;
                }
                match hex::decode(hex_str) {
                    Ok(val) => {
                        let val = u16::from_le_bytes([val[0],val[1]]);
                        val<512
                    },
                    Err(_) => false
                }
            },
            _ => true
        }
    }
}

impl TMap {
    fn blank() -> Self {
        let map: [u8;160] = [0xff;160];
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
    /// This will map 3 quarter tracks for each motor position found in the DiskFormat.
    /// It will return a tuple with (Self,Option<Self>), where the option is the FLUX map.
    /// The map value (for either TMap) is the index into the TRKS chunk, and these will be filled in order.
    /// The FLUX map is created if flux.len()>0, regardless of whether there is a match in fmt.
    fn create(kind: &img::DiskKind, fmt: &img::tracks::DiskFormat, flux: Vec<Track>) -> Result<(Self,Option<Self>),DYNERR> {
        let mut bmap: [u8;160] = [0xff;160];
        let mut fmap: [u8;160] = [0xff;160];
        let motor_head = fmt.get_motor_and_head();
        let mut flux_set = std::collections::HashSet::new();
        for trk in flux {
            let [motor,head,_] = img::woz::get_motor_pos(trk, kind)?;
            flux_set.insert([motor,head]);
        }
        let mut slot = 0;
        match *kind {
            img::DiskKind::D525(_) => {
                for (m,h) in motor_head {
                    if h > 0 || m > 159 {
                        log::error!("motor {} head {} is not legal",m,h);
                        return Err(Box::new(img::Error::GeometryMismatch))
                    }
                    let map = if flux_set.contains(&[m,h]) { &mut fmap } else { &mut bmap };
                    if m>0 {
                        map[m-1] = slot;
                    }
                    map[m] = slot;
                    if m<159 {
                        map[m+1] = slot;
                    }
                    slot += 1;
                }
            },
            img::names::A2_400_KIND => {
                for (m,h) in motor_head {
                    if h > 0 || m > 79 {
                        log::error!("motor {} head {} is not legal",m,h);
                        return Err(Box::new(img::Error::GeometryMismatch))
                    }
                    let map = if flux_set.contains(&[m,h]) { &mut fmap } else { &mut bmap };
                    map[m] = slot;
                    slot += 1;
                }
            },
            img::names::A2_800_KIND => {
                for (m,h) in motor_head {
                    if h > 1 || m > 79 {
                        log::error!("motor {} head {} is not legal",m,h);
                        return Err(Box::new(img::Error::GeometryMismatch))
                    }
                    let map = if flux_set.contains(&[m,h]) { &mut fmap } else { &mut bmap };
                    map[m*2+h] = slot;
                    slot += 1;
                }
            },
            _ => return Err(Box::new(img::Error::ImageTypeMismatch))
        };
        if slot > 160 {
            log::warn!("TMAP has unusual slot reference {}",slot);
        }
        let tmap_chunk = Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map: bmap
        };
        let fmap_chunk = match flux_set.len() > 0 {
            true => Some(Self {
                id: u32::to_le_bytes(FLUX_ID),
                size: u32::to_le_bytes(160),
                map: fmap
            }),
            false => None
        };
        Ok((tmap_chunk,fmap_chunk))
    }
}

impl Trks {
    fn blank() -> Self {
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        let mut chunk_size: usize = 0;
        for _track in 0..160 {
            ans.tracks.push(Trk::new());
            chunk_size += Trk::new().len();
        }
        ans.size = u32::to_le_bytes(chunk_size as u32);
        return ans;
    }
    fn create_segment_and_trk(hood: SectorHood,fmt: &img::tracks::ZoneFormat,block_offset: usize,is_flux: bool) -> Result<(Vec<u8>,Trk),DYNERR> {
        fmt.check_flux_code(img::FluxCode::GCR)?;
        let mut engine = TrackEngine::create(Method::Edit, false);
        let mut cells = engine.format_track(hood, 0xffffff, fmt)?;
        if is_flux {
            cells.change_resolution(2);
        }
        let (segment,count) = cells.to_woz_buf(None,0);
        let blocks = segment.len() / 512;
        // write the track metrics
        let mut trk = Trk::new();
        trk.starting_block = u16::to_le_bytes(block_offset as u16);
        trk.block_count = u16::to_le_bytes(blocks as u16);
        trk.bit_count = u32::to_le_bytes(count as u32);
        Ok((segment,trk))
    }
    fn create(vol: u8,kind: &img::DiskKind,fmt: &img::tracks::DiskFormat,tmap: &TMap,flux: &Option<TMap>) -> Result<Self,DYNERR> {
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        // This offset assumes we are creating with chunk order INFO, TMAP, TRKS.
        // The assumption is only used during creation, where we can make it so.
        let mut block_offset: usize = 3;
        let mut chunk_size: usize = 0;
        let maybe_fmap = match flux {
            Some(chunk) => Some(chunk.map.as_slice()),
            None => None
        };
        for slot in 0..160 {
            if let Some((motor,hood,is_flux)) = img::woz::get_trks_slot_id(vol, slot, &tmap.map, maybe_fmap, kind) {
                log::trace!("create track at slot {} with motor {} and key {}",slot,motor,hood);
                let z_fmt = fmt.get_zone_fmt(motor as usize,hood.head() as usize)?;
                let (mut segment,trk) = Self::create_segment_and_trk(hood,z_fmt,block_offset,is_flux)?;
                chunk_size += Trk::new().len();
                chunk_size += segment.len();
                block_offset += segment.len() / 512;
                ans.tracks.push(trk);
                ans.bits.append(&mut segment);
            } else {
                chunk_size += Trk::new().len();
                ans.tracks.push(Trk::new());
            }
        }
        ans.size = u32::to_le_bytes(chunk_size as u32);
        Ok(ans)
    }
    /// Get the slice-range for `trks.bits`, valid for either bits or flux.
    /// The `idx` argument will generally come from `try_motor`.
    fn rng(&self,idx: usize,track_bits_offset: usize) ->[usize;2] {
        let trk = &self.tracks[idx];
        let begin = u16::from_le_bytes(trk.starting_block) as usize * 512 - track_bits_offset;
        let end = begin + u16::from_le_bytes(trk.block_count) as usize * 512;
        [begin,end]
    }
    /// returns the largest track in blocks that is mapped by `tmap_or_flux`
    fn largest_track_in_blocks(&self,tmap_or_flux: &TMap) -> u16 {
        let mut largest = 0;
        for idx in &tmap_or_flux.map {
            if *idx as usize == 0xff {
                continue;
            }
            if *idx as usize >= self.tracks.len() {
                log::warn!("ignoring bad index into TRKS chunk");
                continue;
            }
            let val = u16::from_le_bytes(self.tracks[*idx as usize].block_count);
            largest = largest.max(val);
        }
        largest
    }
    /// Update the track bits/fluxes in segment `idx` using the data in `seg`.
    /// If the segment has grown, update all the `Trk` offsets.
    /// N.b. the INFO chunk has to be updated externally.
    /// May panic if `seg.len() % 512 > 0`.
    fn update_segment(&mut self,idx: usize,seg: &[u8],count: usize,track_bits_offset: usize) -> STDRESULT {
        let [beg,end] = self.rng(idx,track_bits_offset);
        if beg + seg.len() <= end {
            self.bits[beg..beg+seg.len()].copy_from_slice(&seg);
            self.bits[beg+seg.len()..end].fill(0);
            self.tracks[idx].bit_count = u32::to_le_bytes(count as u32);
        } else {
            let additional = seg.len() + beg - end;
            assert!(additional % 512 == 0);
            self.bits[end..].rotate_right(additional);
            self.bits.extend_from_within(end..end+additional);
            self.bits[beg..beg+seg.len()].copy_from_slice(&seg);
            self.tracks[idx].bit_count = u32::to_le_bytes(count as u32);
            self.tracks[idx].block_count = u16::to_le_bytes((seg.len() >> 9) as u16);
            let block_shift = (additional >> 9) as u16;
            let curr = u32::from_le_bytes(self.size);
            self.size = u32::to_le_bytes(curr + additional as u32);
            for i in idx+1..160 {
                let curr = u16::from_le_bytes(self.tracks[i].starting_block);
                self.tracks[i].starting_block = u16::to_le_bytes(curr + block_shift);
            }
        }
        Ok(())
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
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        self.id = [bytes[0],bytes[1],bytes[2],bytes[3]];
        self.size = [bytes[4],bytes[5],bytes[6],bytes[7]];
        self.tracks = Vec::new();
        self.bits = Vec::new();
        for track in 0..160 {
            let trk = Trk::from_bytes(&bytes[8+track*8..16+track*8].to_vec())?;
            self.tracks.push(trk);
        }
        let bitstream_bytes = u32::from_le_bytes(self.size) - 1280;
        if bitstream_bytes%512>0 {
            log::error!("WOZ bitstream is not an even number of blocks");
            return Err(DiskStructError::IllegalValue);
        }
        self.bits.append(&mut bytes[1288..].to_vec());
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Trks::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
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

impl Meta {
    /// Find an item in the META chunk by key.
    /// Return record number and value in a tuple.
    fn get_meta_item(&self,key: &str) -> Option<(usize,String)> {
        for i in 0..self.recs.len() {
            if self.recs[i].0==key {
                return Some((i,self.recs[i].1.to_string()));
            }
        }
        None
    }
    /// Some META keys must follow a specific pattern.
    /// Return true if `val` is allowed for the given `key`.
    /// This is hard coded to allow an empty string in all cases.
    fn verify_value(&self,key: &str,val: &str) -> bool {
        if val=="" {
            return true;
        }
        let map = HashMap::from(STD_META_OPTIONS);
        if let Some(valid_options) = map.get(key) {
            let re = regex::Regex::new(valid_options).expect("could not parse regex");
            return re.is_match(val);
        }
        true // if key is not in the map then any value is acceptable
    }
    /// Look for the key and replace its value, or else add
    /// a new record if the key is not found.
    fn add_or_replace(&mut self,key: &str,val: &str) -> STDRESULT {
        if key.contains("\t") {
            log::error!("META key contained a tab");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if key.contains("\n") {
            log::error!("META key contained a line feed");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if val.contains("\t") {
            log::error!("META value contained a tab");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        if val.contains("\n") {
            log::error!("META value contained a line feed");
            return Err(Box::new(img::Error::MetadataMismatch));
        }
        match self.get_meta_item(key) {
            Some((i,_)) => {
                self.recs[i] = (key.to_string(),val.to_string());
                Ok(())
            },
            None => {
                self.recs.push((key.to_string(),val.to_string()));
                Ok(())
            }
        }
    }
    /// Delete key if it exists, return true if it existed
    fn delete(&mut self,key: &str) -> bool {
        match self.get_meta_item(key) {
            Some((i,_)) => {
                log::warn!("deleting META record `{}`",key);
                self.recs.remove(i);
                true
            }
            _ => false
        }
    }
}

impl DiskStruct for Meta {
    fn new() -> Self where Self: Sized {
        Self {
            id: u32::to_le_bytes(META_ID),
            size: u32::to_le_bytes(8),
            recs: Vec::new()
        }        
    }
    fn len(&self) -> usize {
        let bytes = self.to_bytes();
        bytes.len()
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        self.id = [bytes[0],bytes[1],bytes[2],bytes[3]];
        self.size = [bytes[4],bytes[5],bytes[6],bytes[7]];
        if let Err(_) = String::from_utf8(bytes[8..].to_vec()) {
            log::warn!("Invalid UTF8 in WOZ META chunk, will use lossy conversion");
        }
        let s = String::from_utf8_lossy(&bytes[8..]);
        let lines: Vec<&str> = s.lines().collect();
        for i in 0..lines.len() {
            let cols: Vec<&str> = lines[i].split('\t').collect();
            if cols.len()!=2 {
                log::warn!("Wrong tab count in META item {}, skipping",lines[i]);
            } else {
                self.recs.push((cols[0].to_string(),cols[1].to_string()));
            }
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Meta::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        let mut s = String::new();
        // first load records into contiguous string
        for i in 0..self.recs.len() {
            s += &self.recs[i].0;
            s += "\t";
            s += &self.recs[i].1;
            s += "\n";
        }
        // now load the flattened chunk
        ans.append(&mut u32::to_le_bytes(META_ID).to_vec());
        ans.append(&mut u32::to_le_bytes(s.len() as u32).to_vec());
        ans.append(&mut s.as_bytes().to_vec());
        return ans;
    }
}

impl Woz2 {
    fn new() -> Self {
        Self {
            kind: img::DiskKind::Unknown,
            fmt: None,
            track_bits_offset: 0,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            flux: None,
            trks: Trks::new(),
            meta: None,
            writ: None,
            engine: TrackEngine::create(Method::Edit, false),
            cells: None,
            tmap_pos: usize::MAX,
        }
    }
    pub fn blank(kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS33_KIND && kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_400_KIND && kind!=img::names::A2_800_KIND {
            panic!("WOZ v2 permits only physical Apple 3.5 or 5.25 inch kinds");
        }
        Self {
            kind,
            fmt: None,
            track_bits_offset: 1536,
            header: Header::create(),
            info: Info::blank(kind),
            tmap: TMap::blank(),
            flux: None,
            trks: Trks::blank(),
            meta: None,
            writ: None,
            engine: TrackEngine::create(Method::Edit, false),
            cells: None,
            tmap_pos: usize::MAX,
        }
    }
    /// panics if `kind` is invalid
    pub fn create(vol: u8,kind: img::DiskKind,maybe_fmt: Option<img::tracks::DiskFormat>,flux: Vec<Track>) -> Result<Self,DYNERR> {
        let fmt = match maybe_fmt {
            Some(fmt) => fmt,
            None => img::woz::kind_to_format(&kind).unwrap()
        };
        let (bmap,fmap) = TMap::create(&kind,&fmt,flux)?;
        let trks = Trks::create(vol,&kind,&fmt,&bmap,&fmap)?;
        let mut info = Info::create(kind);
        info.update(&bmap,&fmap,&trks);
        Ok(Self {
            kind,
            fmt: Some(fmt),
            track_bits_offset: 1536,
            header: Header::create(),
            info,
            tmap: bmap,
            flux: fmap,
            trks,
            meta: None,
            writ: None,
            engine: TrackEngine::create(Method::Edit, false),
            cells: None,
            tmap_pos: usize::MAX,
        })
    }
    fn sanity_check(&self) -> Result<(),DiskStructError> {
        if self.flux.is_some() && (self.info.vers < 3 || self.info.flux_block==[0,0] || self.info.largest_flux_track==[0,0]) {
            log::error!("INFO chunk contradicts presence of FLUX chunk");
            return Err(DiskStructError::IllegalValue);
        }
        if self.flux.is_none() && self.info.vers >= 3 && (self.info.flux_block!=[0,0] || self.info.largest_flux_track!=[0,0]) {
            log::error!("INFO chunk contradicts absence of FLUX chunk");
            return Err(DiskStructError::IllegalValue);
        }
        if u32::from_le_bytes(self.info.id)!=INFO_ID || u32::from_le_bytes(self.tmap.id)!=TMAP_ID || u32::from_le_bytes(self.trks.id)!=TRKS_ID {
            log::debug!("Required chunk id was not as expected");
            return Err(DiskStructError::IllegalValue);
        }
        Ok(())
    }
    /// return index to TRK descriptor, and whether this is a FLUX track, does not change disk state
    fn try_motor(&self,tmap_idx: usize) -> Result<(usize,bool),DYNERR> {
        // if there is a flux track use it
        if let Some(fmap) = &self.flux {
            let ans = fmap.map[tmap_idx];
            if ans != 0xff {
                if ans as usize >= self.trks.tracks.len() {
                    return Err(Box::new(img::Error::InternalStructureAccess));
                } else {
                    return Ok((ans as usize,true));
                }
            }
        }
        // if no flux track look for a normal track
        let ans = self.tmap.map[tmap_idx];
        if ans == 0xff {
            log::info!("touched blank media at TMAP index {}",tmap_idx);
            Err(Box::new(img::Error::BlankTrack))
        } else if ans as usize >= self.trks.tracks.len() {
            Err(Box::new(img::Error::TrackNotFound))
        } else {
            Ok((ans as usize,false))
        }
    }
    /// Get a reference to the track data, valid for either bits or flux
    fn get_trk_data_ref(&self,trk: Track) -> Result<&[u8],DYNERR> {
        let tmap_idx = img::woz::get_tmap_index(trk,&self.kind)?;
        let (idx,_) = self.try_motor(tmap_idx)?;
        let [begin,end] = self.trks.rng(idx,self.track_bits_offset);
        Ok(&self.trks.bits[begin..end])
    }
    /// Save changes to the current track buffer if they exist, valid for either bits or flux.
    /// This will grow the TRKS chunk if necessary, but will never shrink it.
    fn write_back_track(&mut self) -> STDRESULT {
        if let Some(cells) = &self.cells {
            let (idx,_) = self.try_motor(self.tmap_pos)?;
            let (seg,count) = cells.to_woz_buf(None, 0);
            self.trks.update_segment(idx, &seg, count, self.track_bits_offset)?;
            self.info.update(&self.tmap,&self.flux,&self.trks);
        }
        Ok(())
    }
    /// Goto track and extract FluxCells if necessary, returns [motor,head,width]
    fn goto_track(&mut self,trk: Track) -> Result<[usize;3],DYNERR> {
        let tmap_idx = img::woz::get_tmap_index(trk,&self.kind)?;
        if self.tmap_pos != tmap_idx {
            log::debug!("goto {} of {}",trk,self.kind);
            let double_speed = match &self.kind {
                img::DiskKind::D35(_) => true,
                _ => false
            };
            self.write_back_track()?;
            self.tmap_pos = tmap_idx;
            let (idx,is_flux) = self.try_motor(tmap_idx)?;
            let [beg,end] = self.trks.rng(idx,self.track_bits_offset);
            let bits_or_bytes = u32::from_le_bytes(self.trks.tracks[idx].bit_count) as usize;
            let mut new_cells = match is_flux {
                true => {
                    log::debug!("this is a flux track");
                    FluxCells::from_woz_flux(bits_or_bytes, &self.trks.bits[beg..end],0,double_speed)
                },
                false => FluxCells::from_woz_bits(bits_or_bytes, &self.trks.bits[beg..end],0,double_speed)
            };
            if let Some(cells) = &self.cells {
                new_cells.sync_to_other_track(cells);
            }
            self.cells = Some(new_cells);
        }
        img::woz::get_motor_pos(trk, &self.kind)
    }
}

impl img::DiskImage for Woz2 {
    fn track_count(&self) -> usize {
        let mut ans = 0;
        for trk in &self.trks.tracks {
            if trk.bit_count != [0;4] {
                ans += 1;
            }
        }
        ans
    }
    fn end_track(&self) -> usize {
        let mut ans = 0;
        for i in 0..160 {
            if self.tmap.map[i] != 0xff {
                ans = i;
            }
        }
        match self.kind {
            img::DiskKind::D525(_) => ans / 4 + 1,
            _ => ans + 1
        }
    }
    fn num_heads(&self) -> usize {
        self.info.disk_sides as usize
    }
    fn motor_steps_per_cyl(&self) ->usize {
        match self.info.disk_type {
            1 => 4,
            2 => 1,
            _ => panic!("disk type not supported")
        }
    }
    fn nominal_capacity(&self) -> Option<usize> {
        match self.info.disk_type {
            1 => Some(280*512),
            2 => match self.info.disk_sides {
                1 => Some(800*512),
                2 => Some(1600*512),
                _ => panic!("sides must be 1 or 2")
            },
            _ => None
        }
    }
    fn actual_capacity(&mut self) -> Result<usize,DYNERR> {
        let mut ans = 0;
        if self.info.disk_type==1 {
            // simple strategy, advance by full tracks if we find something,
            // advance by half tracks if not.
            let mut motor = 0;
            while motor < 160 {
                match self.get_track_solution(Track::Motor((motor,0))) {
                    Ok(sol) => {
                        ans += sol.size_map.iter().sum::<usize>();
                        motor += 4;
                    },
                    Err(e) => match e.downcast_ref::<img::Error>() {
                        Some(img::Error::BlankTrack) => motor += 2,
                        _ => return Err(Box::new(img::Error::TrackAccess))
                    }
                }
            }
        } else if self.info.disk_type==2 {
            let max_track = match self.info.disk_sides {
                1 => 80,
                2 => 160,
                _ => 0
            };
            for track in 0..max_track {
                if let Ok(sol) = self.get_track_solution(Track::Num(track)) {
                    ans += sol.size_map.iter().sum::<usize>();
                }
            }
        }
        Ok(ans)
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
    fn change_format(&mut self,fmt: img::tracks::DiskFormat) -> STDRESULT {
        self.fmt = Some(fmt);
        Ok(())
    }
    fn change_method(&mut self,method: img::tracks::Method) {
        self.engine.change_method(method);
    }
    fn read_block(&mut self,addr: crate::bios::Block) -> Result<Vec<u8>,DYNERR> {
        apple::read_block(self, addr)
    }
    fn write_block(&mut self, addr: crate::bios::Block, dat: &[u8]) -> STDRESULT {
        apple::write_block(self, addr, dat)
    }
    fn read_sector(&mut self,trk: Track,sec: Sector) -> Result<Vec<u8>,DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        let [motor,head,width] = self.goto_track(trk)?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?;
        let (cyl,head) = (u8::try_from((motor+width/4)/width)?,u8::try_from(head)?);
        let hood = match self.kind {
            img::DiskKind::D525(_) => SectorHood::a2_525(254, cyl),
            _ => SectorHood::a2_35(cyl, head)
        };
        let ans = self.engine.read_sector(self.cells.as_mut().unwrap(),&hood,&sec,zfmt)?;
        Ok(ans)
    }
    fn write_sector(&mut self,trk: Track,sec: Sector,dat: &[u8]) -> Result<(),DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        let [motor,head,width] = self.goto_track(trk)?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?.clone();
        let (cyl,head) = (u8::try_from((motor+width/4)/width)?,u8::try_from(head)?);
        let hood = match self.kind {
            img::DiskKind::D525(_) => SectorHood::a2_525(254, cyl),
            _ => SectorHood::a2_35(cyl, head)
        };
        self.engine.write_sector(self.cells.as_mut().unwrap(),dat,&hood,&sec,&zfmt)?;
        Ok(())
    }
    fn from_bytes(buf: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        if buf.len()<12 {
            return Err(DiskStructError::UnexpectedSize);
        }
        let mut ans = Woz2::new();
        ans.header.update_from_bytes(&buf[0..12].to_vec())?;
        if ans.header.vers!=[0x57,0x4f,0x5a,0x32] {
            return Err(DiskStructError::IllegalValue);
        }
        log::info!("identified WOZ v2 header");
        let mut ptr: usize= 12;
        while ptr>0 {
            let (next,id,maybe_chunk) = img::woz::get_next_chunk(ptr, buf);
            match (id,maybe_chunk) {
                (INFO_ID,Some(chunk)) => ans.info.update_from_bytes(&chunk)?,
                (TMAP_ID,Some(chunk)) => ans.tmap.update_from_bytes(&chunk)?,
                (TRKS_ID,Some(chunk)) => {
                    ans.track_bits_offset = ptr + 1288;
                    ans.trks.update_from_bytes(&chunk)?
                },
                (FLUX_ID,Some(chunk)) => {
                    let mut new_flux = TMap::new();
                    new_flux.update_from_bytes(&chunk)?;
                    ans.flux = Some(new_flux);
                },
                (META_ID,Some(chunk)) => {
                    let mut new_meta = Meta::new();
                    new_meta.update_from_bytes(&chunk)?;
                    ans.meta = Some(new_meta);
                },
                (WRIT_ID,Some(chunk)) => ans.writ = Some(chunk),
                _ => if id!=0 {
                    log::info!("unprocessed chunk with id {:08X}/{}",id,String::from_utf8_lossy(&u32::to_le_bytes(id)))
                }
            }
            ptr = next;
        }
        ans.sanity_check()?;
        ans.kind = match (ans.info.disk_type,ans.info.boot_sector_format,ans.info.disk_sides) {
            (1,0,1) => img::names::A2_DOS33_KIND,
            (1,1,1) => img::names::A2_DOS33_KIND,
            (1,2,1) => img::names::A2_DOS32_KIND,
            (1,3,1) => img::names::A2_DOS33_KIND,
            (2,_,1) => img::names::A2_400_KIND,
            (2,_,2) => img::names::A2_800_KIND,
            _ => img::DiskKind::Unknown
        };
        for baseline_track in [0,3] {
            log::info!("baseline scan of track {}",baseline_track);
            if let Ok(_) = ans.get_track_solution(Track::Num(baseline_track)) {
                log::info!("setting disk kind to {}",ans.kind);
                return Ok(ans);
            }
        }
        log::warn!("no baseline, continuing with {}",ans.kind);
        return Ok(ans);
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        assert_eq!(self.track_bits_offset,1536);
        assert!(self.trks.bits.len() % 512 == 0);
        self.write_back_track().expect("could not restore track");
        self.info.update(&self.tmap,&self.flux,&self.trks);
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_bytes());
        ans.append(&mut self.info.to_bytes());
        ans.append(&mut self.tmap.to_bytes());
        ans.append(&mut self.trks.to_bytes());
        if let Some(flux) = &self.flux {
            log::debug!("writing flux chunk");
            ans.append(&mut flux.to_bytes());
        }
        if let Some(meta) = &self.meta {
            ans.append(&mut meta.to_bytes());
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
    fn get_track_buf(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR> {
        Ok(self.get_trk_data_ref(trk)?.to_vec())
    }
    fn set_track_buf(&mut self,trk: Track,dat: &[u8]) -> STDRESULT {
        let tmap_idx = img::woz::get_tmap_index(trk,&self.kind)?;
        let (idx,_) = self.try_motor(tmap_idx)?;
        let[beg,end] = self.trks.rng(idx,self.track_bits_offset);
        if end-beg != dat.len() {
            log::error!("source track buffer is {} bytes, destination track buffer is {} bytes",dat.len(),end-beg);
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        self.trks.bits[beg..end].copy_from_slice(dat);
        Ok(())
    }
    fn get_track_solution(&mut self,trk: Track) -> Result<img::TrackSolution,DYNERR> {
        let [motor,head,width] = self.goto_track(trk)?;
        // First try the given format if it exists
        if let Some(fmt) = &self.fmt {
            log::debug!("try current format");
            let zfmt = fmt.get_zone_fmt(motor,head)?;
            if let Ok((addr_map,size_map)) = self.engine.get_sector_map(self.cells.as_mut().unwrap(),zfmt) {
                let (addr_type,addr_mask) = match zfmt.addr_nibs() {
                    img::FieldCode::WOZ((4,4)) => ("VTSK",[255,255,255,255,0]),
                    img::FieldCode::WOZ((6,2)) => ("CSHFK",[255;5]),
                    _ => ("???",[0;5])
                };
                return Ok(zfmt.track_solution(motor,head,width,addr_map,size_map,addr_type,addr_mask));
            }
        }
        // If the given format fails try some standard ones
        if self.info.disk_type==1 {
            log::debug!("try DOS 3.2 format");
            self.kind = img::names::A2_DOS32_KIND;
            self.fmt = img::woz::kind_to_format(&self.kind);
            let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
            if let Ok((addr_map,size_map)) = self.engine.get_sector_map(self.cells.as_mut().unwrap(),zfmt) {
                if addr_map.len()==13 {
                    return Ok(zfmt.track_solution(motor,head,width,addr_map,size_map,"VTSK",[255,255,255,255,0]));
                }
            }
            log::debug!("try DOS 3.3 format");
            self.kind = img::names::A2_DOS33_KIND;
            self.fmt = img::woz::kind_to_format(&self.kind);
            let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
            if let Ok((addr_map,size_map)) = self.engine.get_sector_map(self.cells.as_mut().unwrap(),zfmt) {
                if addr_map.len()==16 {
                    return Ok(zfmt.track_solution(motor,head,width,addr_map,size_map,"VTSK",[255,255,255,255,0]));
                }
            }
            return Err(Box::new(img::Error::UnknownFormat));
        } else if self.info.disk_type==2 {
            self.kind = match self.info.disk_sides {
                1 => img::names::A2_400_KIND,
                2 => img::names::A2_800_KIND,
                _ => return Err(Box::new(img::Error::UnknownImageType))
            };
            self.fmt = img::woz::kind_to_format(&self.kind);
            let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
            if let Ok((addr_map,size_map)) = self.engine.get_sector_map(self.cells.as_mut().unwrap(),zfmt) {
                return Ok(zfmt.track_solution(motor,head,width,addr_map,size_map,"CSHFK",[255;5]));
            }
            return Err(Box::new(img::Error::UnknownFormat));
        }
        return Err(Box::new(img::Error::UnknownFormat));
    }
    fn export_geometry(&mut self,indent: Option<u16>) -> Result<String,DYNERR> {
        let pkg = img::package_string(&self.kind());
        let mut track_sols = Vec::new();
        if self.info.disk_type==1 {
            // simple strategy, advance by full tracks if we find something,
            // advance by half tracks if not.
            let mut motor = 0;
            while motor < 160 {
                match self.get_track_solution(Track::Motor((motor,0))) {
                    Ok(sol) => {
                        track_sols.push(sol);
                        motor += 4;
                    },
                    _ => {
                        motor += 2;
                    }
                }
            }
        } else if self.info.disk_type==2 {
            let max_track = match self.info.disk_sides {
                1 => 80,
                2 => 160,
                _ => return Err(Box::new(img::Error::UnknownImageType))
            };
            for track in 0..max_track {
                if let Ok(sol) = self.get_track_solution(Track::Num(track)) {
                    track_sols.push(sol);
                }
            }
        }
        img::geometry_json(pkg,track_sols,indent)
    }
    fn get_track_nibbles(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR> {
        let [motor,head,_] = self.goto_track(trk)?;
        let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
        Ok(self.engine.to_nibbles(self.cells.as_mut().unwrap(), zfmt))
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        let trk = super::woz::trk_from_tmap_idx(self.tmap_pos, &self.kind);
        let [motor,head,_] = img::woz::get_motor_pos(trk, &self.kind).expect("could not get head position");
        let zfmt = match img::tracks::get_zone_fmt(motor, head, &self.fmt) {
            Ok(z) => Some(z),
            _ => None
        };
        super::woz::track_string_for_display(0, &bytes, zfmt)
    }
    fn get_metadata(&self,indent: Option<u16>) -> String {
        let mut root = json::JsonValue::new_object();
        let woz2 = self.what_am_i().to_string();
        root[&woz2] = json::JsonValue::new_object();
        root[&woz2]["info"] = json::JsonValue::new_object();
        root[&woz2]["meta"] = json::JsonValue::new_object();
        getByteEx!(root,woz2,self.info.disk_type);
        root[&woz2]["info"]["disk_type"]["_pretty"] = json::JsonValue::String(match self.info.disk_type {
            1 => "Apple 5.25 inch".to_string(),
            2 => "Apple 3.5 inch".to_string(),
            _ => "Unexpected value".to_string()
        });
        getByte!(root,woz2,self.info.write_protected);
        getByte!(root,woz2,self.info.synchronized);
        getByte!(root,woz2,self.info.cleaned);
        root[&woz2]["info"]["creator"] = json::JsonValue::String(String::from_utf8_lossy(&self.info.creator).trim_end().to_string());
        if self.info.vers>=2 {
            getByte!(root,woz2,self.info.disk_sides);
            getByteEx!(root,woz2,self.info.boot_sector_format);
            root[&woz2]["info"]["boot_sector_format"]["_pretty"] = json::JsonValue::String(match self.info.boot_sector_format {
                0 => "Unknown".to_string(),
                1 => "Boots 16-sector".to_string(),
                2 => "Boots 13-sector".to_string(),
                3 => "Boots both".to_string(),
                _ => "Unexpected value".to_string()
            });
            getByte!(root,woz2,self.info.optimal_bit_timing);
            getHexEx!(root,woz2,self.info.compatible_hardware);
            let mut hardware = String::new();
            let hard_flags = u16::from_le_bytes(self.info.compatible_hardware);
            let mut hard_mask = 1 as u16;
            for machine in COMPATIBLE_HARDWARE_OPT {
                if hard_flags & hard_mask > 0 {
                    hardware += machine;
                    hardware += ", ";
                }
                hard_mask *= 2;
            }
            if hardware.len()==0 {
                root[&woz2]["info"]["compatible_hardware"]["_pretty"] = json::JsonValue::String("unknown".to_string());
            } else {
                root[&woz2]["info"]["compatible_hardware"]["_pretty"] = json::JsonValue::String(hardware);
            }
            getHexEx!(root,woz2,self.info.required_ram);
            let ram = u16::from_le_bytes(self.info.required_ram);
            root[&woz2]["info"]["required_ram"]["_pretty"] = json::JsonValue::String(match ram { 0 => "unknown".to_string(), _ => ram.to_string()+"K" });
            getHexEx!(root,woz2,self.info.largest_track);
            let lrg_trk = u16::from_le_bytes(self.info.largest_track);
            root[&woz2]["info"]["largest_track"]["_pretty"] = json::JsonValue::String(lrg_trk.to_string() + " blocks");
        }
        if self.info.vers>=3 {
            getHexEx!(root,woz2,self.info.flux_block);
            let flx_blk = u16::from_le_bytes(self.info.flux_block);
            root[&woz2]["info"]["flux_block"]["_pretty"] = json::JsonValue::String(["block ",&flx_blk.to_string()].concat());
            getHexEx!(root,woz2,self.info.largest_flux_track);
            let lrg_flx = u16::from_le_bytes(self.info.largest_flux_track);
            root[&woz2]["info"]["largest_flux_track"]["_pretty"] = json::JsonValue::String(lrg_flx.to_string() + " blocks");
        }

        if let Some(meta) = &self.meta {
            for (k,v) in &meta.recs {
                root[&woz2]["meta"][k] = json::JsonValue::String(v.to_string());
                if !meta.verify_value(k, v) {
                    log::warn!("illegal META value `{}` for key `{}`",v,k);
                }
            }
        }
        if let Some(spaces) = indent {
            json::stringify_pretty(root,spaces)
        } else {
            json::stringify(root)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            log::debug!("put key `{:?}` with val `{}`",key_path,val);
            meta::test_metadata(key_path, self.what_am_i())?;
            if key_path.len()>2 && key_path[0]=="woz2" && key_path[1]=="info" {
                if RO_META_ITEMS.contains(&key_path[2].as_str()) {
                    log::warn!("skipping read-only `{}`",key_path[2]);
                    return Ok(());
                }
                if !self.info.verify_value(&key_path[2], val) {
                    log::error!("INFO chunk key `{}` had a bad value `{}`",key_path[2],val);
                    return Err(Box::new(img::Error::MetadataMismatch));
                }
            }
            let woz2 = self.what_am_i().to_string();
            putByte!(val,key_path,woz2,self.info.write_protected);
            putByte!(val,key_path,woz2,self.info.synchronized);
            putByte!(val,key_path,woz2,self.info.cleaned);
            putStringBuf!(val,key_path,woz2,self.info.creator,0x20);
            // TODO: take some action if user is writing an item
            // that is not consistent with the INFO chunk version
            putByte!(val,key_path,woz2,self.info.disk_sides);
            putByte!(val,key_path,woz2,self.info.boot_sector_format);
            putByte!(val,key_path,woz2,self.info.optimal_bit_timing);
            putHex!(val,key_path,woz2,self.info.compatible_hardware);
            putHex!(val,key_path,woz2,self.info.required_ram);
            putHex!(val,key_path,woz2,self.info.largest_track);
            putHex!(val,key_path,woz2,self.info.flux_block);
            putHex!(val,key_path,woz2,self.info.largest_flux_track);
            
            if key_path[1]=="meta" {
                if key_path.len()!=3 {
                    log::error!("wrong depth in WOZ key path {:?}",key_path);
                    return Err(Box::new(img::Error::MetadataMismatch));
                }
                match self.meta.as_mut() {
                    None => {
                        self.meta = Some(Meta::new());
                    },
                    Some(_) => {}
                };
                if val=="" {
                    if self.meta.as_mut().unwrap().delete(&key_path[2]) {
                        return Ok(());
                    }
                }
                if !self.meta.as_ref().unwrap().verify_value(&key_path[2], val) {
                    log::error!("illegal META value `{}` for key `{}`",val,key_path[2]);
                    return Err(Box::new(img::Error::MetadataMismatch));
                }
                if !STD_META_KEYS.contains(&key_path[2].as_str()) {
                    log::warn!("`{}` is not a standard META key",key_path[2]);
                }
                return self.meta.as_mut().unwrap().add_or_replace(&key_path[2], val);
            } 
        }
        log::error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}
