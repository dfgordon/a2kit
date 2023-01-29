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
//! an enumeration called `Chunk` which identifies a disk address in a given file system's
//! own language.  Each disk image implementation has to provide `read_chunk` and `write_chunk`.
//! These functions have to be able to take a `Chunk` and transform it into whatever disk
//! addressing the image uses.  The tables in `bios::skew` are accessible to any image.

pub mod disk35;
pub mod disk525;
pub mod dsk_d13;
pub mod dsk_do;
pub mod dsk_po;
pub mod woz;
pub mod woz1;
pub mod woz2;
pub mod imd;
pub mod names;

use std::str::FromStr;
use std::fmt;
use log::info;
use crate::fs;

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
    SectorAccess
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
pub struct BlockLayout {
    block_size: usize,
    block_count: usize
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub struct TrackLayout {
    cylinders: [usize;5],
    sides: [usize;5],
    sectors: [usize;5],
    sector_size: [usize;5],
    flux_code: [FluxCode;5],
    nib_code: [NibbleCode;5]
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum DiskKind {
    Unknown,
    LogicalBlocks(BlockLayout),
    LogicalSectors(TrackLayout),
    D35(TrackLayout),
    D525(TrackLayout),
    D8(TrackLayout)
}

#[derive(PartialEq,Clone,Copy)]
pub enum DiskImageType {
    D13,
    DO,
    PO,
    WOZ1,
    WOZ2,
    IMD
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
}

/// Allows the track layout to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for TrackLayout {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cyls = 0;
        for c in self.cylinders {
            cyls += c;
        }
        write!(f,"{} cylinders",cyls)
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
            names::IBM_CPM1_KIND => write!(f,"IBM 8 inch SSSD"),
            names::OSBORNE1_SD_KIND => write!(f,"IBM 5.25 inch SSSD"),
            names::OSBORNE1_DD_KIND | names::KAYPROII_KIND => write!(f,"IBM 5.25 inch SSDD"),
            names::KAYPRO4_KIND => write!(f,"IBM 5.25 inch DSDD"),
            names::TRS80_M2_CPM_KIND => write!(f,"IBM 8 inch SSDD"),
            names::NABU_CPM_KIND => write!(f,"IBM 8 inch DSDD"),
            DiskKind::D35(lay) => write!(f,"3.5 inch {}",lay),
            DiskKind::D525(lay) => write!(f,"5.25 inch {}",lay),
            DiskKind::D8(lay) => write!(f,"8 inch {}",lay),
            DiskKind::Unknown => write!(f,"unknown")
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
            "5.25in-osb-sd" => Ok(names::OSBORNE1_SD_KIND),
            "5.25in-osb-dd" => Ok(names::OSBORNE1_DD_KIND),
            "5.25in-kayii" => Ok(names::KAYPROII_KIND),
            "5.25in-kay4" => Ok(names::KAYPRO4_KIND),
            "5.25in" => Ok(names::A2_DOS33_KIND), // mkdsk will change it if DOS 3.2 requested
            "3.5in" => Ok(names::A2_800_KIND),
            "hdmax" => Ok(names::A2_HD_MAX),
            "3.5in-ss" => Ok(names::A2_400_KIND),
            "3.5in-ds" => Ok(names::A2_800_KIND),
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
            "woz1" => Ok(Self::WOZ1),
            "woz2" => Ok(Self::WOZ2),
            "imd" => Ok(Self::IMD),
            _ => Err(Error::UnknownImageType)
        }
    }
}

// TODO: TrackBits owns its own track data:
// is this the clone anti-pattern?

pub trait TrackBits {
    /// Length of the track buffer in bytes
    fn len(&self) -> usize;
    /// Bits actually on the track
    fn bit_count(&self) -> usize;
    /// Rotate the disk to the reference bit
    fn reset(&mut self);
    /// Get the current displacement from the reference bit
    fn get_bit_ptr(&self) -> usize;
    /// Write physical sector (as identified by address field)
    fn write_sector(&mut self,dat: &Vec<u8>,track: u8,sector: u8) -> Result<(),NibbleError>;
    /// Read physical sector (as identified by address field)
    fn read_sector(&mut self,track: u8,sector: u8) -> Result<Vec<u8>,NibbleError>;
    /// Copy of the unfiltered track buffer
    fn to_buf(&self) -> Vec<u8>;
    /// Get aligned track nibbles; n.b. head position will move.
    fn to_nibbles(&mut self) -> Vec<u8>;
}

/// The main trait for working with any kind of disk image.
/// The corresponding trait object serves as storage for `DiskFS`.
/// Reading can mutate the object because the image may be keeping
/// track of the head position or other status indicators.
pub trait DiskImage {
    fn track_count(&self) -> usize;
    fn byte_capacity(&self) -> usize;
    fn what_am_i(&self) -> DiskImageType;
    fn file_extensions(&self) -> Vec<String>;
    fn kind(&self) -> DiskKind;
    fn change_kind(&mut self,kind: DiskKind);
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized;
    fn to_bytes(&mut self) -> Vec<u8>;
    /// Read a chunk (block or sector) from the image; can affect disk state
    fn read_chunk(&mut self,addr: fs::Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a chunk (block or sector) to the image
    fn write_chunk(&mut self, addr: fs::Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
    /// Read a physical sector from the image; can affect disk state
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a physical sector to the image
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
    /// Get the track buffer exactly in the form the image stores it; for user inspection
    fn get_track_buf(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Get the track bytes as aligned nibbles; for user inspection
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write the track to a string suitable for display, input should be pre-aligned nibbles, e.g. from `get_track_nibbles`
    fn display_track(&self,bytes: &Vec<u8>) -> String;
}

/// Test a buffer for a size match to DOS-oriented track and sector counts.
pub fn is_dos_size(dsk: &Vec<u8>,allowed_track_counts: &Vec<usize>,sectors: usize) -> Result<(),Box<dyn std::error::Error>> {
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
pub fn quantize_chunk(src: &Vec<u8>,quantum: usize) -> Vec<u8> {
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

