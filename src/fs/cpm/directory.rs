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
use super::pack::*;
use crate::bios::dpb::DiskParameterBlock;
use std::collections::BTreeMap;
use log::{error,debug,warn,trace};
use crate::{STDRESULT,DYNERR};

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;

const RCH: &str = "unreachable was reached";
const LABEL_EXISTS: u8 = 0x01;
const CREATE: u8 = 0x10;
const UPDATE: u8 = 0x20;
const ACCESS: u8 = 0x40;
const PROTECT_READ: u8 = 0x80;
const PROTECT_WRITE: u8 = 0x40;
const PROTECT_DELETE: u8 = 0x20;
const FILES_PROTECTED: u8 = 0x80;

/// Convenient collection of information about a file.
/// Flags are broken out into their own variables.
/// This is the value of the map produced by Directory::build_files.
#[derive(Clone)]
pub struct FileInfo {
    pub user: u8,
    pub name: String,
    pub typ: String,
    pub read_only: bool,
    pub system: bool,
    pub archived: bool,
    pub f1: bool,
    pub f2: bool,
    pub f3: bool,
    pub f4: bool,
    pub encrypted_password: [u8;8],
    pub read_pass: bool,
    pub write_pass: bool,
    pub del_pass: bool,
    pub decoder: u8,
    pub update_time: Option<[u8;4]>,
    pub create_time: Option<[u8;4]>,
    pub access_time: Option<[u8;4]>,
    pub blocks_allocated: usize,
    /// ordered map of data pointers to entry pointers, the data pointer
    /// is the last logical extent within an entry.
    pub entries: BTreeMap<Ptr,Ptr>
}

pub trait DirectoryEntry {
    fn stat_range() -> [u8;2];
}

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
    name: [u8;8],
    /// positive ASCII; high bits depend on version as follows:
    /// * v1: unused, unused, unused
    /// * v2: read only, system file, unused
    /// * v3: read only, system file, archived
    typ: [u8;3],
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
    /// Controls which operations are locked.
    /// 0x80 = read, 0x40 = write, 0x20 = delete
    mode: u8,
    /// The decoder is the sum of the bytes in the unencrypted password.
    /// The encrypted password is xor(decoder,byte) in reverse order.
    decoder: u8,
    pad1: [u8;2],
    password: [u8;8],
    pad2: [u8;8]
}

/// Disk label extent can appear anywhere in the directory.
/// Requires CP/M v3 or higher.
#[derive(DiskStruct)]
pub struct Label {
    status: u8, // 0x20
    name: [u8;8],
    typ: [u8;3],
    /// mode controls passwords and timestamps.
    /// 0x80 = enable passwords on disk, 0x01 = "label exists" (always set?)
    /// 0x40 = timestamp access, 0x20 = timestamp updates, 0x10 = timestamp creation
    /// Access and creation timestamping are mutually exclusive.
    mode: u8,
    /// The decoder for the label's password.
    /// Decoder is the sum of the bytes in the unencrypted password.
    /// The encrypted password is xor(decoder,byte) in reverse order.
    decoder: u8,
    pad: [u8;2],
    password: [u8;8],
    /// this will be duplicated in timestamp extent if it exists
    create_time: [u8;4],
    /// this will be duplicated in timestamp extent if it exists
    update_time: [u8;4]
}

/// Timestamp extent for CP/M 3, if present, follows every third other extent.
/// This contains 2 timestamps and 1 password mode per file, for up to 3 files.
/// The timestamp is meaningful only if it follows the first extent (logical extents 0-EXM) of the corresponding file.
/// 4-byte timestamp: from_le_bytes([b0,b1])=days, day 1 = 1-jan-1978, b2=BCD hour, b3=BCD minute.
/// Requires CP/M v3 or higher; other time stamping was provided by third parties earlier.
#[derive(DiskStruct)]
pub struct Timestamp {
    status: u8, // 0x21
    create_access1: [u8;4],
    update1: [u8;4],
    pass1: u8, // redundant password mode
    pad1: u8,
    create_access2: [u8;4],
    update2: [u8;4],
    pass2: u8,
    pad2: u8,
    create_access3: [u8;4],
    update3: [u8;4],
    pass3: u8,
    pad3: u8,
    pad4: u8
}

/// Directory is merely a packed sequence of entries.
pub struct Directory {
    entries: Vec<[u8;DIR_ENTRY_SIZE]>
}

impl Label {
    pub fn create() -> Self {
        let mut ans = Label::new();
        ans.status = LABEL;
        ans.mode |= LABEL_EXISTS;
        ans.name = b"LABEL   ".clone();
        ans.typ = b"   ".clone();
        ans
    }
    pub fn set(&mut self,name: [u8;8],typ: [u8;3]) {
        self.name = name;
        self.typ = typ;
    }
    /// This is the label's own timestamp, not a timestamp entry.
    /// If argument is None, the existing timestamp is not changed.
    pub fn set_timestamp_for_label(&mut self,create: Option<chrono::NaiveDateTime>,update: Option<chrono::NaiveDateTime>) {
        if create.is_some() {
            self.create_time = pack_date(create);
        }
        if update.is_some() {
            self.update_time = pack_date(update);
        }
    }
    pub fn get_split_string(&self) -> (String,String) {
        file_name_to_split_string(self.name, self.typ)
    }
    pub fn get_create_time(&self) -> [u8;4] {
        self.create_time
    }
    pub fn get_update_time(&self) -> [u8;4] {
        self.update_time
    }
    pub fn protect(&mut self,yes: bool) {
        if yes {
            self.mode |= FILES_PROTECTED;
        } else {
            self.mode &= FILES_PROTECTED ^ u8::MAX;
        }
    }
    pub fn timestamp_access(&mut self,yes: bool) {
        if yes {
            self.mode |= ACCESS;
            self.mode &= CREATE ^ u8::MAX;
        } else {
            self.mode &= ACCESS ^ u8::MAX;
        }
    }
    pub fn timestamp_update(&mut self,yes: bool) {
        if yes {
            self.mode |= UPDATE;
        } else {
            self.mode &= UPDATE ^ u8::MAX;
        }
    }
    pub fn timestamp_creation(&mut self,yes: bool) {
        if yes {
            self.mode |= CREATE;
            self.mode &= ACCESS ^ u8::MAX;
        } else {
            self.mode &= CREATE ^ u8::MAX;
        }
    }
    pub fn is_protected(&self) -> bool {
        self.mode & FILES_PROTECTED > 0
    }
    pub fn is_timestamped(&self ) -> bool {
        self.is_timestamped_access() || self.is_timestamped_creation() || self.is_timestamped_update()
    }
    pub fn is_timestamped_access(&self) -> bool {
        self.mode & ACCESS > 0
    }
    pub fn is_timestamped_update(&self) -> bool {
        self.mode & UPDATE > 0
    }
    pub fn is_timestamped_creation(&self) -> bool {
        self.mode & CREATE > 0
    }
}

impl DirectoryEntry for Label {
    fn stat_range() -> [u8;2] {
        [0x20,0x21]
    }
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
    /// Get the 11 bytes containing the name and type, keeping the flags
    pub fn get_name_and_flags(&self) -> [u8;11] {
        let mut ans: [u8;11] = [0;11];
        for i in 0..8 {
            ans[i] = self.name[i];
        }
        for i in 0..3 {
            ans[i+8] = self.typ[i];
        }
        return ans;
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
    pub fn get_string(&self) -> String {
        file_name_to_string(self.name, self.typ)
    }
    pub fn get_string_escaped(&self) -> String {
        file_name_to_string_escaped(self.name, self.typ)
    }
    pub fn get_split_string(&self) -> (String,String) {
        file_name_to_split_string(self.name, self.typ)
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
    /// Result may be modulo RECORD_SIZE depending on `self.last_bytes`,
    /// which in turn depends on the CP/M version.
    pub fn get_eof(&self) -> usize {
        let logical_ext_idx = self.get_data_ptr().unwrap();
        let rec_idx = match self.last_records {
            0 => return logical_ext_idx*LOGICAL_EXTENT_SIZE,
            rc if rc < 0x80 => rc as usize - 1,
            _ => 0x7f
        };
        let bytes = match self.last_bytes {
            0 => RECORD_SIZE as usize,
            x => x as usize
        };
        logical_ext_idx*LOGICAL_EXTENT_SIZE + rec_idx*RECORD_SIZE + bytes
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

impl DirectoryEntry for Extent {
    fn stat_range() -> [u8;2] {
        [0,USER_END]
    }
}

impl Password {
    pub fn create(password: &str,user: u8,name_string: &str,read: bool,write: bool,delete: bool) -> Self {
        let (name,typ) = string_to_file_name(&name_string);
        let (decoder,encrypted) = string_to_password(password);
        Self {
            user: user + 16,
            name,
            typ,
            mode: (read as u8 * PROTECT_READ) | (write as u8 * PROTECT_WRITE) | (delete as u8 * PROTECT_DELETE),
            decoder,
            pad1: [0,0],
            password: encrypted,
            pad2: [0;8]
        }
    }
    pub fn get_string(&self) -> String {
        file_name_to_string(self.name, self.typ)
    }
}

impl DirectoryEntry for Password {
    fn stat_range() -> [u8;2] {
        [USER_END,2*USER_END-1]
    }
}

impl Timestamp {
    fn create() -> Self {
        let mut bytes = vec![0;32];
        bytes[0] = TIMESTAMP;
        Self::from_bytes(&bytes).expect(RCH)
    }
    /// Given the ptr to the entry containing logical extent 0 of a file, get the time stamps and save in the FileInfo struct.
    fn get(dir: &Directory,lab: &Label,lx0: &Ptr,info: &mut FileInfo) -> STDRESULT {
        if !lab.is_timestamped() {
            return Ok(());
        }
        let expected_idx = 4*(1+lx0.unwrap()/4) - 1;
        let sub_idx = lx0.unwrap()%4 + 1;
        if let Some(ts) = dir.get_entry::<Timestamp>(&Ptr::ExtentEntry(expected_idx)) {
            let (update,create_access) = match sub_idx {
                1 => (ts.update1,ts.create_access1),
                2 => (ts.update2,ts.create_access2),
                3 => (ts.update3,ts.create_access3),
                _ => {
                    return Err(Box::new(Error::BadFormat)) 
                }
            };
            if lab.is_timestamped_update() {
                info.update_time = Some(update);
            } else {
                info.update_time = None;
            }
            if lab.is_timestamped_creation() {
                info.create_time = Some(create_access);
            } else {
                info.create_time = None;
            }
            if lab.is_timestamped_access() {
                info.access_time = Some(create_access);
            } else {
                info.access_time = None;
            }
        } else {
            error!("timestamp entry not in expected slot");
            return Err(Box::new(Error::BadFormat));
        }
        Ok(())
    }
    /// Given the ptr to the entry containing logical extent 0 of a file, set a timestamp given by
    /// flags (same flags as label mode), but only if the label mode is set correspondingly.
    /// Only one bit in `flags` should be set.
    fn maybe_set(dir: &mut Directory,lab: &Label,lx0: &Ptr,time: Option<chrono::NaiveDateTime>,flags: u8) -> STDRESULT {
        if (flags == CREATE) && !lab.is_timestamped_creation() && !lab.is_timestamped_access() {
            return Ok(());
        } 
        if (flags == UPDATE) && !lab.is_timestamped_update() {
            return Ok(());
        }
        if (flags == ACCESS) && !lab.is_timestamped_access() {
            return Ok(());
        } 
        let expected_idx = 4*(1+lx0.unwrap()/4) - 1;
        let sub_idx = lx0.unwrap()%4 + 1;
        if let Some(mut ts) = dir.get_entry::<Timestamp>(&Ptr::ExtentEntry(expected_idx)) {
            match (sub_idx,flags) {
                (1,CREATE) | (1,ACCESS) => ts.create_access1 = pack_date(time),
                (2,CREATE) | (2,ACCESS) => ts.create_access2 = pack_date(time),
                (3,CREATE) | (3,ACCESS) => ts.create_access3 = pack_date(time),
                (1,UPDATE) => ts.update1 = pack_date(time),
                (2,UPDATE) => ts.update2 = pack_date(time),
                (3,UPDATE) => ts.update3 = pack_date(time),
                _ => {
                    return Err(Box::new(Error::BadFormat)) 
                }
            };
            dir.set_entry(&Ptr::ExtentEntry(expected_idx), &ts);
        } else {
            error!("timestamp entry not in expected slot");
            return Err(Box::new(Error::BadFormat));
        }
        Ok(())
    }
    /// Given the ptr to the entry containing logical extent 0 of a file, set the create/access 
    /// timestamp, if either create or access timestamping is on.
    pub fn maybe_set_create(dir: &mut Directory,lab: &Label,lx0: &Ptr,time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        Self::maybe_set(dir,lab,lx0,time,CREATE)?;
        Self::maybe_set(dir,lab,lx0,time,UPDATE)
    }
    /// Given the ptr to the entry containing logical extent 0 of a file, set the create/access 
    /// timestamp, if access timestamping is on.
    pub fn maybe_set_access(dir: &mut Directory,lab: &Label,lx0: &Ptr,time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        Self::maybe_set(dir,lab,lx0,time,ACCESS)
    }
    /// Given the ptr to the entry containing logical extent 0 of a file, set the update 
    /// timestamp, if update timestamping is on.
    pub fn maybe_set_update(dir: &mut Directory,lab: &Label,lx0: &Ptr,time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        Self::maybe_set(dir,lab,lx0,time,UPDATE)
    }
}

impl DirectoryEntry for Timestamp {
    fn stat_range() -> [u8;2] {
        [0x21,0x22]
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
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        self.entries = Vec::new();
        let num_extents = bytes.len()/DIR_ENTRY_SIZE;
        if bytes.len()%DIR_ENTRY_SIZE!=0 {
            warn!("directory buffer wrong size");
        }
        for i in 0..num_extents {
            match bytes[i*DIR_ENTRY_SIZE..(i+1)*DIR_ENTRY_SIZE].try_into() {
                Ok(x) => self.entries.push(x),
                Err(_) => return Err(DiskStructError::OutOfData)
            }
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
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
            x if x<USER_END*2 => ExtentType::Password,
            LABEL => ExtentType::Label,
            TIMESTAMP => ExtentType::Timestamp,
            DELETED => ExtentType::Deleted,
            x => {
                debug!("unknown extent type {}",x);
                ExtentType::Unknown
            }
        }
    }
    pub fn get_raw_entry(&self,ptr: &Ptr) -> [u8;DIR_ENTRY_SIZE] {
        match ptr {
            Ptr::ExtentEntry(i) => self.entries[*i].clone(),
            _ => panic!("wrong pointer type")
        }
    }
    // pub fn set_raw_entry(&mut self,ptr: &Ptr,bytes: [u8;DIR_ENTRY_SIZE]) {
    //     match ptr {
    //         Ptr::ExtentEntry(i) => self.entries[*i] = bytes,
    //         _ => panic!("wrong pointer type")
    //     }
    // }
    pub fn get_entry<EntryType: DiskStruct + DirectoryEntry>(&self,ptr: &Ptr) -> Option<EntryType> {
        let rng = EntryType::stat_range();
        match ptr {
            Ptr::ExtentEntry(idx) => match self.entries[*idx][0] {
                x if x>=rng[0] && x<rng[1] => Some(
                    EntryType::from_bytes(&self.entries[*idx]).expect(RCH)
                ),
                _ => None
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn set_entry<EntryType: DiskStruct>(&mut self,ptr: &Ptr,x: &EntryType) {
        match ptr {
            Ptr::ExtentEntry(idx) => {
                self.entries[*idx] = x.to_bytes().try_into().expect("unexpected size")
            },
            _ => panic!("wrong pointer type")
        }
    }
    pub fn find_label(&self) -> Option<Label> {
        for i in 0..self.num_entries() {
            if let Some(label) = self.get_entry::<Label>(&Ptr::ExtentEntry(i)) {
                return Some(label);
            }
        }
        None
    }
    /// Create a new directory with timestamps, this directory is untouched.
    /// If there aren't enough free entries return error.
    pub fn add_timestamps(&self) -> Result<Directory,DYNERR> {
        let mut ans = Directory::new();
        let mut timestamp = Timestamp::create();
        let empty_entry: [u8;32] = [vec![DELETED],vec![0;31]].concat().try_into().expect(RCH);
        for i in 0..self.num_entries() {
            if ans.entries.len()%4 == 3 {
                ans.entries.push(timestamp.to_bytes().try_into().expect(RCH));
                timestamp = Timestamp::create();
            }
            if self.entries[i][0]==TIMESTAMP {
                error!("directory already has timestamps");
                return Err(Box::new(Error::BadFormat));
            }
            if self.entries[i][0]==LABEL {
                let lab = self.get_entry::<Label>(&Ptr::ExtentEntry(i)).unwrap();
                match ans.entries.len()%4 + 1 {
                    1 => {
                        timestamp.create_access1 = lab.create_time;
                        timestamp.update1 = lab.update_time;
                    },
                    2 => {
                        timestamp.create_access2 = lab.create_time;
                        timestamp.update2 = lab.update_time;
                    },
                    3 => {
                        timestamp.create_access3 = lab.create_time;
                        timestamp.update3 = lab.update_time;
                    },
                    _ => {
                        error!("unexpected non-timestamp");
                        return Err(Box::new(Error::BadFormat));
                    }
                };
            }
            if self.entries[i][0]!=DELETED {
                let bytes = self.get_raw_entry(&Ptr::ExtentEntry(i));
                ans.entries.push(bytes);
            }
        }
        if ans.num_entries() > self.num_entries() {
            return Err(Box::new(Error::DirectoryFull));
        }
        for _i in ans.num_entries()..self.num_entries() {
            if ans.entries.len()%4 == 3 {
                ans.entries.push(timestamp.to_bytes().try_into().expect(RCH));
                timestamp = Timestamp::create();
            } else {
                ans.entries.push(empty_entry);
            }
        }
        Ok(ans)
    }
    /// Remove time stamping from this directory.
    pub fn remove_timestamps(&mut self) {
        for i in 0..self.num_entries() {
            if let Some(mut label) = self.get_entry::<Label>(&Ptr::ExtentEntry(i)) {
                label.timestamp_access(false);
                label.timestamp_creation(false);
                label.timestamp_update(false);
                self.set_entry::<Label>(&Ptr::ExtentEntry(i),&label);
            }
            if let Some(mut timestamp) = self.get_entry::<Timestamp>(&Ptr::ExtentEntry(i)) {
                timestamp.status = DELETED;
                self.set_entry::<Timestamp>(&Ptr::ExtentEntry(i),&timestamp);
            }
        }
    }
    /// Build an alphabetized map of user prefixed file names to file info.
    /// This is designed to work whether the disk is CP/M 1, 2, or 3.
    /// The `cpm_vers` sets the maximum version that is accepted.
    pub fn build_files(&self,dpb: &DiskParameterBlock,cpm_vers: [u8;3]) -> Result<BTreeMap<String,FileInfo>,DYNERR> {
        let mut bad_names = 0;
        let mut ans = BTreeMap::new();
        let maybe_lab = self.find_label();
        // first pass collects everything except passwords
        for i in 0..self.num_entries() {
            let xtype = self.get_type(&Ptr::ExtentEntry(i));
            if xtype==ExtentType::Unknown {
                debug!("unknown extent type in entry {}",i);
                return Err(Box::new(Error::BadFormat));
            }
            if cpm_vers[0]<3 && (xtype==ExtentType::Label || xtype==ExtentType::Timestamp || xtype==ExtentType::Password) {
                debug!("rejecting CP/M v3 entry type at {}",i);
                return Err(Box::new(Error::BadFormat));
            }

            if let Some(fx) = self.get_entry::<Extent>(&Ptr::ExtentEntry(i)) {
    
                let key = fx.user.to_string() + ":" + &fx.get_string();
                let (name,typ) = fx.get_split_string();
                let flags = fx.get_flags();
                if flags[4]>0x7f || flags[5]>0x7f || flags[6]>0x7f || flags[7]>0x7f {
                    debug!("unexpected high bits in file name");
                    return Err(Box::new(Error::BadFormat));
                }
                if !is_name_valid(&fx.get_string()) {
                    bad_names += 1;
                }
                if bad_names > 2 {
                    debug!("after {} bad file names rejecting disk",bad_names);
                    return Err(Box::new(Error::BadFormat));
                }
                trace!("found file {}:{}",fx.user,fx.get_string_escaped());

                let finfo = match ans.get_mut(&key) {
                    Some(f) => f,
                    None => {
                        let v = FileInfo {
                            user: fx.user,
                            name,
                            typ,
                            read_only: flags[8] > 0,
                            system: flags[9] > 0,
                            archived: flags[10] > 0,
                            f1: flags[0] > 0,
                            f2: flags[1] > 0,
                            f3: flags[2] > 0,
                            f4: flags[3] > 0,
                            encrypted_password: [0;8],
                            read_pass: false,
                            write_pass: false,
                            del_pass: false,
                            decoder: 0,
                            update_time: None,
                            create_time: None,
                            access_time: None,
                            blocks_allocated: 0,
                            entries: BTreeMap::new()
                        };
                        ans.insert(key.clone(),v);
                        ans.get_mut(&key).unwrap()
                    }
                };
                finfo.entries.insert(fx.get_data_ptr(),Ptr::ExtentEntry(i));
                for b in fx.get_block_list(dpb) {
                    finfo.blocks_allocated += match b>0 { true => 1, false => 0};
                }
                if fx.get_data_ptr() <= Ptr::ExtentData(dpb.exm as usize) {
                    if let Some(lab) = &maybe_lab {
                        if lab.is_timestamped() {
                            Timestamp::get(&self,lab, &Ptr::ExtentEntry(i), finfo)?;
                        }
                    }
                }
            }
        }
        // second pass collects passwords
        for i in 0..self.num_entries() {
            if let Some(px) = self.get_entry::<Password>(&Ptr::ExtentEntry(i)) {
                let key = (px.user-16).to_string() + ":" + &px.get_string();
                match ans.get_mut(&key) {
                    Some(finfo) => {
                        finfo.read_pass = px.mode & 0b10000000 > 0;
                        finfo.write_pass = px.mode & 0b01000000 > 0;
                        finfo.del_pass = px.mode & 0b00100000 > 0;
                        finfo.decoder = px.decoder;
                        finfo.encrypted_password = px.password;
                    },
                    None => {
                        warn!("detached password for `{}`",key);
                    }
                }
            }
        }
        Ok(ans)
    }
    /// Collect users, this assumes we have established a valid CP/M directory
    pub fn get_users(&self) -> Vec<u8> {
        let mut ans = Vec::new();
        for i in 0..self.num_entries() {
            if let Some(fx) = self.get_entry::<Extent>(&Ptr::ExtentEntry(i)) {
                if !ans.contains(&fx.user) {
                    ans.push(fx.user);
                }
            }
        }
        ans.sort();
        ans
    }
    /// Sort the files based on the order of appearance in the directory.
    /// Panics if there is a file with an empty entry list.
    pub fn sort_on_entry_index(&self,files: &BTreeMap<String,FileInfo>) -> BTreeMap<usize,FileInfo> {
        let mut ans = BTreeMap::new();
        for f in files.values() {
            let min_idx = f.entries.values().min().unwrap().unwrap();
            ans.insert(min_idx,f.clone());
        }
        ans
    }
 }

/// Search for a file in the map produced by `Directory::build_files`.
/// This will try a few variations on `xname`, and should usually be
/// preferred over directly accessing the map.
pub fn get_file<'a>(xname: &str,files: &'a BTreeMap<String,FileInfo>) -> Option<&'a FileInfo> {
    // the order of these attempts is significant
    let mut trimmed = xname.trim_end().to_string();
    if !xname.contains(".") {
        trimmed += ".";
    }
    if let Some(finfo) = files.get(&trimmed) {
        return Some(finfo);
    }
    if let Some(finfo) = files.get(&trimmed.to_uppercase()) {
        return Some(finfo);
    }
    if trimmed.contains(":") {
        return None;
    }
    if let Some(finfo) = files.get(&("0:".to_string()+&trimmed)) {
        return Some(finfo);
    }
    if let Some(finfo) = files.get(&("0:".to_string()+&trimmed.to_uppercase())) {
        return Some(finfo);
    }
    return None;
}
