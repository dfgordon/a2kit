//! ## Disk Parameter Block Module
//! 
//! This contains disk parameter blocks (DPB) for CP/M disks.  There is no standard for storing a DPB on a CP/M
//! disk; all we know is the BIOS must generate it somehow.  As a result, we end up with the strategy of keeping
//! a few likely DPB's on hand to try heuristically.

use crate::fs::cpm::types::{DIR_ENTRY_SIZE,LOGICAL_EXTENT_SIZE,RECORD_SIZE};
use log::debug;

/// The Disk Parameter Block (DPB) was introduced with CP/M v2.
/// This allows CP/M to work with a variety of disk formats.
/// The DPB was stored somewhere in the BIOS.
/// The parameters are interdependent in a complicated way, see `verify` function.
/// Fields are public, but should be changed by hand only with caution.
pub struct DiskParameterBlock {
    /// number of 128-byte records per track
    pub spt: u16,
    /// block shift factor, records in block = 1 << bsh, bytes in block = 1 << bsh << 7
    pub bsh: u8,
    /// block mask, 2**bsh - 1, records in a block minus 1
    pub blm: u8,
    /// extent mask = logical extents per extent - 1.  Can be 0,1,3,7,15.
    /// Extent capacity is 16K * (EXM+1).
    /// The extent holds up to 16 8-bit refs for DSM<256, 8 16-bit refs otherwise.
    pub exm: u8,
    /// total blocks minus 1, not counting OS tracks
    pub dsm: u16,
    /// directory entries minus 1
    pub drm: u16,
    /// bitmap of directory blocks 1
    pub al0: u8,
    /// bitmap of directory blocks 2
    pub al1: u8,
    /// size of directory check vector
    pub cks: u16,
    /// number of reserved tracks, also track where directory starts
    pub off: u16,
    /// Physical record shift factor, PSH = log2(sector_bytes/128).
    /// Set to 0 if we don't need BDOS to translate.
    /// Requires CP/M v3 or higher.
    pub psh: u8,
    /// Physical record mask, PHM = sector_bytes/128 - 1
    /// Set to 0 if we don't need BDOS to translate.
    /// Requires CP/M v3 or higher.
    pub phm: u8
}

pub const A2_525: DiskParameterBlock = DiskParameterBlock {
    spt: 32,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 127,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 0x8000,
    off: 3,
    psh: 0,
    phm: 0
};

pub const CPM1: DiskParameterBlock = DiskParameterBlock {
    spt: 26,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 242,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 0x8000,
    off: 2,
    psh: 0,
    phm: 0
};

pub const OSBORNE1: DiskParameterBlock = DiskParameterBlock {
    spt: 40,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 184,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 0x8000,
    off: 3,
    psh: 0,
    phm: 0
};

impl DiskParameterBlock {
    pub fn create(kind: &crate::img::DiskKind) -> Self {
        match *kind {
            crate::img::names::A2_DOS33_KIND => A2_525,
            crate::img::names::IBM_CPM1_KIND => CPM1,
            crate::img::names::OSBORNE_KIND => OSBORNE1,
            _ => panic!("Disk kind not supported")
        }
    }
    /// Check that parameter dependencies are all satisfied.
    pub fn verify(&self) -> bool {
        // n.b. order of these checks can matter
        if self.bsh<3 || self.bsh>7 {
            debug!("BSH is invalid");
            return false;
        }
        if self.blm as usize!=num_traits::pow(2,self.bsh as usize)-1 {
            debug!("BLM must be 2^BSH-1");
            return false;
        }
        if self.dsm>0x7fff {
            debug!("block count exceeds maximum");
            return false;
        }
        if self.bsh==3 && self.dsm>0xff {
            debug!("block count exceeds maximum for 1K blocks");
            return false;
        }
        let bls = (128 as usize) << self.bsh as usize;
        let max_exm = match self.dsm {
            dsm if dsm<256 => 16*bls/LOGICAL_EXTENT_SIZE - 1,
            _ => 8*bls/LOGICAL_EXTENT_SIZE - 1
        };
        if self.exm as usize > max_exm {
            debug!("too many logical extents");
            return false;
        }
        match self.exm {
            0b0 | 0b1 | 0b11 | 0b111 | 0b1111 => {},
            _ => {
                debug!("invalid extent mask {}",self.exm);
                return false;
            }
        }
        if self.drm as usize + 1 > 16*bls/32 {
            debug!("too many directory entries");
            return false;
        }
        let mut entry_bits = 0;
        for i in 0..8 {
            entry_bits += (self.al0 >> i) & 0x01;
            entry_bits += (self.al1 >> i) & 0x01;
        }
        if entry_bits as usize != (self.drm as usize + 1)*32/bls {
            debug!("directory block map mismatch");
            return false;
        }
        if self.dir_blocks() > self.user_blocks() {
            debug!("directory end block out of range");
            return false;
        }
        return true;
    }
    /// size of block in bytes
    pub fn block_size(&self) -> usize {
        (128 as usize) << self.bsh as usize
    }
    /// size of block pointer in bytes
    pub fn ptr_size(&self) -> usize {
        match self.dsm {
            dsm if dsm<256 => 1,
            _ => 2
        }
    }
    /// capacity of a full extent in bytes
    pub fn extent_capacity(&self) -> usize {
        (self.exm as usize + 1) * LOGICAL_EXTENT_SIZE
    }
    /// blocks available for directory and data
    pub fn user_blocks(&self) -> usize {
        self.dsm as usize + 1
    }
    /// maximum directory entries
    pub fn dir_entries(&self) -> usize {
        self.drm as usize + 1
    }
    /// number of directory blocks
    pub fn dir_blocks(&self) -> usize {
        self.dir_entries()*DIR_ENTRY_SIZE/self.block_size()
    }
    /// Work out the total byte capacity, accounting for OS tracks and unused "remainder sectors" on the last track.
    /// This assumes that every track is used, and that all tracks have the same capacity (we cannot do better with
    /// what is provided in the DPB).
    pub fn disk_capacity(&self) -> usize {
        let track_capacity = self.spt as usize * RECORD_SIZE;
        let os = self.off as usize * track_capacity;
        let user = self.user_blocks() * self.block_size();
        let remainder = user % track_capacity;
        if remainder>0 {
            return os + user + track_capacity - remainder;
        } else {
            return os + user;
        }
    }
}