//! # Disk Image Module
//! 
//! This is a container for disk image modules.  The disk image modules
//! serve as the underlying storage for file system modules.
//! 
//! Disk images are represented by the `DiskImage` trait.  Every kind of disk image
//! must respond to a file system's request for that file system's preferred allocation block.
//! Certain kinds of disk images must also provide encoding and decoding of track bits.
//! 
//! ## Sector Skews
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
//! addressing the image uses.

pub mod disk525;
pub mod dsk_d13;
pub mod dsk_do;
pub mod dsk_po;
pub mod woz;
pub mod woz1;
pub mod woz2;
pub mod imd;

use std::str::FromStr;
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

#[derive(PartialEq,Clone,Copy)]
pub enum DiskKind {
    /// no name for it; but may be detailed within image format in some cases
    Unknown,
    /// Apple II 5.25 inch disk with 13 sectors per track and 5-3 nibble encoding
    A2_525_13,
    /// Apple II 5.25 inch disk with 16 sectors per track and 6-2 nibble encoding
    A2_525_16,
    /// Abstract Apple II 3.5 inch disk
    A2_35,
    /// Abstract Apple II maximum ProDOS volume (32M)
    A2Max,
    /// Standard 8 inch CP/M disk with 26 sectors per track and FM encoding
    CPM1_8_26
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

impl FromStr for DiskKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "8in" => Ok(Self::CPM1_8_26),
            "5.25in" => Ok(Self::A2_525_16),
            "3.5in" => Ok(Self::A2_35),
            "hdmax" => Ok(Self::A2Max),
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

/// The main trait for working with any kind of disk image.
/// The corresponding trait object serves as storage for `DiskFS`.
pub trait DiskImage {
    fn track_count(&self) -> usize;
    fn byte_capacity(&self) -> usize;
    fn what_am_i(&self) -> DiskImageType;
    fn kind(&self) -> DiskKind;
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized;
    fn to_bytes(&self) -> Vec<u8>;
    /// Read a chunk (block or sector) from the image
    fn read_chunk(&self,addr: fs::Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a chunk (block or sector) to the image
    fn write_chunk(&mut self, addr: fs::Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
    /// Get the track buffer exactly in the form the image stores it
    fn get_track_buf(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
    /// Get the track bytes; bits are processed through a soft latch, if applicable
    fn get_track_bytes(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
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

/// If a data source is smaller than `quantum` bytes, pad it with zeros
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

/// Get block number and byte offset into block corresponding to
/// track and logical sector.  Returned in tuple (block,offset)
pub fn prodos_block_from_ts(track: usize,sector: usize) -> (usize,usize) {
    let block_offset: [usize;16] = [0,7,6,6,5,5,4,4,3,3,2,2,1,1,0,7];
    let byte_offset: [usize;16] = [0,0,256,0,256,0,256,0,256,0,256,0,256,0,256,256];
    return (8*track + block_offset[sector], byte_offset[sector]);
}

/// Get the two track and logical sector pairs corresponding to a block.
/// The returned array is arranged in order.
pub fn ts_from_prodos_block(block: usize) -> [[usize;2];2] {
    let sector1: [usize;8] = [0,13,11,9,7,5,3,1];
    let sector2: [usize;8] = [14,12,10,8,6,4,2,15];
    return [
        [(block/8), sector1[block % 8]],
        [(block/8), sector2[block % 8]]
    ];
}
