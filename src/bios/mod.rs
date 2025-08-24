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

use std::fmt;

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

/// Encapsulates the disk address and addressing mode used by a file system.
/// Disk addresses generally involve some transformation between logical (file system) and physical (disk fields) addresses.
/// Disk images are responsible for serving blocks in response to a file system request, see the 'img' docstring for more.
/// The `Block` implementation includes a simple mapping from blocks to sectors; disk images can use this or not as appropriate.
#[derive(PartialEq,Eq,Clone,Copy,Hash)]
pub enum Block {
    /// value is [track,sector]
    D13([usize;2]),
    /// value is [track,sector]
    DO([usize;2]),
    /// value is block number
    PO(usize),
    /// value is (absolute block number, BSH, OFF); see cpm::types
    CPM((usize,u8,u16)),
    /// value is (first logical sector,num sectors)
    FAT((u64,u8)),
    /// value is [track,sector]
    D64([usize;2])
}

impl Block {
    /// Get a track-sector list for this block.
    /// At this level we can only assume a simple monotonically increasing relationship between blocks and sectors.
    /// Any further skewing must be handled by the caller.  CP/M and FAT offsets are accounted for.
    /// For CP/M be sure to use 128 byte logical sectors when computing `secs_per_track`.
    pub fn get_lsecs(&self,secs_per_track: usize) -> Vec<[usize;2]> {
        match self {
            Self::D13([t,s]) => vec![[*t,*s]],
            Self::DO([t,s]) => vec![[*t,*s]],
            Self::PO(_) => panic!("function `get_lsecs` not appropriate for ProDOS"),
            Self::CPM((block,bsh,off)) => {
                let mut ans: Vec<[usize;2]> = Vec::new();
                let lsecs_per_block = 1 << bsh;
                for sec_count in block*lsecs_per_block..(block+1)*lsecs_per_block {
                    ans.push([*off as usize + sec_count/secs_per_track , 1 + sec_count%secs_per_track]);
                }
                ans
            },
            Self::FAT((sec1,secs)) => {
                let mut ans: Vec<[usize;2]> = Vec::new();
                for sec in (*sec1 as usize)..(*sec1 as usize)+(*secs as usize) {
                    ans.push([sec/secs_per_track , sec%secs_per_track]);
                }
                ans
            },
            Self::D64([t,s]) => vec![[*t,*s]]
        }
    }
}
impl fmt::Display for Block {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13([t,s]) => write!(f,"D13 track {} sector {}",t,s),
            Self::DO([t,s]) => write!(f,"DOS track {} sector {}",t,s),
            Self::PO(b) => write!(f,"ProDOS block {}",b),
            Self::CPM((b,s,o)) => write!(f,"CPM block {} shift {} offset {}",b,s,o),
            Self::FAT((s1,secs)) => write!(f,"FAT cluster sec1 {} secs {}",s1,secs),
            Self::D64([t,s]) => write!(f,"D64 track {} sector {}",t,s)
        }
    }
}