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

use log::info;
use crate::fs;
const BLOCK_SIZE: usize = 512;

/// Enumerates disk image errors.  The `Display` trait will print equivalent long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("track count did not match request")]
    TrackCountMismatch,
	#[error("image size did not match the request")]
	ImageSizeMismatch,
    #[error("image type not compatible with request")]
    ImageTypeMismatch
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