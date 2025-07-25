//! # BIOS module
//! 
//! This module is a place for any middleware we may require
//! between the `fs` and `img` modules.  It is named in analogy
//! with the CP/M concept of a BIOS as being (in part) a layer between
//! the BDOS and the physical disk.  Tasks that live here include:
//! 
//! * converting a block request into a sector request
//! * maintaining sector skewing tables
//! * maintaining parameter tables (such as CP/M DPB and FAT BPB)

pub mod skew;
pub mod dpb;
pub mod bpb;
pub mod fat;
pub mod blocks;

/// Enumerates bios errors.  The `Display` trait will print equivalent long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("unsupported disk kind")]
    UnsupportedDiskKind,
    #[error("incompatible disk kind")]
    IncompatibleDiskKind,
    #[error("problem accessing sector")]
    SectorAccess
}
