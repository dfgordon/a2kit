//! ## Disk Parameter Block Module
//! 
//! This contains disk parameter blocks (DPB) for CP/M disks.  There is no standard for storing a DPB on a CP/M
//! disk; all we know is the BIOS must generate it somehow.  As a result, we end up with the strategy of keeping
//! a few likely DPB's on hand to try heuristically.

use crate::fs::cpm::types::{DIR_ENTRY_SIZE,LOGICAL_EXTENT_SIZE,RECORD_SIZE};
use log::debug;
use std::fmt;

/// The Disk Parameter Block (DPB) was introduced with CP/M v2.
/// This allows CP/M to work with a variety of disk formats.
/// The DPB was stored somewhere in the BIOS.
/// The parameters are interdependent in a complicated way, see `verify` function.
/// Fields are public, but should be changed by hand only with caution.
#[derive(PartialEq,Eq,Clone)]
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
    pub phm: u8,
    /// capacity in bytes of the reserved tracks
    /// (a2kit extension useful for heuristics)
    pub reserved_track_capacity: usize
}

pub const A2_525: DiskParameterBlock = DiskParameterBlock {
    spt: 32,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 127,// some have this as 139 (erroneously?)
    drm: 47,// why is this not 63?  the last sector seems unused.
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 12,//0x8000,
    off: 3,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 3*32*128
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
    cks: 16,
    off: 2,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 2*26*128
};

/// This covers standard Osborne1 disks
pub const SSSD_525: DiskParameterBlock = DiskParameterBlock {
    spt: 20,
    bsh: 4,
    blm: 15,
    exm: 1,
    dsm: 45,
    drm: 63,
    al0: 0b10000000,
    al1: 0b00000000,
    cks: 16,
    off: 3,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 3*20*128
};

/// This covers upgraded Osborne1 disks
pub const SSDD_525_OFF3: DiskParameterBlock = DiskParameterBlock {
    spt: 40,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 184,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 16,
    off: 3,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 3*40*128
};

/// This covers Kaypro II disks.
/// 32*(DRM+1) uses half the blocks mapped by AL0.  The remainder are reserved OS blocks. 
pub const SSDD_525_OFF1: DiskParameterBlock = DiskParameterBlock {
    spt: 40,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 194,
    drm: 63,
    al0: 0b11110000,
    al1: 0b00000000,
    cks: 16,
    off: 1,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 40*128
};

/// This covers Kaypro 4 disks.
/// Kaypro sector id's are sequenced by cylinder, but we
/// still regard SPT as the count of sectors on only one side.
/// 32*(DRM+1) uses half the blocks mapped by AL0.  The remainder are reserved OS blocks.
pub const DSDD_525_OFF1: DiskParameterBlock = DiskParameterBlock {
    spt: 40,
    bsh: 4,
    blm: 15,
    exm: 1,
    dsm: 196,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 16,
    off: 1,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 40*128
};

// This covers Amstrad PCW9512 and maybe PCW8256 (at least 1 image has extra cylinder with 8 sectors)
// There is a "superblock" at track 0 sector 0 [40 cyl, 9 secs, sector shift 2, off 1, bsh 3, dir blocks 2]
pub const SSDD_525_AMSTRAD_184K: DiskParameterBlock = DiskParameterBlock {
    spt: 36,
    bsh: 3,
    blm: 7,
    exm: 0,
    dsm: 174,
    drm: 63,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 16,
    off: 1,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 36*128
};

pub const TRS80_M2: DiskParameterBlock = DiskParameterBlock {
    spt: 64,
    bsh: 4,
    blm: 15,
    exm: 0,
    dsm: 299,
    drm: 127,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 16,
    off: 2,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 26*128 + 16*512
};

pub const NABU: DiskParameterBlock = DiskParameterBlock {
    spt: 52,
    bsh: 4,
    blm: 15,
    exm: 0,
    dsm: 493,
    drm: 127,
    al0: 0b11000000,
    al1: 0b00000000,
    cks: 16,
    off: 2,
    psh: 0,
    phm: 0,
    reserved_track_capacity: 2*26*128
};

impl DiskParameterBlock {
    pub fn create(kind: &crate::img::DiskKind) -> Self {
        match *kind {
            crate::img::names::A2_DOS33_KIND => A2_525,
            crate::img::names::IBM_CPM1_KIND => CPM1,
            crate::img::names::OSBORNE1_SD_KIND => SSSD_525,
            crate::img::names::OSBORNE1_DD_KIND => SSDD_525_OFF3,
            crate::img::names::KAYPROII_KIND => SSDD_525_OFF1,
            crate::img::names::KAYPRO4_KIND => DSDD_525_OFF1,
            crate::img::names::AMSTRAD_184K_KIND => SSDD_525_AMSTRAD_184K,
            crate::img::names::TRS80_M2_CPM_KIND => TRS80_M2,
            crate::img::names::NABU_CPM_KIND => NABU,
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
            debug!("directory exceeds 16 blocks");
            return false;
        }
        let mut mask16 = self.al0 as u16 * 256 + self.al1 as u16;
        let mut contiguous_dir_blocks = 0;
        for _i in 0..16 {
            if mask16 & 0x8000 > 0 {
                contiguous_dir_blocks += 1;
                mask16 <<= 1;
            } else {
                break;
            }
        }
        if (contiguous_dir_blocks as usize) < (self.drm as usize + 1)*32/bls {
            debug!("block map fails to cover directory : {} contiguous blocks were provided",contiguous_dir_blocks);
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
    /// blocks available for directory, data, and reserved blocks (which are in user tracks)
    pub fn user_blocks(&self) -> usize {
        self.dsm as usize + 1
    }
    /// maximum directory entries
    pub fn dir_entries(&self) -> usize {
        self.drm as usize + 1
    }
    /// Number of directory blocks rounded up. Full block is not always used. Reserved blocks may follow.
    /// This assumes the directory blocks mapped in contiguous fashion.
    pub fn dir_blocks(&self) -> usize {
        let full_blocks = self.dir_entries()*DIR_ENTRY_SIZE/self.block_size();
        match (self.dir_entries() * DIR_ENTRY_SIZE) % self.block_size() {
            0 => full_blocks,
            _ => 1 + full_blocks
        }
    }
    /// how many blocks in the user area are reserved
    pub fn reserved_blocks(&self) -> usize {
        let mut ans = 0;
        let mut mask16 = self.al0 as u16 * 256 + self.al1 as u16;
        for _i in 0..16 {
            if mask16 & 0x8000 > 0 {
                ans += 1;
                mask16 <<= 1;
            }
        }
        ans
    }
    /// is block reserved according to the DPB bitmap
    pub fn is_reserved(&self,block: usize) -> bool {
        if block > 15 {
            return false;
        }
        ((self.al0 as u16 * 256 + self.al1 as u16) << block) & 0x8000 > 0
    }
    /// Work out the total byte capacity, accounting for OS tracks and unused "remainder sectors" on the last track.
    pub fn disk_capacity(&self) -> usize {
        let track_capacity = self.spt as usize * RECORD_SIZE;
        let user = self.user_blocks() * self.block_size();
        let remainder = user % track_capacity;
        if remainder>0 {
            return self.reserved_track_capacity + user + track_capacity - remainder;
        } else {
            return self.reserved_track_capacity + user;
        }
    }
}

/// Allows the DPB to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the struct can be converted to `String`.
impl fmt::Display for DiskParameterBlock {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            A2_525 => write!(f,"Apple 5.25 inch"),
            CPM1 => write!(f,"IBM 8 inch SSSD"),
            SSSD_525 => write!(f,"IBM 5.25 inch SSSD"),
            SSDD_525_OFF1 => write!(f,"IBM 5.25 inch SSDD"),
            SSDD_525_OFF3 => write!(f,"IBM 5.25 inch SSDD"),
            DSDD_525_OFF1 => write!(f,"IBM 5.25 inch DSSD"),
            SSDD_525_AMSTRAD_184K => write!(f,"IBM 5.25 inch SSDD"),
            TRS80_M2 => write!(f,"IBM 8 inch SSDD"),
            NABU => write!(f,"IBM 8 inch DSDD"),
            _ => write!(f,"unknown disk")
        }
    }
}
