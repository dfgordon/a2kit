//! # File System Module
//! 
//! This is a container for file system modules.  File system modules handle
//! interactions with directories and files.  They are largely independent
//! of the `img` module, because they retain their own version of the disk data
//! in a convenient form.  N.b. this means you have
//! to explicitly transfer save changes to the original disk image if you want them
//! to be permanent.
//! 
//! File systems are represented by the `DiskFS` trait.

pub mod dos33;
pub mod prodos;
pub mod pascal;

use std::fmt;
use std::collections::HashMap;

/// Get block number and byte offset into block corresponding to
/// track and logical sector.  Returned in tuple (block,offset)
pub fn block_from_ts(track: usize,sector: usize) -> (usize,usize) {
    let block_offset: [usize;16] = [0,7,6,6,5,5,4,4,3,3,2,2,1,1,0,7];
    let byte_offset: [usize;16] = [0,0,256,0,256,0,256,0,256,0,256,0,256,0,256,256];
    return (8*track + block_offset[sector], byte_offset[sector]);
}

/// Get the two track and logical sector pairs corresponding to a block.
/// The returned array is arranged in order.
pub fn ts_from_block(block: usize) -> [[usize;2];2] {
    let sector1: [usize;8] = [0,13,11,9,7,5,3,1];
    let sector2: [usize;8] = [14,12,10,8,6,4,2,15];
    return [
        [(block/8), sector1[block % 8]],
        [(block/8), sector2[block % 8]]
    ];
}

/// The address (block number, or track-sector number) and type of a file system chunk.
/// Chunk addresses generally involve some transformation between logical (file system) and physical (disk fields) addresses.
/// The `ChunkSpec` implementation includes the necessary mappings.
/// There is a protocol that must be followed by DiskImage:
/// * Given a D13 chunk, return an error unless the image is 13 sectors.
/// * Given a DO chunk, always try to get the chunk.
/// * Given a PO chunk, return an error if the image is 13 sectors.
#[derive(PartialEq,Eq,Clone,Copy,Hash)]
pub enum ChunkSpec {
    D13([usize;2]),
    DO([usize;2]),
    PO(usize)
}

impl ChunkSpec {
    /// Return an ordered vector of track/sector pairs containing the chunk data.
    /// The number of pairs will generally be 1 (e.g. DOS) or 2 (e.g. proDOS).
    /// The returned sectors are physical, i.e., the addresses found on disk.
    pub fn get_ts_list(&self) -> Vec<[usize;2]> {
        let phys_sec: [usize;16] = [0,13,11,9,7,5,3,1,14,12,10,8,6,4,2,15];
        match self {
            Self::D13([t,s]) => vec![[*t,*s]],
            Self::DO([t,s]) => vec![[*t,phys_sec[*s]]],
            Self::PO(block) => {
                let [[t1,s1],[t2,s2]] = ts_from_block(*block);
                vec![[t1,phys_sec[s1]],[t2,phys_sec[s2]]]
            }
        }
    }
}
impl fmt::Display for ChunkSpec {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13([t,s]) => write!(f,"D13 track {} sector {}",t,s),
            Self::DO([t,s]) => write!(f,"DOS track {} sector {}",t,s),
            Self::PO(b) => write!(f,"ProDOS block {}",b)
        }
    }
}

/// Testing aid, adds offsets to the existing key, or create a new key if needed
pub fn add_ignorable_offsets(map: &mut HashMap<ChunkSpec,Vec<usize>>,key: ChunkSpec, offsets: Vec<usize>) {
    if let Some(val) = map.get(&key) {
        map.insert(key,[val.clone(),offsets].concat());
    } else {
        map.insert(key,offsets);
    }
}

/// Testing aid, combines offsets from two maps (used to fold in subdirectory offsets)
pub fn combine_ignorable_offsets(map: &mut HashMap<ChunkSpec,Vec<usize>>,other: HashMap<ChunkSpec,Vec<usize>>) {
    for (k,v) in other.iter() {
        add_ignorable_offsets(map, *k, v.clone());
    }
}