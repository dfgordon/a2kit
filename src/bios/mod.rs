//! # BIOS module
//! 
//! This module is a place for any middleware we may require
//! between the `fs` and `img` modules.  It is named in analogy
//! with the CP/M concept of a BIOS as being (in part) a layer between
//! the BDOS and the physical disk.
//! 
//! All the sector skewing tables are kept in this module.
//! CP/M disk parameter blocks are here as well.

pub mod skew;
pub mod dpb;