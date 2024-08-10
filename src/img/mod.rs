//! # Disk Image Module
//! 
//! This is a container for disk image modules.  The disk image modules
//! serve as the underlying storage for file system modules.
//! 
//! Disk images are represented by the `DiskImage` trait.  Every kind of disk image
//! must respond to a file system's request for that file system's preferred allocation block.
//! Certain kinds of disk images must also provide encoding and decoding of track bits.
//! 
//! ## Disk Kind Patterns
//! 
//! There is an enumeration called `DiskKind` that can be used to make branching decisions
//! based on parameters of a given disk.  This is intended to be used with rust's `match` statement.
//! As an example, if you want to do something only with 3.5 inch disks, you would use a pattern like
//! `DiskKind::D35(_)`.  The embedded structure can be used to limit the pattern to specific elements
//! of a track layout.
//! 
//! ## Sector Skews
//! 
//! The actual skew tables are maintained separately in `bios::skew`.
//! 
//! Disk addresses are often transformed one or more times as they propagate from a file
//! system request to a physical disk.  The file system may use an abstract unit, like a block,
//! which is transformed into a "logical" sector number, which is further transformed into a
//! "physical" address.  In a soft-sectoring scheme the physical address is encoded with
//! the other sector data.  If the physical addresses are out of order with respect to the
//! order in which they pass by the read/write head, we have a "physical" skew.
//! 
//! The way this is handled within `a2kit` is as follows.  The `fs` module provides
//! an enumeration called `Block` which identifies a disk address in a given file system's
//! own language.  Each disk image implementation has to provide `read_block` and `write_block`.
//! These functions have to be able to take a `Block` and transform it into whatever disk
//! addressing the image uses.  The tables in `bios::skew` are accessible to any image.

// TODO: the DiskStruct trait, defined in separate derive_macro crates, should be revised
// to return a Result, so we can error out instead of panicking if the image is bad.
// Also revise it to accept slices.

pub mod disk35;
pub mod disk525;
pub mod dsk_d13;
pub mod dsk_do;
pub mod dsk_po;
pub mod dsk_img;
pub mod dot2mg;
pub mod nib;
pub mod woz;
pub mod woz1;
pub mod woz2;
pub mod imd;
pub mod td0;
pub mod names;
pub mod meta;

use std::str::FromStr;
use std::fmt;
use log::{info,error};
use crate::fs;
use crate::{STDRESULT,DYNERR};

use a2kit_macro::DiskStructError;

/// Enumerates disk image errors.  The `Display` trait will print equivalent long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("unknown kind of disk")]
    UnknownDiskKind,
    #[error("unknown image type")]
    UnknownImageType,
    #[error("track count did not match request")]
    TrackCountMismatch,
	#[error("image size did not match the request")]
	ImageSizeMismatch,
    #[error("image type not compatible with request")]
    ImageTypeMismatch,
    #[error("unable to access sector")]
    SectorAccess,
    #[error("metadata mismatch")]
    MetadataMismatch
}

/// Errors pertaining to nibble encoding
#[derive(thiserror::Error,Debug)]
pub enum NibbleError {
    #[error("could not interpret track data")]
    BadTrack,
    #[error("invalid byte while decoding")]
    InvalidByte,
    #[error("bad checksum found in a sector")]
    BadChecksum,
    #[error("could not find bit pattern")]
    BitPatternNotFound,
    #[error("sector not found")]
    SectorNotFound,
    #[error("nibble type appeared in wrong context")]
    NibbleType
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum FluxCode {
    None,
    FM,
    GCR,
    MFM
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum NibbleCode {
    None,
    N44,
    N53,
    N62
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum DataRate {
    R250Kbps,
    R300Kbps,
    R500Kbps,
    R1000Kbps
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub struct BlockLayout {
    block_size: usize,
    block_count: usize
}

pub struct TrackSolution {
    cylinder: usize,
    head: usize,
    flux_code: FluxCode,
    nib_code: NibbleCode,
    chss_map: Vec<[usize;4]>
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub struct TrackLayout {
    cylinders: [usize;5],
    sides: [usize;5],
    sectors: [usize;5],
    sector_size: [usize;5],
    flux_code: [FluxCode;5],
    nib_code: [NibbleCode;5],
    data_rate: [DataRate;5]
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum DiskKind {
    Unknown,
    LogicalBlocks(BlockLayout),
    LogicalSectors(TrackLayout),
    D3(TrackLayout),
    D35(TrackLayout),
    D525(TrackLayout),
    D8(TrackLayout)
}

#[derive(PartialEq,Clone,Copy)]
pub enum DiskImageType {
    D13,
    DO,
    PO,
    IMG,
    WOZ1,
    WOZ2,
    IMD,
    DOT2MG,
    NIB,
    TD0
}

impl TrackLayout {
    pub fn track_count(&self) -> usize {
        let mut ans = 0;
        for i in 0..5 {
            ans += self.cylinders[i] * self.sides[i];
        }
        ans
    }
    pub fn sides(&self) -> usize {
        *self.sides.iter().max().unwrap()
    }
    pub fn zones(&self) -> usize {
        for i in 0..5 {
            if self.cylinders[i]==0 {
                return i;
            }
        }
        5
    }
    pub fn zone(&self,track_num: usize) -> usize {
        let mut tcount: [usize;5] = [0;5];
        tcount[0] = self.cylinders[0] * self.sides[0];
        for i in 1..5 {
            tcount[i] = tcount[i-1] + self.cylinders[i] * self.sides[i];
        }
        match track_num {
            n if n < tcount[0] => 0,
            n if n < tcount[1] => 1,
            n if n < tcount[2] => 2,
            n if n < tcount[3] => 3,
            _ => 4
        }
    }
    pub fn byte_capacity(&self) -> usize {
        let mut ans = 0;
        for i in 0..5 {
            ans += self.cylinders[i] * self.sides[i] * self.sectors[i] * self.sector_size[i];
        }
        ans
    }
}

/// Allows the track layout to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for TrackLayout {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cyls = 0;
        for c in self.cylinders {
            cyls += c;
        }
        write!(f,"{}/{}/{}/{}",cyls,self.sides(),self.sectors[0],self.sector_size[0])
    }
}

/// Allows the disk kind to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for DiskKind {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DiskKind::LogicalBlocks(lay) => write!(f,"Logical disk, {} blocks",lay.block_count),
            DiskKind::LogicalSectors(lay) => write!(f,"Logical disk, {}",lay),
            names::A2_400_KIND => write!(f,"Apple 3.5 inch 400K"),
            names::A2_800_KIND => write!(f,"Apple 3.5 inch 800K"),
            names::A2_DOS32_KIND => write!(f,"Apple 5.25 inch 13 sector"),
            names::A2_DOS33_KIND => write!(f,"Apple 5.25 inch 16 sector"),
            DiskKind::D3(lay) => write!(f,"3.0 inch {}",lay),
            DiskKind::D35(lay) => write!(f,"3.5 inch {}",lay),
            DiskKind::D525(lay) => write!(f,"5.25 inch {}",lay),
            DiskKind::D8(lay) => write!(f,"8 inch {}",lay),
            DiskKind::Unknown => write!(f,"unknown")
        }
    }
}

impl fmt::Display for NibbleCode {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            NibbleCode::N44 => write!(f,"4&4"),
            NibbleCode::N53 => write!(f,"5&3"),
            NibbleCode::N62 => write!(f,"6&2"),
            NibbleCode::None => write!(f,"none")
        }
    }
}

impl fmt::Display for FluxCode {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            FluxCode::FM => write!(f,"FM"),
            FluxCode::MFM => write!(f,"MFM"),
            FluxCode::GCR => write!(f,"GCR"),
            FluxCode::None => write!(f,"none")
        }
    }
}

/// Given a command line argument return a likely disk kind the user may want
impl FromStr for DiskKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "8in" => Ok(names::IBM_CPM1_KIND),
            "8in-trs80" => Ok(names::TRS80_M2_CPM_KIND),
            "8in-nabu" => Ok(names::NABU_CPM_KIND),
            "5.25in-ibm-ssdd8" => Ok(Self::D525(names::IBM_SSDD_8)),
            "5.25in-ibm-ssdd9" => Ok(Self::D525(names::IBM_SSDD_9)),
            "5.25in-ibm-dsdd8" => Ok(Self::D525(names::IBM_DSDD_8)),
            "5.25in-ibm-dsdd9" => Ok(Self::D525(names::IBM_DSDD_9)),
            "5.25in-ibm-ssqd" => Ok(Self::D525(names::IBM_SSQD)),
            "5.25in-ibm-dsqd" => Ok(Self::D525(names::IBM_DSQD)),
            "5.25in-ibm-dshd" => Ok(Self::D525(names::IBM_DSHD)),
            "5.25in-osb-sd" => Ok(names::OSBORNE1_SD_KIND),
            "5.25in-osb-dd" => Ok(names::OSBORNE1_DD_KIND),
            "5.25in-kayii" => Ok(names::KAYPROII_KIND),
            "5.25in-kay4" => Ok(names::KAYPRO4_KIND),
            "5.25in" => Ok(names::A2_DOS33_KIND), // mkdsk will change it if DOS 3.2 requested
            "3.5in" => Ok(names::A2_800_KIND),
            "3.5in-ss" => Ok(names::A2_400_KIND),
            "3.5in-ds" => Ok(names::A2_800_KIND),
            "3.5in-ibm-720" => Ok(Self::D35(names::IBM_720)),
            "3.5in-ibm-1440" => Ok(Self::D35(names::IBM_1440)),
            "3.5in-ibm-2880" => Ok(Self::D35(names::IBM_2880)),
            "3in-amstrad" => Ok(names::AMSTRAD_SS_KIND),
            "hdmax" => Ok(names::A2_HD_MAX),
            _ => Err(Error::UnknownDiskKind)
        }
    }
}

impl FromStr for DiskImageType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "d13" => Ok(Self::D13),
            "do" => Ok(Self::DO),
            "po" => Ok(Self::PO),
            "img" => Ok(Self::IMG),
            "woz1" => Ok(Self::WOZ1),
            "woz2" => Ok(Self::WOZ2),
            "imd" => Ok(Self::IMD),
            "2mg" => Ok(Self::DOT2MG),
            "2img" => Ok(Self::DOT2MG),
            "nib" => Ok(Self::NIB),
            "td0" => Ok(Self::TD0),
            _ => Err(Error::UnknownImageType)
        }
    }
}

impl fmt::Display for DiskImageType {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13 => write!(f,"d13"),
            Self::DO => write!(f,"do"),
            Self::PO => write!(f,"po"),
            Self::IMG => write!(f,"img"),
            Self::WOZ1 => write!(f,"woz1"),
            Self::WOZ2 => write!(f,"woz2"),
            Self::IMD => write!(f,"imd"),
            Self::DOT2MG => write!(f,"2mg"),
            Self::NIB => write!(f,"nib"),
            Self::TD0 => write!(f,"td0")
        }
    }
}

/// Lightweight trait object for reading and writing track bits.
/// The track buffer is borrowed.
pub trait TrackBits {
    /// get id of the track, usually sequence indexed from 0
    fn id(&self) -> usize;
    /// Bits actually on the track
    fn bit_count(&self) -> usize;
    /// Rotate the disk to the reference bit
    fn reset(&mut self);
    /// Get the current displacement from the reference bit
    fn get_bit_ptr(&self) -> usize;
    /// Set the current displacement from the reference bit
    fn set_bit_ptr(&mut self,displ: usize);
    /// Write physical sector (as identified by address field)
    fn write_sector(&mut self,bits: &mut [u8],dat: &[u8],track: u8,sector: u8) -> Result<(),NibbleError>;
    /// Read physical sector (as identified by address field)
    fn read_sector(&mut self,bits: &[u8],track: u8,sector: u8) -> Result<Vec<u8>,NibbleError>;
    /// Get aligned track nibbles; n.b. head position will move.
    fn to_nibbles(&mut self,bits: &[u8]) -> Vec<u8>;
    /// Get [cyl,head,sec] in time order, or return an error.  Head position will move.
    /// This can also be used to determine if the assumed encoding is valid.
    fn chs_map(&mut self,bits: &[u8]) -> Result<Vec<[usize;3]>,NibbleError>;
    /// Get [cyl,head,sec,size] in time order, or return an error.  Head position will move.
    /// This can also be used to determine if the assumed encoding is valid.
    fn chss_map(&mut self,bits: &[u8]) -> Result<Vec<[usize;4]>,NibbleError>;
}

/// The main trait for working with any kind of disk image.
/// The corresponding trait object serves as storage for `DiskFS`.
/// Reading can mutate the object because the image may be keeping
/// track of the head position or other status indicators.
pub trait DiskImage {
    fn track_count(&self) -> usize;
    fn num_heads(&self) -> usize;
    fn track_2_ch(&self,track: usize) -> [usize;2] {
        [track/self.num_heads(),track%self.num_heads()]
    }
    fn ch_2_track(&self,ch: [usize;2]) -> usize {
        ch[0]*self.num_heads() + ch[1]
    }
    /// N.b. this means bytes, not nibbles, e.g., a nibble buffer will be larger
    fn byte_capacity(&self) -> usize;
    fn what_am_i(&self) -> DiskImageType;
    fn file_extensions(&self) -> Vec<String>;
    fn kind(&self) -> DiskKind;
    fn change_kind(&mut self,kind: DiskKind);
    fn from_bytes(buf: &[u8]) -> Result<Self,DiskStructError> where Self: Sized;
    fn to_bytes(&mut self) -> Vec<u8>;
    /// Read a block from the image; can affect disk state
    fn read_block(&mut self,addr: fs::Block) -> Result<Vec<u8>,DYNERR>;
    /// Write a block to the image
    fn write_block(&mut self, addr: fs::Block, dat: &[u8]) -> STDRESULT;
    /// Read a physical sector from the image; can affect disk state
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR>;
    /// Write a physical sector to the image
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT;
    /// Get the track buffer exactly in the form the image stores it; for user inspection
    fn get_track_buf(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR>;
    /// Set the track buffer using another track buffer, the sizes must match
    fn set_track_buf(&mut self,cyl: usize,head: usize,dat: &[u8]) -> STDRESULT;
    /// Given a track index, get the physical CH, CHSS map, flux and nibble codes for the track.
    /// Implement this at a low level, making as few assumptions as possible.
    /// The expense of this operation can vary widely depending on the image type.
    /// No solution is not an error, i.e., we can return Ok(None).
    fn get_track_solution(&mut self,track: usize) -> Result<Option<TrackSolution>,DYNERR>;
    /// Get the track bytes as aligned nibbles; for user inspection
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR>;
    /// Write the track to a string suitable for display, input should be pre-aligned nibbles, e.g. from `get_track_nibbles`
    fn display_track(&self,bytes: &[u8]) -> String;
    /// Get image metadata into JSON string.
    /// Default contains only the image type.
    fn get_metadata(&self,indent: Option<u16>) -> String {
        let mut root = json::JsonValue::new_object();
        let typ = self.what_am_i().to_string();
        root[typ] = json::JsonValue::new_object();
        if let Some(spaces) = indent {
            json::stringify_pretty(root,spaces)
        } else {
            json::stringify(root)
        }
    }
    /// Add or change a single metadata item.  This is designed to take as its arguments the
    /// outputs produced by walking a JSON tree with `crate::JsonCursor`.
    /// The `key_path` has the keys leading up to the leaf, e.g., `/type/item/_raw`, and
    /// the `val` is the JSON value associated with the leaf (anything but an object).
    /// The special keys `_raw` and `_pretty` should be handled as follows.
    /// If a leaf is neither `_raw` nor `_pretty` treat it as raw.
    /// If a leaf is `_pretty` ignore it.
    fn put_metadata(&mut self,key_path: &Vec<String>, _val: &json::JsonValue) -> STDRESULT {
        meta::test_metadata(key_path,self.what_am_i())
    }
    /// Write the disk geometry, including all track solutions, into a JSON string
    fn export_geometry(&mut self,indent: Option<u16>) -> Result<String,DYNERR> {
        let mut solved_track_count = 0;
        let mut root = json::JsonValue::new_object();
        root["package"] = json::JsonValue::String(package_string(&self.kind()));
        let mut trk_ary = json::JsonValue::new_array();
        for trk in 0..self.track_count() {
            match self.get_track_solution(trk) {
                Ok(Some(sol)) => {
                    solved_track_count += 1;
                    let mut trk_obj = json::JsonValue::new_object();
                    trk_obj["cylinder"] = json::JsonValue::Number(sol.cylinder.into());
                    trk_obj["head"] = json::JsonValue::Number(sol.head.into());
                    trk_obj["flux_code"] = match sol.flux_code {
                        FluxCode::None => json::JsonValue::Null,
                        f => json::JsonValue::String(f.to_string())
                    };
                    trk_obj["nibble_code"] = match sol.nib_code {
                        NibbleCode::None => json::JsonValue::Null,
                        n => json::JsonValue::String(n.to_string())
                    };
                    trk_obj["chs_map"] = json::JsonValue::new_array();
                    for chss in sol.chss_map {
                        let mut chss_json = json::JsonValue::new_array();
                        chss_json.push(chss[0])?;
                        chss_json.push(chss[1])?;
                        chss_json.push(chss[2])?;
                        chss_json.push(chss[3])?;
                        trk_obj["chs_map"].push(chss_json)?;
                    }
                    trk_ary.push(trk_obj)?;
                },
                Ok(None) => trk_ary.push(json::JsonValue::Null)?,
                Err(e) => return Err(e)
            }
        }
        if solved_track_count==0 {
            root["tracks"] = json::JsonValue::Null;
        } else {
            root["tracks"] = trk_ary;
        }
        if let Some(spaces) = indent {
            Ok(json::stringify_pretty(root,spaces))
        } else {
            Ok(json::stringify(root))
        }
    }
}

/// Test a buffer for a size match to DOS-oriented track and sector counts.
pub fn is_dos_size(dsk: &Vec<u8>,allowed_track_counts: &Vec<usize>,sectors: usize) -> STDRESULT {
    let bytes = dsk.len();
    for tracks in allowed_track_counts {
        if bytes==tracks*sectors*256 {
            return Ok(());
        }
    }
    info!("image size was {}",bytes);
    return Err(Box::new(Error::ImageSizeMismatch));
}

/// If a data source is smaller than `quantum` bytes, pad it with zeros.
/// If it is larger, do not include the extra bytes.
pub fn quantize_block(src: &[u8],quantum: usize) -> Vec<u8> {
	let mut padded: Vec<u8> = Vec::new();
	for i in 0..quantum {
		if i<src.len() {
			padded.push(src[i])
		} else {
			padded.push(0);
		}
	}
    return padded;
}

/// Package designation for geometry JSON (e.g., "3.5", "5.25", ...)
pub fn package_string(kind: &DiskKind) -> String {
    match kind {
        DiskKind::D3(_) => "3".to_string(),
        DiskKind::D35(_) => "3.5".to_string(),
        DiskKind::D525(_) => "5.25".to_string(),
        DiskKind::D8(_) => "8".to_string(),
        DiskKind::LogicalBlocks(_) => "logical".to_string(),
        DiskKind::LogicalSectors(_) => "logical".to_string(),
        DiskKind::Unknown => "unknown".to_string()
    }
}
