//! ### CP/M directory structures
//! 
//! The fundamental structure is a 32-byte entry, which comes in various flavors.
//! The directory is nothing other than a packed sequence of entries.
//! The primary type of entry is the "extent," represented by the `Extent` struct.
//! All information about file locations is contained in the extents, in
//! particular, there is no separate file index or volume bitmap.
//! 
//! The term "extent" has shades of meaning, see the parent module notes. 

use super::types::*;
use crate::bios::dpb::DiskParameterBlock;

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;

use log::{debug,warn,trace};

/// The extent is in general a partial directory entry.  The bigger the
/// file gets the more extents are needed to point to all the blocks.
/// The extent capacity is in the form 16384 * (EXM+1), where EXM = 2^n-1, with n from 0 to 4.
/// The 16K subsets within are called "logical extents."
/// The extents are indexed by counting logical extents.
#[derive(DiskStruct,Copy,Clone,PartialEq)]
pub struct Extent {
    /// value 0-15 identifies this as a file extent.  value 0xe5 means unused or deleted.
    pub user: u8,
    /// positive ASCII; high bits are used as attributes in some specific implementations.
    pub name: [u8;8],
    /// positive ASCII; high bits depend on version as follows:
    /// * v1: unused, unused, unused
    /// * v2: read only, system file, unused
    /// * v3: read only, system file, archived
    pub typ: [u8;3],
    /// bits 0-4 are the low 5-bits of the extent index.
    /// The index counts *logical* extents.  Hence the step between
    /// indices is EXM+1, except possibly for the last step.
    idx_low: u8,
    /// Bytes used in the last record in the extent, 0 means record is full (128 bytes).
    /// This is always 0 until CP/M v3 (eof was only known to within a record boundary)
    last_bytes: u8,
    /// bits 0-5 are the high 6-bits of the extent index.
    /// This is always 0 until CP/M v2.
    idx_high: u8,
    /// Records (128 bytes each) used in the last logical extent.
    last_records: u8,
    /// block pointers, 8-bit for CP/M v1, can be 16-bit in later versions,
    /// depending on disk size and disk parameter block.
    pub block_list: [u8;16]
}

/// Password extents can appear anywhere in the directory.
/// Requires CP/M v3 or higher.
#[derive(DiskStruct)]
pub struct Password {
    pub user: u8, // 16-31 means this is a password extent, value is user number + 16
    pub name: [u8;8], // file protected
    pub typ: [u8;3], // type of file
    mode: u8, // bits: 7-read, 6-write, 5-delete (right to left)
    decoder: u8, // password decode, decode: xor with password in reverse order, encode: sum
    pad1: [u8;2],
    password: [u8;8],
    pad2: [u8;8]
}

/// Disk label extent can appear anywhere in the directory.
/// Requires CP/M v3 or higher.
#[derive(DiskStruct)]
pub struct Label {
    status: u8, // 0x20
    pub name: [u8;8],
    pub typ: [u8;3],
    mode: u8, // bits: 7-password, 6-timestamp on access, 5-timestamp on mod, 4-timestamp on create, 0-label exists, 4&6 exclusive
    decoder: u8, // password decode, decode: xor with password in reverse order, encode: sum
    pad: [u8;2],
    password: [u8;8],
    create_time: [u8;4],
    mod_time: [u8;4]
}

/// Timestamp extent for CP/M 3, if present, follows every third other extent.
/// 4-byte timestamp: from_le_bytes([b0,b1])=days, day 1 = 1-jan-1978, b2=BCD hour, b3=BCD minute.
/// Requires CP/M v3 or higher; other time stamping was provided by third parties earlier.
#[derive(DiskStruct)]
pub struct Timestamp {
    status: u8, // 0x21
    create1: [u8;4],
    mod1: [u8;4],
    pass1: u8,
    pad1: u8,
    create2: [u8;4],
    mod2: [u8;4],
    pass2: u8,
    pad2: u8,
    create3: [u8;4],
    mod3: [u8;4],
    pass3: u8,
    pad3: u8,
    pad4: u8
}

/// Directory is merely a packed sequence of extents.
pub struct Directory {
    entries: Vec<[u8;DIR_ENTRY_SIZE]>
}

impl Extent {
    /// Change only the lowest 7 bits (change name, keep flags)
    pub fn set_name(&mut self,name: [u8;8],typ: [u8;3]) {
        for i in 0..8 {
            self.name[i] = (name[i] & 0x7f) + (self.name[i] & 0x80);
        }
        for i in 0..3 {
            self.typ[i] = (typ[i] & 0x7f) + (self.typ[i] & 0x80);
        }
    }
    /// Change only the high bit (keep name, change flags)
    pub fn set_flags(&mut self,name: [u8;8],typ: [u8;3]) {
        for i in 0..8 {
            self.name[i] = (name[i] & 0x80) + (self.name[i] & 0x7f);
        }
        for i in 0..3 {
            self.typ[i] = (typ[i] & 0x80) + (self.typ[i] & 0x7f);
        }
    }
    /// Get the flags by keeping just the high bit in the name and type
    pub fn get_flags(&self) -> [u8;11] {
        let mut ans: [u8;11] = [0;11];
        for i in 0..8 {
            ans[i] = self.name[i] & 0x80;
        }
        for i in 0..3 {
            ans[i+8] = self.typ[i] & 0x80;
        }
        return ans;
    }
    /// Get the ordered index for this extent of data.
    /// Inner value is the count of logical extents up to and including this extent, minus 1.
    /// If this is the last extent, only *used* logical extents are counted.
    pub fn get_data_ptr(&self) -> Ptr {
        Ptr::ExtentData((self.idx_low as u16 + ((self.idx_high as u16) << 5)) as usize)
    }
    /// Set the ordered index for this extent of data.
    /// Inner value is the count of logical extents up to and including this extent, minus 1.
    /// If this is the last extent, only *used* logical extents are counted.
    pub fn set_data_ptr(&mut self,ptr: Ptr) {
        match ptr {
            Ptr::ExtentData(i) => {
                self.idx_low = (i & 0b11111) as u8;
                self.idx_high = ((i & 0b11111100000) >> 5) as u8;
            },
            _ => panic!("wrong pointer type")
        }
    }
    /// Returns the eof in bytes, *assuming* this is the last extent.
    /// Result may be modulo RECORD_SIZE depending on `self.num_bytes`,
    /// which in turn depends on the CP/M version.
    pub fn get_eof(&self) -> usize {
        let idx = self.get_data_ptr().unwrap();
        let bytes = match self.last_bytes {
            0 => RECORD_SIZE as u8,
            x => x
        };
        // start with full capacity of all logical extents but the last one (idx is count minus 1)
        let mut ans = idx * LOGICAL_EXTENT_SIZE;
        // account for the last logical extent which may be partially filled
        ans += match self.last_records {
            rc if rc<0x80 => (rc-1) as usize * RECORD_SIZE + bytes as usize,
            _ => 0x7f * RECORD_SIZE + bytes as usize
        };
        return ans;
    }
    /// Set the last_bytes and last_records (effectively, this sets the eof)
    /// `x_bytes` is bytes used by *this extent only*.  Must be run for every extent.
    pub fn set_eof(&mut self,x_bytes: usize,vers: [u8;3]) {
        // First get total records and byte remainder, ignoring logical extent boundaries
        let mut total_records = x_bytes/RECORD_SIZE;
        self.last_bytes = (x_bytes%RECORD_SIZE) as u8;
        if self.last_bytes>0 {
            total_records += 1;
        }
        // Now get the number of records only in the last logical extent
        let recs_per_lx = LOGICAL_EXTENT_SIZE / RECORD_SIZE;
        self.last_records = (total_records % recs_per_lx) as u8;
        if self.last_records==0 && total_records>0 {
            self.last_records = recs_per_lx as u8;
        }
        // Finally, if we are CPM 1 or 2, the byte count should be 0
        if vers[0]<3 {
            self.last_bytes = 0;
        }
    }
    /// Set the block pointer `iblock` at the `slot` in the logical extent `lx`.
    /// The `lx` count is reset to 0 for each new extent
    /// The`slot` count is reset to 0 for each new logical extent.
    pub fn set_block_ptr(&mut self,slot: usize,lx: usize,iblock: u16,dpb: &DiskParameterBlock) {
        let lx_per_x = dpb.exm as usize + 1;
        match dpb.ptr_size() {
            1 => self.block_list[lx*16/lx_per_x + slot] = iblock as u8,
            2 => {
                self.block_list[2*(lx*8/lx_per_x + slot)] = u16::to_le_bytes(iblock)[0];
                self.block_list[2*(lx*8/lx_per_x + slot)+1] = u16::to_le_bytes(iblock)[1];
            },
            _ => panic!("invalid block pointer size")
        }
    }
    /// Get block pointers, given the DPB (which implies the pointer size).
    /// The pointers are converted to u16 unconditionally.
    /// CP/M block pointers are always relative to the track offset (also in DPB).
    pub fn get_block_list(&self,dpb: &DiskParameterBlock) -> Vec<u16> {
        match dpb.ptr_size() {
            1 => self.block_list.iter().map(|x| *x as u16).collect::<Vec<u16>>(),
            2 => {
                let mut ans: Vec<u16> = Vec::new();
                for i in 0..8 {
                    ans.push(u16::from_le_bytes([self.block_list[i*2],self.block_list[i*2+1]]));
                }
                ans
            },
            _ => panic!("invalid block pointer size")
        }
    }
}

impl DiskStruct for Directory {
    fn new() -> Self {
        let entries: Vec<[u8;DIR_ENTRY_SIZE]> = Vec::new();
        Self {
            entries
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        for x in &self.entries {
            ans.append(&mut x.to_vec());
        }
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        self.entries = Vec::new();
        let num_extents = bytes.len()/DIR_ENTRY_SIZE;
        if bytes.len()%DIR_ENTRY_SIZE!=0 {
            warn!("directory buffer wrong size");
        }
        for i in 0..num_extents {
            self.entries.push(bytes[i*DIR_ENTRY_SIZE..(i+1)*DIR_ENTRY_SIZE].try_into().expect("bad slice length"));
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
    fn len(&self) -> usize {
        return DIR_ENTRY_SIZE*(self.entries.len());
    }
}

impl Directory {
    /// number of entries (used or not) in the directory
    pub fn num_entries(&self) -> usize {
        self.entries.len()
    }
    pub fn get_type(&self,ptr: &Ptr) -> ExtentType {
        let (idx,xstat) = match ptr {
            Ptr::ExtentEntry(i) => (*i,self.entries[*i][0]),
            _ => panic!("wrong pointer type")
        };
        trace!("entry {} has extent type {}",idx,xstat);
        match xstat {
            x if x<USER_END => ExtentType::File,
            x if x==DELETED => ExtentType::Deleted,
            x if x<USER_END*2 => ExtentType::Password,
            0x20 => ExtentType::Label,
            0x21 => ExtentType::Timestamp,
            x => {
                debug!("unknown extent type {}",x);
                ExtentType::Unknown
            }
        }
    }
    pub fn get_file(&self,ptr: &Ptr) -> Option<Extent> {
        match ptr {
            Ptr::ExtentEntry(idx) => match self.entries[*idx][0] {
                x if x<USER_END => Some(Extent::from_bytes(&self.entries[*idx].to_vec())),
                _ => None
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn set_file(&mut self,ptr: &Ptr,fx: &Extent) {
        match ptr {
            Ptr::ExtentEntry(idx) => {
                self.entries[*idx] = fx.to_bytes().try_into().expect("unexpected size")
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn get_password(&self,ptr: &Ptr) -> Option<Password> {
        match ptr {
            Ptr::ExtentEntry(idx) => match self.entries[*idx][0] {
                x if x>=USER_END && x<USER_END*2 => Some(Password::from_bytes(&self.entries[*idx].to_vec())),
                _ => None
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn get_label(&self,ptr: &Ptr) -> Option<Label> {
        match ptr {
            Ptr::ExtentEntry(idx) => match self.entries[*idx][0] {
                0x20 => Some(Label::from_bytes(&self.entries[*idx].to_vec())),
                _ => None
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn get_timestamp(&self,ptr: &Ptr) -> Option<Timestamp> {
        match ptr {
            Ptr::ExtentEntry(idx) => match self.entries[*idx][0] {
                0x21 => Some(Timestamp::from_bytes(&self.entries[*idx].to_vec())),
                _ => None
            },
            _ => panic!("wrong pointer type")
        }
    }
}