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
//! `DiskKind::D35(_,_,_)`.  If you wanted to further limit this to disks with 6&2 nibbles, you could
//! use a pattern like `DiskKind::D35(_,NibbleCode::N62,_)`.
//! 
//! ## Sector Skews
//! 
//! The actual skew tables are maintained separately in `bios::skew`.
//! 
//! Disk addresses are often transformed one or more times as they propagate from a file
//! system request to a physical disk.  The file system may use an abstract unit, like a block,
//! which is transformed into a "logical" sector number, which is further transformed into a
//! physical sector address.  The physical sector address is the one actually stored on the disk track.
//! 
//! The way this is done within `a2kit` is as follows.  The `fs` module provides
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
pub struct SectorLayout {
    cylinders: usize,
    sides: usize,
    zones: usize,
    sectors: usize,
    sector_size: usize
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub enum DiskKind {
    Unknown,
    LogicalBlocks(BlockLayout),
    LogicalSectors(SectorLayout),
    D35(SectorLayout,NibbleCode,FluxCode),
    D525(SectorLayout,NibbleCode,FluxCode),
    D8(SectorLayout,NibbleCode,FluxCode)
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

/// Allows the disk kind to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for DiskKind {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DiskKind::LogicalBlocks(lay) => write!(f,"Logical disk, {} blocks",lay.block_count),
            DiskKind::LogicalSectors(lay) => write!(f,"Logical disk, {} x {} tracks",lay.cylinders,lay.sides),
            names::A2_400_KIND => write!(f,"Apple 3.5 inch 400K"),
            names::A2_800_KIND => write!(f,"Apple 3.5 inch 800K"),
            names::A2_DOS32_KIND => write!(f,"Apple 5.25 inch 13 sector"),
            names::A2_DOS33_KIND => write!(f,"Apple 5.25 inch 16 sector"),
            names::IBM_CPM1_KIND => write!(f,"IBM 8 inch SSSD"),
            names::OSBORNE_KIND => write!(f,"IBM 5.25 inch SSDD"),
            DiskKind::D35(lay,_,_) => write!(f,"3.5 inch {} x {} tracks",lay.cylinders,lay.sides),
            DiskKind::D525(lay,_,_) => write!(f,"5.25 inch {} x {} tracks",lay.cylinders,lay.sides),
            DiskKind::D8(lay,_,_) => write!(f,"8 inch {} x {} tracks",lay.cylinders,lay.sides),
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
            "5.25in-osborne" => Ok(names::OSBORNE_KIND),
            "5.25in" => Ok(names::A2_DOS33_KIND),
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
pub trait DiskImage {
    fn track_count(&self) -> usize;
    fn byte_capacity(&self) -> usize;
    fn what_am_i(&self) -> DiskImageType;
    fn kind(&self) -> DiskKind;
    fn change_kind(&mut self,kind: DiskKind);
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized;
    fn to_bytes(&self) -> Vec<u8>;
    /// Read a chunk (block or sector) from the image
    fn read_chunk(&self,addr: fs::Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a chunk (block or sector) to the image
    fn write_chunk(&mut self, addr: fs::Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
    /// Read a physical sector from the image
    fn read_sector(&self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a physical sector to the image
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
    /// Get the track buffer exactly in the form the image stores it; for user inspection
    fn get_track_buf(&self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Get the track bytes as aligned nibbles; for user inspection
    fn get_track_nibbles(&self,cyl: usize,head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
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

