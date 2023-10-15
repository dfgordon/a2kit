//! ## BIOS Parameter Block Module
//! 
//! This contains the BIOS parameter block (BPB) used with FAT volumes.
//! Implementation is based on Microsoft Hardware White Paper,
//! "FAT: General Overview of On-Disk Format,"" Dec. 6, 2000.

use log::debug;
use crate::{STDRESULT,DYNERR};

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;

const JMP_BOOT: [u8;3] = [0xeb,0x58,0x90];
const OEM_NAME: [u8;8] = *b"A2KITX.X";
const BOOT_SIGNATURE: [u8;2] = [0x55,0xaa]; // goes in boot[510..512]
const RCH: &str = "unreachable was reached";

/// Introduced with MS-DOS 2.0, appears starting at byte 11 of the boot sector,
/// following `JMP_BOOT` and `OEM_NAME`.
/// The last field `tot_sec_32` was introduced with MS-DOS 3.0.
/// The FAT32 fields and the tail fields are not included.
/// These fields are applicable to all FAT file systems.
#[derive(DiskStruct)]
pub struct BPBFoundation {
    /// 512, 1024, 2048, or 4096
    pub bytes_per_sec: [u8;2],
    /// 1, 2, 4, 8, 16, 32, 64, or 128.
    /// The cluster size must not come out to > 32K.
    pub sec_per_clus: u8,
    /// usually 1 for FAT12 or FAT16, 32 for FAT32
    pub reserved_sectors: [u8;2],
    /// usually 2
    pub num_fats: u8,
    /// Directory entries in the root directory, must be 0 for FAT32.
    /// The root directory should take up an integral number of sectors.
    /// For FAT16 512 is recommended.
    pub root_ent_cnt: [u8;2],
    /// 16-bit sector count, superceded by tot_sec_32 if 0.
    /// Includes all areas, but can be less than total disk capacity.
    pub tot_sec_16: [u8;2],
    /// 0xf0,0xf8,0xf9,0xfa,0xfb,0xfc,0xfd,0xfe,0xff.
    /// Value should also be put in FAT[0] in the low 8 bits.
    /// typical values are 0xf0 (removable) and 0xf8 (fixed).
    /// based on 86BOX :
    /// 0xf9 = 1200K
    /// 0xfb = 640K
    /// 0xfc = 180K
    /// 0xfd = 360K
    /// 0xfe = 160K
    /// 0xff = 320K
    pub media: u8,
    /// count of sectors occupied by one FAT, should be 0 for FAT32
    pub fat_size_16: [u8;2],
    /// sectors per track for interrupt 0x13
    pub sec_per_trk: [u8;2],
    /// number of heads for interrupt 0x13
    pub num_heads: [u8;2],
    /// hidden sectors preceding this FAT volume's partition,
    /// set to 0 for non-partitioned media.
    pub hidd_sec: [u8;4],
    /// 32-bit sector count, if 0 use tot_sec_16.
    /// This field was added in MS-DOS 3.0.
    pub tot_sec_32: [u8;4],
}

/// Introduced with Windows 95, appears starting at byte 36 of the boot sector.
#[derive(DiskStruct)]
pub struct BPBExtension32 {
    /// 32-bit version of fat_size
    pub fat_size_32: [u8;4],
    /// bits 0-3 = active FAT, 4-6 reserved, 7 = disable mirroring, 8-15 reserved
    pub flags:  [u8;2],
    /// high byte = major, low byte = minor
    pub fs_version: [u8;2],
    /// cluster number of the root directory, usually 2
    pub root_cluster: [u8;4],
    /// sector number of FSINFO structure, usually 1.
    /// The boot sectors and backup boot sectors both point to the same FSInfo sector,
    /// even though a backup FSInfo exists in the backup sectors.
    pub fs_info: [u8;2],
    /// if non-zero, indicates sector number of the backup boot record, usually 6.
    /// This is the start of the backup boot record, which may be multiple sectors.
    pub bk_boot_sec: [u8;2],
    /// reserved, set to 0
    pub reserved: [u8;12]
}

/// This follows the BPB, whether it is the FAT12/16 BPB or the FAT32 BPB.
#[derive(DiskStruct)]
pub struct BPBTail {
    /// interrupt 0x13 drive number (0x00 for floppy, 0x80 for hard disk, MS-DOS specific)
    pub drv_num: u8,
    /// set to 0
    pub reserved1: u8,
    /// signature (0x29) indicating following 3 fields are present
    pub boot_sig: u8,
    /// volume serial number, can generate using 32-bit timestamp
    pub vol_id: [u8;4],
    /// volume label, matches root directory label if it exists, otherwise "NO NAME    " 
    pub vol_lab: [u8;11],
    /// file system type only for display, "FAT12", "FAT16", or "FAT32" padded with spaces.
    /// Should not be used to determine the FAT type.
    pub fil_sys_type: [u8;8]
}

/// This has its own sector, given by BPBExtension32.fs_info, usually sector 1
#[derive(DiskStruct)]
pub struct Info {
    pub lead_sig: [u8;4],
    pub reserved1: [u8;480],
    pub struc_sig: [u8;4],
    pub free_count: [u8;4],
    pub nxt_free: [u8;4],
    pub reserved2: [u8;12],
    pub trail_sig: [u8;4]
}

impl BPBFoundation {
    pub fn verify(&self) -> bool {
        let mut ans = true;
        let bytes = u16::from_le_bytes(self.bytes_per_sec) as u64;
        if ![512,1024,2048,4096].contains(&bytes) {
            debug!("invalid bytes per sector {}",bytes);
            ans = false;
        }
        if ![1,2,4,8,16,32,64,128].contains(&self.sec_per_clus) {
            debug!("invalid sectors per cluster {}",self.sec_per_clus);
            ans = false;
        }
        if self.reserved_sectors==[0,0] {
            debug!("invalid count of reserved sectors 0");
            ans = false;
        }
        if self.num_fats==0 {
            debug!("invalid count of FATs 0");
            ans = false;
        }
        let entries = u16::from_le_bytes(self.root_ent_cnt) as u64;
        if bytes > 0 && (entries*32)%bytes != 0 {
            debug!("invalid entry count {}",entries);
            ans = false;
        }
        if self.tot_sec_16==[0,0] && self.tot_sec_32==[0,0,0,0] {
            debug!("invalid sector count 0");
            ans = false;
        }
        ans
    }
    pub fn sec_size(&self) -> u64 {
        u16::from_le_bytes(self.bytes_per_sec) as u64
    }
    pub fn block_size(&self) -> u64 {
        self.sec_per_clus() as u64 * self.sec_size()
    }
    pub fn heads(&self) -> u64 {
        u16::from_le_bytes(self.num_heads) as u64
    }
    pub fn secs_per_track(&self) -> u64 {
        u16::from_le_bytes(self.sec_per_trk) as u64
    }
    pub fn tot_sec(&self) -> u64 {
        match self.tot_sec_16 {
            [0,0] => u32::from_le_bytes(self.tot_sec_32) as u64,
            _ => u16::from_le_bytes(self.tot_sec_16) as u64
        }
    }
    pub fn res_secs(&self) -> u16 {
        u16::from_le_bytes(self.reserved_sectors)
    }
    pub fn root_dir_secs(&self) -> u64 {
        let bytes = u16::from_le_bytes(self.bytes_per_sec) as u64;
        let entries = u16::from_le_bytes(self.root_ent_cnt) as u64;
        if bytes==0 {
            return u16::MAX as u64;
        }
        (entries*32 + bytes - 1) / bytes
    }
    pub fn root_dir_entries(&self) -> u64 {
        u16::from_be_bytes(self.root_ent_cnt) as u64
    }
    pub fn sec_per_clus(&self) -> u8 {
        // TODO: why is this often/always erroneously set to 2 for 160K and 180K disks?
        // for now we override it in the FS layer when such disks are detected.
        self.sec_per_clus
    }
}

/// This represents and manages the data in the boot sector,
/// which includes the BPB, along with some other information.
pub struct BootSector {
    jmp: [u8;3],
    oem: [u8;8],
    foundation: BPBFoundation,
    /// This is always read, but if it turns out we have a FAT12/16,
    /// the tail data and remainder are obtained by rewinding.
    extension32: BPBExtension32,
    tail: BPBTail,
    /// Whatever follows, including the signature.
    /// Remainder length depends on FAT type and sector size.
    remainder: Vec<u8>
}

impl DiskStruct for BootSector {
    fn new() -> Self where Self: Sized {
        Self {
            jmp: [0,0,0],
            oem: [0;8],
            foundation: BPBFoundation::new(),
            extension32: BPBExtension32::new(),
            tail: BPBTail::new(),
            remainder: Vec::new()
        }
    }
    fn len(&self) -> usize {
        13 + self.foundation.len() + self.extension32.len() + self.tail.len() 
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        // suppose we have a FAT32
        let tentative = Self {
            jmp: [0,0,0],
            oem: [0;8],
            foundation: BPBFoundation::from_bytes(&bytes[11..36].to_vec()),
            extension32: BPBExtension32::from_bytes(&bytes[36..64].to_vec()),
            tail: BPBTail::from_bytes(&bytes[64..90].to_vec()),
            remainder: bytes[90..].to_vec()
        };
        // Setup the right FAT type using info from the supposed FAT32.
        // This can panic if the FAT data is unverified.
        self.jmp = bytes[0..3].try_into().expect(RCH);
        self.oem = bytes[3..11].try_into().expect(RCH);
        self.foundation = BPBFoundation::from_bytes(&bytes[11..36].to_vec());
        self.extension32 = BPBExtension32::from_bytes(&bytes[36..64].to_vec());
        self.tail = match tentative.fat_type() {
            32 => BPBTail::from_bytes(&bytes[64..90].to_vec()),
            _ => BPBTail::from_bytes(&bytes[36..62].to_vec())
        };
        self.remainder = match tentative.fat_type() {
            32 => bytes[90..].to_vec(),
            _ => bytes[62..].to_vec()
        };
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        match self.fat_type() {
            32 => {
                ans.append(&mut self.jmp.to_vec());
                ans.append(&mut self.oem.to_vec());
                ans.append(&mut self.foundation.to_bytes());
                ans.append(&mut self.extension32.to_bytes());
                ans.append(&mut self.tail.to_bytes());
                ans.append(&mut self.remainder.clone());
            },
            _ => {
                ans.append(&mut self.jmp.to_vec());
                ans.append(&mut self.oem.to_vec());
                ans.append(&mut self.foundation.to_bytes());
                ans.append(&mut self.tail.to_bytes());
                ans.append(&mut self.remainder.clone());
            }
        }
        ans
    }
}

impl BootSector {
    pub fn create(kind: &crate::img::DiskKind) -> Result<Self,DYNERR> {
        use crate::img::names;
        use crate::img::DiskKind::{D35,D525};
        match kind {
            D525(names::IBM_SSDD_8) => Ok(Self::create1216(SSDD_525_8)),
            D525(names::IBM_SSDD_9) =>  Ok(Self::create1216(SSDD_525_9)),
            D525(names::IBM_DSDD_8) =>  Ok(Self::create1216(DSDD_525_8)),
            D525(names::IBM_DSDD_9) =>  Ok(Self::create1216(DSDD_525_9)),
            D525(names::IBM_DSQD) =>  Ok(Self::create1216(DSQD_525)),
            D525(names::IBM_DSHD) =>  Ok(Self::create1216(DSHD_525)),
            D35(names::IBM_720) =>  Ok(Self::create1216(D35_720)),
            D35(names::IBM_1440) =>  Ok(Self::create1216(D35_1440)),
            D35(names::IBM_2880) =>  Ok(Self::create1216(D35_2880)),
            _ => Err(Box::new(super::Error::UnsupportedDiskKind))
        }
    }
    fn create1216(bpb: BPBFoundation) -> Self {
        let tail = BPBTail::new();
        let sec_size = bpb.sec_size() as usize;
        let used = JMP_BOOT.len() + OEM_NAME.len() + bpb.len() + tail.len();
        let mut remainder: Vec<u8> = vec![0;sec_size - used];
        remainder[510-used] = BOOT_SIGNATURE[0];
        remainder[511-used] = BOOT_SIGNATURE[1];
        let mut oem = OEM_NAME;
        oem[5..8].copy_from_slice(&env!("CARGO_PKG_VERSION").as_bytes()[0..3]);
        Self {
            jmp: JMP_BOOT,
            oem,
            foundation: bpb,
            extension32: BPBExtension32::new(),
            tail,
            remainder 
        }
    }
    /// This replaces the BPB foundation fields with a tabulated one.
    /// This is used when we detect a 160K or 180K disk, where the BPB data cannot be relied on.
    pub fn replace_foundation(&mut self,kind: &crate::img::DiskKind) -> STDRESULT {
        let lookup = Self::create(kind)?;
        self.foundation = lookup.foundation;
        Ok(())
    }
    /// Verify that the sector data is a valid boot sector,
    /// should be called before unpacking with from_bytes.
    pub fn verify(sec_data: &Vec<u8>) -> bool {
        let mut ans = true;
        if sec_data.len()<512 {
            debug!("sector too small");
            return false;
        }
        let signature = [sec_data[510],sec_data[511]];
        if signature!=BOOT_SIGNATURE {
            debug!("signature mismatch");
            ans = false;
        }
        let bpb = BPBFoundation::from_bytes(&sec_data[11..36].to_vec());
        ans |= bpb.verify();
        let ext32 = BPBExtension32::from_bytes(&sec_data[36..64].to_vec());
        let fat_secs = match bpb.fat_size_16 {
            [0,0] => u32::from_le_bytes(ext32.fat_size_32) as u64,
            _ => u16::from_le_bytes(bpb.fat_size_16) as u64
        };
        if fat_secs==0 {
            debug!("invalid count of FAT sectors 0");
            ans = false;
        }
        if bpb.tot_sec() <= bpb.res_secs() as u64 + (bpb.num_fats as u64 * fat_secs) + bpb.root_dir_secs() {
            debug!("data region came out 0 or negative");
            ans = false;
        }
        if ans {
            debug!("BPB counts: {}({}) FAT, {} tot, {} res, {} root",fat_secs,bpb.num_fats,bpb.tot_sec(),bpb.res_secs(),bpb.root_dir_secs());
        }
        ans
    }
    pub fn label(&self) -> Option<[u8;11]> {
        if self.tail.boot_sig==0x29 && self.tail.vol_lab!=[0x20;11] {
            Some(self.tail.vol_lab)
        } else {
            None
        }
    }
    pub fn sec_size(&self) -> u64 {
        self.foundation.sec_size()
    }
    pub fn block_size(&self) -> u64 {
        self.foundation.block_size()
    }
    pub fn heads(&self) -> u64 {
        self.foundation.heads()
    }
    pub fn secs_per_track(&self) -> u64 {
        self.foundation.secs_per_track()
    }
    pub fn tot_sec(&self) -> u64 {
        self.foundation.tot_sec()
    }
    pub fn res_secs(&self) -> u16 {
        self.foundation.res_secs()
    }
    pub fn secs_per_clus(&self) -> u8 {
        self.foundation.sec_per_clus()
    }
    pub fn media_byte(&self) -> u8 {
        self.foundation.media
    }
    /// only meaningful for FAT32
    pub fn root_dir_cluster1(&self) -> u64 {
        u32::from_le_bytes(self.extension32.root_cluster) as u64
    }
    // count of entries in root directory, zero for FAT32
    pub fn root_dir_entries(&self) -> u64 {
        self.foundation.root_dir_entries()
    }
    /// sectors used by the root directory, rounding up, zero for FAT32
    pub fn root_dir_secs(&self) -> u64 {
        self.foundation.root_dir_secs()
    }
    pub fn num_fats(&self) -> u64 {
        self.foundation.num_fats as u64
    }
    /// sectors occupied by 1 FAT
    pub fn fat_secs(&self) -> u64 {
        match self.foundation.fat_size_16 {
            [0,0] => u32::from_le_bytes(self.extension32.fat_size_32) as u64,
            _ => u16::from_le_bytes(self.foundation.fat_size_16) as u64
        }
    }
    pub fn root_dir_sec_rng(&self) -> [u64;2] {
        [
            self.res_secs() as u64 + (self.foundation.num_fats as u64 * self.fat_secs()),
            self.res_secs() as u64 + (self.foundation.num_fats as u64 * self.fat_secs()) + self.root_dir_secs()
        ]
    }
    pub fn data_rgn_secs(&self) -> u64 {
        self.tot_sec() - (self.res_secs() as u64 + (self.foundation.num_fats as u64 * self.fat_secs()) + self.root_dir_secs())
    }
    /// total clusters used, rounding down (remainder partial-cluster is not used)
    pub fn cluster_count(&self) -> u64 {
        self.data_rgn_secs()/self.foundation.sec_per_clus() as u64
    }
    /// FAT type determination based on the cluster count.
    /// These peculiar cutoffs are correct according to MS.
    pub fn fat_type(&self) -> usize {
        match self.cluster_count() {
            x if x < 4085 => 12,
            x if x < 65525 => 16,
            _ => 32
        }
    }
    /// Locate FAT entry for cluster n as [sec,offset].
    /// For FAT12, the last 4 bits may be in the next sector.
    pub fn cluster_ref(&self,n: u64) -> [u64;2] {
        let by_per_sec = u16::from_le_bytes(self.foundation.bytes_per_sec) as u64;
        match self.fat_type() {
            12 => {
                let sec = self.res_secs() as u64 + (n + (n/2))/by_per_sec;
                let offset = (n + (n/2)) % by_per_sec;
                [sec,offset]
            },
            16 => {
                let sec = self.res_secs() as u64 + (n*2) / by_per_sec;
                let offset = (n*2) % by_per_sec;
                [sec,offset]
            },
            32 => {
                let sec = self.res_secs() as u64 + (n*4) / by_per_sec;
                let offset = (n*4) % by_per_sec;
                [sec,offset]
            }
            _ => panic!("unexpected FAT type")
        }
    }
    pub fn first_data_sec(&self) -> u64 {
        self.res_secs() as u64 + self.foundation.num_fats as u64 * self.fat_secs() + self.root_dir_secs()
    }
    pub fn first_cluster_sec(&self,n: u64) -> u64 {
        (n-2)*self.foundation.sec_per_clus() as u64 + self.first_data_sec()
    }
}

const SSDD_525_8: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 1,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x40),
    tot_sec_16: u16::to_le_bytes(320),
    media: 0xfe,
    fat_size_16: [1,0],
    sec_per_trk: [8,0],
    num_heads: [1,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const SSDD_525_9: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 1,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x40),
    tot_sec_16: u16::to_le_bytes(360),
    media: 0xfc,
    fat_size_16: [1,0],
    sec_per_trk: [9,0],
    num_heads: [1,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const DSDD_525_8: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 2,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x70),
    tot_sec_16: u16::to_le_bytes(640),
    media: 0xff,
    fat_size_16: [1,0],
    sec_per_trk: [8,0],
    num_heads: [1,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const DSDD_525_9: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 2,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x70),
    tot_sec_16: u16::to_le_bytes(720),
    media: 0xfd,
    fat_size_16: [2,0],
    sec_per_trk: [9,0],
    num_heads: [1,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const DSQD_525: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 2,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x70),
    tot_sec_16: u16::to_le_bytes(1280),
    media: 0xfb,
    fat_size_16: [2,0],
    sec_per_trk: [8,0],
    num_heads: [2,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const DSHD_525: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 1,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0xe0),
    tot_sec_16: u16::to_le_bytes(2400),
    media: 0xf9,
    fat_size_16: [7,0],
    sec_per_trk: [15,0],
    num_heads: [2,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const D35_720: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 2,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0x70),
    tot_sec_16: u16::to_le_bytes(1440),
    media: 0xf9,
    fat_size_16: [3,0],
    sec_per_trk: [9,0],
    num_heads: [2,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const D35_1440: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 1,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0xe0),
    tot_sec_16: u16::to_le_bytes(2880),
    media: 0xf0,
    fat_size_16: [9,0],
    sec_per_trk: [18,0],
    num_heads: [2,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};

const D35_2880: BPBFoundation = BPBFoundation {
    bytes_per_sec: [0,2],
    sec_per_clus: 2,
    reserved_sectors: [1,0],
    num_fats: 2,
    root_ent_cnt: u16::to_le_bytes(0xf0),
    tot_sec_16: u16::to_le_bytes(5760),
    media: 0xf0,
    fat_size_16: [9,0],
    sec_per_trk: [36,0],
    num_heads: [2,0],
    hidd_sec: [0,0,0,0],
    tot_sec_32: [0,0,0,0]
};