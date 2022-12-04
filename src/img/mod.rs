//! # Disk Image Module
//! 
//! This is a container for disk image modules.  The disk image modules
//! serve the purpose of encoding/decoding disk tracks at a level below the
//! file system.  Hence there is no information about files, only collections of
//! data that fall within track, sector, or block boundaries.
//! 
//! Disk images are represented by the `DiskImage` trait.

pub mod disk525;
pub mod dsk_d13;
pub mod dsk_do;
pub mod dsk_po;
pub mod woz;
pub mod woz1;
pub mod woz2;

use std::str::FromStr;
use log::info;
use crate::fs;
const BLOCK_SIZE: usize = 512;

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
    ImageTypeMismatch
}

#[derive(PartialEq,Clone,Copy)]
pub enum DiskKind {
    A2_525_13,
    A2_525_16,
    A2_35,
    A2Max
}

#[derive(PartialEq,Clone,Copy)]
pub enum DiskImageType {
    D13,
    DO,
    PO,
    WOZ1,
    WOZ2
}

impl FromStr for DiskKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
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
    fn from_bytes(buf: &Vec<u8>) -> Option<Self> where Self: Sized;
    fn to_bytes(&self) -> Vec<u8>;
    /// Read a chunk (block or sector) from the image
    fn read_chunk(&self,addr: fs::ChunkSpec) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Write a chunk (block or sector) to the image
    fn write_chunk(&mut self, addr: fs::ChunkSpec, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>>;
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


/// Convert a DSK image from DOS order to ProDOS order.
/// Assumes the buffer is an appropriate size for the operation, abstract track counts are OK.
pub fn reorder_do_to_po(dsk: &Vec<u8>,sectors: usize) -> Vec<u8> {
    let mut ans = dsk.clone();
    let tracks = dsk.len()/sectors/256;
    for track in 0..tracks {
        for sector in 0..sectors {
            let (block,hoff) = fs::block_from_ts(track, sector);
            let doff = track*BLOCK_SIZE*8 + sector as usize*256;
            let poff = block as usize*BLOCK_SIZE + hoff;
            for byte in 0..256 {
                ans[poff+byte] = dsk[doff+byte];
            }
        }
    }
    return ans;
}

/// Convert a DSK image from ProDOS order to DOS order.
/// Assumes the buffer is an appropriate size for the operation, abstract track counts are OK.
pub fn reorder_po_to_do(dsk: &Vec<u8>,sectors: usize) -> Vec<u8> {
    let mut ans = dsk.clone();
    let tracks = dsk.len()/sectors/256;
    for track in 0..tracks {
        for sector in 0..sectors {
            let (block,hoff) = fs::block_from_ts(track, sector);
            let doff = track*BLOCK_SIZE*8 + sector as usize*256;
            let poff = block as usize*BLOCK_SIZE + hoff;
            for byte in 0..256 {
                ans[doff+byte] = dsk[poff+byte];
            }
        }
    }
    return ans;
}