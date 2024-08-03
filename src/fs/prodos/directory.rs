
//! ### ProDOS directory structures
//! 
//! These are all implemented as fixed length structs with private fields.
//! External interactions are largely trhough the `Directory` trait object,
//! and the `Entry` struct.  The internals involve a somewhat complex
//! arrangement of traits and generics.


use chrono::{Datelike,Timelike};
use std::fmt;
use log::{warn,error};
use num_traits::FromPrimitive;
use std::collections::HashMap;
use regex::Regex;
use colored::*;
use super::types::*;
use super::super::FileImage;
use crate::DYNERR;

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;

fn pack_time(time: Option<chrono::NaiveDateTime>) -> [u8;4] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let (_is_common_era,year) = now.year_ce();
    let packed_date = (now.day() + (now.month() << 5) + (year%100 << 9)) as u16;
    let packed_time = (now.minute() + (now.hour() << 8)) as u16;
    let bytes_date = u16::to_le_bytes(packed_date);
    let bytes_time = u16::to_le_bytes(packed_time);
    return [bytes_date[0],bytes_date[1],bytes_time[0],bytes_time[1]];
}

fn unpack_time(prodos_date_time: [u8;4]) -> Option<chrono::NaiveDateTime> {
    let date = u16::from_le_bytes([prodos_date_time[0],prodos_date_time[1]]);
    let time = u16::from_le_bytes([prodos_date_time[2],prodos_date_time[3]]);
    let yearmod100 = date >> 9;
    // Suppose the earliest date stamp we can find originates from the year before
    // SOS was released, i.e., 1979.  Use this to help decide the century.
    // This scheme will work until 2079.
    let year = match yearmod100 < 79 {
        true => 2000 + yearmod100,
        false => 1900 + yearmod100
    };
    let month = (date >> 5) & 15;
    let day = date & 31;
    let hour = (time >> 8) & 255;
    let minute = time & 255;
    match chrono::NaiveDate::from_ymd_opt(year as i32,month as u32,day as u32) {
        Some(date) => date.and_hms_opt(hour as u32,minute as u32,0),
        None => None
    }
}

/// Test the string for validity as a ProDOS name.
/// This can be used to check names before passing to functions that may panic.
pub fn is_name_valid(s: &str) -> bool {
    let fname_patt = Regex::new(r"^[A-Z][A-Z0-9.]{0,14}$").unwrap();
    if !fname_patt.is_match(&s.to_uppercase()) {
        return false;
    } else {
        return true;
    }
}

/// Convert filename bytes to a string.  Will not panic, will escape the string if necessary.
/// Must pass the stor_len_nibs field into nibs.
fn file_name_to_string(nibs: u8, fname: [u8;15]) -> String {
    let name_len = nibs & 0x0f;
    if let Ok(result) = String::from_utf8(fname[0..name_len as usize].to_vec()) {
        return result;
    }
    warn!("continuing with invalid filename");
    crate::escaped_ascii_from_bytes(&fname[0..name_len as usize].to_vec(), true, false)
}

/// Convert storage type and String to (stor_len_nibs,fname).
/// Panics if the string is not a valid ProDOS name.
fn string_to_file_name(stype: &StorageType, s: &str) -> (u8,[u8;15]) {
    if !is_name_valid(s) {
        panic!("attempt to create a bad file name {}",s);
    }
    let new_nibs = ((*stype as u8) << 4) + s.len() as u8;
    let mut ans: [u8;15] = [0;15];
    let mut i = 0;
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    (new_nibs,ans)
}

/// Test a generic trait object with a name against the given string.
/// If the string is not a valid ProDOS name this will panic.
pub fn is_file_match<T: HasName>(valid_types: &Vec<StorageType>,name: &String,obj: &T) -> bool {
    let (nibs_disk,fname_disk) = obj.fname();
    for typ in valid_types {
        let (nibs,fname) = string_to_file_name(typ, name);
        let l = (nibs & 0x0f) as usize;
        if nibs==nibs_disk && fname[0..l]==fname_disk[0..l] {
            return true;
        }
    }
    return false;
}

// Block   | Contents
// -----------------------------
// 0       | Loader
// 1       | Loader
// 2       | Volume Directory Key
// 3 - n   | Volume Directory
// n+1 - p | Volume Bitmap

pub trait Header {
    fn file_count(&self) -> u16;
    fn inc_file_count(&mut self);
    fn dec_file_count(&mut self);
    fn set_access(&mut self,what: Access,which: bool);
    fn set_all_access(&mut self,what: u8);
    fn standardize(&mut self,offset: usize) -> Vec<usize>;
}

pub trait HasEntries {
    fn name(&self) -> String;
    fn file_count(&self) -> u16;
    fn entry_locations(&self,iblock: u16) -> Vec<EntryLocation>;
    fn prev(&self) -> u16;
    fn next(&self) -> u16;
    fn set_links(&mut self,prev: Option<u16>,next: Option<u16>);
    fn get_entry(&self,loc: &EntryLocation) -> Entry;
    fn set_entry(&mut self,loc: &EntryLocation,entry: Entry);
    fn delete_entry(&mut self,loc: &EntryLocation);
}

pub trait HasName {
    fn fname(&self) -> (u8,[u8;15]);
    fn name(&self) -> String;
    fn storage_type(&self) -> StorageType;
}

pub trait Directory: DiskStruct + HasEntries {
    fn total_blocks(&self) -> Option<usize>;
    fn parent_entry_loc(&self) -> Option<EntryLocation>;
    fn inc_file_count(&mut self);
    fn dec_file_count(&mut self);
    fn standardize(&mut self,offset: usize) -> Vec<usize>;
    fn delete(&mut self);
}

/// KeyBlock has a generic header type, which can be either
/// VolDirHeader or SubDirHeader
#[derive(Clone,Copy)]
pub struct KeyBlock<T> {
    prev_block: [u8;2],
    next_block: [u8;2],
    pub header: T,
    entries: [Entry;12]
}

#[derive(Clone,Copy)]
pub struct EntryBlock {
    prev_block: [u8;2],
    next_block: [u8;2],
    entries: [Entry;13]
}

#[derive(DiskStruct,Clone,Copy)]
pub struct VolDirHeader {
    stor_len_nibs: u8,
    name: [u8;15],
    pub pad1: [u8;8],
    create_time: [u8;4],
    vers: u8,
    min_vers: u8,
    access: u8,
    entry_len: u8,
    entries_per_block: u8,
    file_count: [u8;2],
    pub bitmap_ptr: [u8;2],
    total_blocks: [u8;2]
}

#[derive(DiskStruct,Clone,Copy)]
pub struct SubDirHeader {
    stor_len_nibs: u8,
    name: [u8;15],
    pad1: [u8;8],
    create_time: [u8;4],
    vers: u8,
    min_vers: u8,
    access: u8,
    entry_len: u8,
    entries_per_block: u8,
    file_count: [u8;2],
    parent_ptr: [u8;2],
    parent_entry_num: u8,
    parent_entry_len: u8
}

#[derive(DiskStruct,Clone,Copy)]
pub struct Entry {
    stor_len_nibs: u8,
    name: [u8;15],
    file_type: u8,
    key_ptr: [u8;2],
    blocks_used: [u8;2],
    eof: [u8;3],
    create_time: [u8;4],
    vers: u8,
    min_vers: u8,
    access: u8,
    aux_type: [u8;2],
    last_mod: [u8;4],
    header_ptr: [u8;2]
}

impl VolDirHeader {
    pub fn format(&mut self, blocks: u16, vol_name: &str, create_time: Option<chrono::NaiveDateTime>) {
        let (nibs,fname) = string_to_file_name(&StorageType::VolDirHeader, vol_name);
        self.stor_len_nibs = nibs;
        self.name = fname;
        self.create_time = pack_time(create_time);
        self.vers = 0;
        self.min_vers = 0;
        self.access = STD_ACCESS;
        self.entry_len = 0x27;
        self.entries_per_block = 13;
        self.file_count = [0,0];
        self.bitmap_ptr = [6,0];
        self.total_blocks = u16::to_le_bytes(blocks);
    }
    pub fn total_blocks(&self) -> u16 {
        u16::from_le_bytes(self.total_blocks)
    }
}

impl SubDirHeader {
    /// Panics if `name` is invalid
    pub fn create(&mut self, name: &String, parent_ptr: u16, parent_entry_num: u8, create_time: Option<chrono::NaiveDateTime>) {
        let (nibs,fname) = string_to_file_name(&StorageType::SubDirHeader, name);
        self.stor_len_nibs = nibs;
        self.name = fname;
        self.pad1 = [0x75,0,0,0,0,0,0,0];
        self.create_time = pack_time(create_time);
        self.vers = 0;
        self.min_vers = 0;
        self.access = STD_ACCESS;
        self.entry_len = 0x27;
        self.entries_per_block = 13;
        self.file_count = [0,0];
        self.parent_ptr = u16::to_le_bytes(parent_ptr);
        self.parent_entry_num = parent_entry_num;
        self.parent_entry_len = 0x27;
    }
}

impl Entry {
    pub fn is_active(&self) -> bool {
        return self.stor_len_nibs>0;
    }
    pub fn change_storage_type(&mut self,stype: StorageType) {
        self.stor_len_nibs &= 0x0f;
        self.stor_len_nibs |= (stype as u8) << 4;
    }
    pub fn get_ptr(&self) -> u16 {
        return u16::from_le_bytes(self.key_ptr);
    }
    pub fn set_ptr(&mut self,ptr: u16) {
        self.key_ptr = u16::to_le_bytes(ptr);
    }
    // pub fn get_header(&self) -> u16 {
    //     return u16::from_le_bytes(self.header_ptr);
    // }
    pub fn eof(&self) -> usize {
        return u32::from_le_bytes([self.eof[0],self.eof[1],self.eof[2],0]) as usize;
    }
    pub fn aux(&self) -> u16 {
        return u16::from_le_bytes(self.aux_type);
    }
    pub fn set_aux(&mut self,aux: u16) {
        self.aux_type = u16::to_le_bytes(aux);
    }
    pub fn ftype(&self) -> u8 {
        return self.file_type;
    }
    pub fn set_ftype(&mut self,typ: u8) {
        self.file_type = typ;
    }
    pub fn set_eof(&mut self,bytes: usize) {
        let inc = u32::to_le_bytes(bytes as u32);
        self.eof = [inc[0],inc[1],inc[2]];
    }
    pub fn delta_blocks(&mut self,delta: i32) {
        let new_val = u16::from_le_bytes(self.blocks_used) as i32 + delta;
        self.blocks_used = u16::to_le_bytes(new_val as u16);
    }
    pub fn metadata_to_fimg(&self,fimg: &mut FileImage) {
        fimg.eof = super::super::FileImage::fix_le_vec(self.eof(),3);
        fimg.access = vec![self.access];
        fimg.fs_type = vec![self.file_type];
        fimg.aux = self.aux_type.to_vec();
        fimg.created = self.create_time.to_vec();
        fimg.modified = self.last_mod.to_vec();
        fimg.version = vec![self.vers];
        fimg.min_version = vec![self.min_vers];
    }
    /// Panics if `name` is invalid
    pub fn create_subdir(name: &str,key_ptr: u16,header_ptr: u16,create_time: Option<chrono::NaiveDateTime>) -> Entry {
        let mut ans = Self::new();
        let (nibs,fname) = string_to_file_name(&StorageType::SubDirEntry, name);
        ans.stor_len_nibs = nibs;
        ans.name = fname;
        ans.file_type = FileType::Directory as u8;
        ans.key_ptr = u16::to_le_bytes(key_ptr);
        ans.blocks_used = [0,0];
        ans.eof = [0,0,0];
        ans.create_time = pack_time(create_time);
        ans.vers = 0;
        ans.min_vers = 0;
        ans.access = STD_ACCESS | DIDCHANGE;
        ans.aux_type = u16::to_le_bytes(0);
        ans.last_mod = pack_time(create_time);
        ans.header_ptr = u16::to_le_bytes(header_ptr);
        return ans;
    }
    /// Panics if `name` is invalid
    pub fn create_file(name: &str,fimg: &FileImage,key_ptr: u16,header_ptr: u16,create_time: Option<chrono::NaiveDateTime>) -> Result<Entry,DYNERR> {
        if fimg.fs_type.len()<1 || fimg.version.len()<1 || fimg.min_version.len()<1 || fimg.aux.len()<2 {
            error!("one or more ProDOS file image fields were too short");
            return Err(Box::new(Error::Range));
        }
        let mut ans = Self::new();
        let (nibs,fname) = string_to_file_name(&StorageType::Seedling, name);
        ans.stor_len_nibs = nibs;
        ans.name = fname;
        ans.file_type = fimg.fs_type[0];
        ans.key_ptr = u16::to_le_bytes(key_ptr);
        ans.blocks_used = [0,0];
        ans.eof = [0,0,0];
        ans.create_time = pack_time(create_time);
        ans.vers = fimg.version[0];
        ans.min_vers = fimg.min_version[0];
        ans.access = fimg.access[0];
        ans.aux_type = [fimg.aux[0],fimg.aux[1]];
        ans.last_mod = pack_time(create_time);
        ans.header_ptr = u16::to_le_bytes(header_ptr);
        return Ok(ans);
    }
    pub fn get_access(&self,what: Access) -> bool {
        return self.access & what as u8 > 0;
    }
    pub fn set_access(&mut self,what: Access,which: bool) {
        if which {
            self.access |= what as u8;
        } else {
            self.access &= u8::MAX ^ what as u8;
        }
    }
    // pub fn get_all_access(&self) -> u8 {
    //     self.access
    // }
    pub fn set_all_access(&mut self,what: u8) {
        self.access = what;
    }
    /// Panics if `name` is invalid
    pub fn rename(&mut self,name: &str) {
        let stor = self.storage_type();
        let (nibs,fname) = string_to_file_name(&stor, name);
        self.stor_len_nibs = nibs;
        self.name = fname;
    }
    pub fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // relative to the entry start
        // creation, version, min version, last mod
        let mut ans = vec![0x18,0x19,0x1a,0x1b,0x01c,0x1d,0x21,0x22,0x23,0x24];
        // ignore trailing characters in the name
        let name_start = match self.is_active() {
            true => 1 + file_name_to_string(self.stor_len_nibs,self.name).len(),
            false => 1
        };
        for i in name_start..16 {
            ans.push(i);
        }
        ans.iter().map(|x| x + offset).collect()
    }
    /// put metadata into JSON object, intended for use with TREE
    pub fn meta_to_json(&self) -> json::JsonValue {
        const DATE_FMT: &str = "%Y/%m/%d %H:%M";
        let mut meta = json::JsonValue::new_object();
        let create_time = match unpack_time(self.create_time) {
            Some(date_time) => date_time.format(DATE_FMT).to_string(),
            None => "".to_string()
        };
        let mod_time = match unpack_time(self.last_mod) {
            Some(date_time) => date_time.format(DATE_FMT).to_string(),
            None => "".to_string()
        };
        meta["type"] = json::JsonValue::String(hex::encode_upper(vec![self.file_type]));
        meta["aux"] = json::JsonValue::String(hex::encode_upper(self.aux_type.to_vec()));
        meta["eof"] = json::JsonValue::Number(self.eof().into());
        meta["time_created"] = json::JsonValue::String(create_time);
        meta["time_modified"] = json::JsonValue::String(mod_time);
        meta["read_only"] = json::JsonValue::Boolean(self.access & 0x02 == 0);
        meta["system"] = json::JsonValue::Boolean(self.file_type==FileType::System as u8);
        meta["blocks"] = json::JsonValue::Number(u16::from_le_bytes(self.blocks_used).into());
        meta
    }
    pub fn universal_row(&self) -> String  {
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let type_as_hex = "$".to_string()+ &hex::encode_upper(vec![self.file_type]);
        super::super::universal_row(
            match typ_map.get(&self.file_type) { Some(s) => *s, _ => &type_as_hex },
            u16::from_le_bytes(self.blocks_used) as usize,
            &self.name()
        )
    }
}

/// Allows the entry to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
/// Intended use is for CATALOG.
impl fmt::Display for Entry {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const DATE_FMT: &str = "%d-%b-%y %H:%M";
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let create_time = match unpack_time(self.create_time) {
            Some(date_time) => date_time.format(DATE_FMT).to_string(),
            None => "<NO DATE>".to_string()
        };
        let mod_time = match unpack_time(self.last_mod) {
            Some(date_time) => date_time.format(DATE_FMT).to_string(),
            None => "<NO DATE>".to_string()
        };
        let mut write_protect = "*".to_string();
        if self.access & 0x02 == 0x02 {
            write_protect = " ".to_string();
        }
        //"NAME","TYPE","BLOCKS","MODIFIED","CREATED","ENDFILE","SUBTYPE"
        let type_as_hex = "$".to_string()+ &hex::encode_upper(vec![self.file_type]);
        write!(f,"{}{:15} {:4} {:6} {:16} {:16} {:7} {:7}",
            write_protect,
            match self.file_type { 0x0f => self.name().blue().bold(), _ => self.name().normal() },
            match typ_map.get(&self.file_type) { Some(s) => *s, _ => &type_as_hex },
            u16::from_le_bytes(self.blocks_used),
            mod_time,
            create_time,
            self.eof(),
            u16::from_le_bytes(self.aux_type)
        )
    }
}

impl Header for VolDirHeader {
    fn file_count(&self) -> u16 {
        return u16::from_le_bytes(self.file_count);
    }
    fn inc_file_count(&mut self) {
        self.file_count = u16::to_le_bytes(u16::from_le_bytes(self.file_count)+1);
    }
    fn dec_file_count(&mut self) {
        self.file_count = u16::to_le_bytes(u16::from_le_bytes(self.file_count)-1);
    }
    fn set_access(&mut self,what: Access,which: bool) {
        if which {
            self.access |= what as u8;
        } else {
            self.access &= u8::MAX ^ what as u8;
        }
    }
    fn set_all_access(&mut self,what: u8) {
        self.access = what;
    }
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // these are relative to the block start
        let mut ans: Vec<usize> = Vec::new();
        // padding, creation, version, min version
        for i in 0x14..0x22 {
            ans.push(i);
        }
        // ignore trailing characters in the name
        let start = 1 + file_name_to_string(self.stor_len_nibs,self.name).len();
        for i in start..16 {
            ans.push(i);
        }
        ans.iter().map(|x| x + offset).collect()
}
}

impl Header for SubDirHeader {
    fn file_count(&self) -> u16 {
        return u16::from_le_bytes(self.file_count);
    }
    fn inc_file_count(&mut self) {
        self.file_count = u16::to_le_bytes(u16::from_le_bytes(self.file_count)+1);
    }
    fn dec_file_count(&mut self) {
        self.file_count = u16::to_le_bytes(u16::from_le_bytes(self.file_count)-1);
    }
    fn set_access(&mut self,what: Access,which: bool) {
        if which {
            self.access |= what as u8;
        } else {
            self.access &= u8::MAX ^ what as u8;
        }
    }
    fn set_all_access(&mut self,what: u8) {
        self.access = what;
    }
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // these are relative to the block start
        let mut ans: Vec<usize> = Vec::new();
        // pad1 (except first byte), creation, version, min version 
        for i in 0x15..0x22 {
            ans.push(i);
        }
        // ignore trailing characters in the name
        let start = 1 + file_name_to_string(self.stor_len_nibs,self.name).len();
        for i in start..16 {
            ans.push(i);
        }
        ans.iter().map(|x| x + offset).collect()
}
}

impl<T: Header + HasName + DiskStruct> HasEntries for KeyBlock<T> {
    fn name(&self) -> String {
        return self.header.name();
    }
    fn file_count(&self) -> u16 {
        return self.header.file_count();
    }
    fn prev(&self) -> u16 {
        return u16::from_le_bytes(self.prev_block);
    }
    fn next(&self) -> u16 {
        return u16::from_le_bytes(self.next_block);
    }
    fn entry_locations(&self,iblock: u16) -> Vec<EntryLocation> {
        let mut ans = Vec::<EntryLocation>::new();
        for i in 0..self.entries.len() {
            ans.push(EntryLocation { block: iblock, idx: i+2 });
        }
        return ans;
    }
    fn set_links(&mut self,prev: Option<u16>,next: Option<u16>) {
        self.prev_block = match prev {
            Some(ptr) => u16::to_le_bytes(ptr),
            None => self.prev_block
        };
        self.next_block = match next {
            Some(ptr) => u16::to_le_bytes(ptr),
            None => self.next_block
        };
    }
    fn get_entry(&self,loc: &EntryLocation) -> Entry {
        return Entry::from_bytes(&self.entries[loc.idx-2].to_bytes()).expect("unexpected entry size");
    }
    fn set_entry(&mut self,loc: &EntryLocation,entry: Entry) {
        self.entries[loc.idx-2] = entry;
    }
    fn delete_entry(&mut self,loc: &EntryLocation) {
        self.entries[loc.idx-2].stor_len_nibs = 0;
    }
}

impl HasEntries for EntryBlock {
    fn name(&self) -> String {
        panic!("only the key block has a name");
    }
    fn file_count(&self) -> u16 {
        panic!("only the key block has a file count");
    }
    fn prev(&self) -> u16 {
        return u16::from_le_bytes(self.prev_block);
    }
    fn next(&self) -> u16 {
        return u16::from_le_bytes(self.next_block);
    }
    fn entry_locations(&self,iblock: u16) -> Vec<EntryLocation> {
        let mut ans = Vec::<EntryLocation>::new();
        for i in 0..self.entries.len() {
            ans.push(EntryLocation { block: iblock, idx: i+1 });
        }
        return ans;
    }
    fn set_links(&mut self,prev: Option<u16>,next: Option<u16>) {
        self.prev_block = match prev {
            Some(ptr) => u16::to_le_bytes(ptr),
            None => self.prev_block
        };
        self.next_block = match next {
            Some(ptr) => u16::to_le_bytes(ptr),
            None => self.next_block
        };
    }
    fn get_entry(&self,loc: &EntryLocation) -> Entry {
        return Entry::from_bytes(&self.entries[loc.idx-1].to_bytes()).expect("unexpected entry size");
    }
    fn set_entry(&mut self,loc: &EntryLocation,entry: Entry) {
        self.entries[loc.idx-1] = entry;
    }
    fn delete_entry(&mut self,loc: &EntryLocation) {
        self.entries[loc.idx-1].stor_len_nibs = 0;
    }
}

impl<T: Header + HasName + DiskStruct> DiskStruct for KeyBlock<T> {
    fn new() -> Self where Self: Sized {
        Self {
            prev_block: [0;2],
            next_block: [0;2],
            header: T::new(),
            entries: [
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
            ]
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.prev_block.to_vec());
        ans.append(&mut self.next_block.to_vec());
        ans.append(&mut self.header.to_bytes());
        for i in 0..self.entries.len() {
            ans.append(&mut self.entries[i].to_bytes());
        }
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        self.prev_block = [bytes[0],bytes[1]];
        self.next_block = [bytes[2],bytes[3]];
        let mut offset = 4;
        self.header.update_from_bytes(&bytes[offset..self.header.len()+offset])?;
        offset += self.header.len();
        for i in 0..self.entries.len() {
            self.entries[i].update_from_bytes(&bytes[offset..offset+self.entries[i].len()])?;
            offset += self.entries[i].len();
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
    fn len(&self) -> usize {
        return 511;
    }
}

impl DiskStruct for EntryBlock {
    fn new() -> Self where Self: Sized {
        Self {
            prev_block: [0;2],
            next_block: [0;2],
            entries: [
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
                Entry::new(),
            ]
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.prev_block.to_vec());
        ans.append(&mut self.next_block.to_vec());
        for i in 0..self.entries.len() {
            ans.append(&mut self.entries[i].to_bytes());
        }
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        self.prev_block = [bytes[0],bytes[1]];
        self.next_block = [bytes[2],bytes[3]];
        let mut offset = 4;
        for i in 0..self.entries.len() {
            self.entries[i].update_from_bytes(&bytes[offset..offset+self.entries[i].len()])?;
            offset += self.entries[i].len();
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
    fn len(&self) -> usize {
        return 511;
    }
}

impl HasName for Entry {
    fn fname(&self) -> (u8,[u8;15]) {
        return (self.stor_len_nibs,self.name);
    }
    fn name(&self) -> String {
        return file_name_to_string(self.stor_len_nibs, self.name);
    }
    fn storage_type(&self) -> StorageType {
        match StorageType::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
            Some(t) => t,
            _ => panic!("encountered unknown storage type")
        }
    }
}

impl HasName for VolDirHeader {
    fn fname(&self) -> (u8,[u8;15]) {
        return (self.stor_len_nibs,self.name);
    }
    fn name(&self) -> String {
        return file_name_to_string(self.stor_len_nibs, self.name);
    }
    fn storage_type(&self) -> StorageType {
        match StorageType::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
            Some(t) => t,
            _ => panic!("encountered unknown storage type")
        }
    }
}

impl HasName for SubDirHeader {
    fn fname(&self) -> (u8,[u8;15]) {
        return (self.stor_len_nibs,self.name);
    }
    fn name(&self) -> String {
        return file_name_to_string(self.stor_len_nibs, self.name);
    }
    fn storage_type(&self) -> StorageType {
        match StorageType::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
            Some(t) => t,
            _ => panic!("encountered unknown storage type")
        }
    }
}

impl Directory for KeyBlock<VolDirHeader> {
    fn total_blocks(&self) -> Option<usize> {
        Some(u16::from_le_bytes(self.header.total_blocks) as usize)
    }
    fn parent_entry_loc(&self) -> Option<EntryLocation> {
        None
    }
    fn inc_file_count(&mut self) {
        self.header.inc_file_count()
    }
    fn dec_file_count(&mut self) {
        self.header.dec_file_count()
    }
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        self.header.standardize(offset)
    }
    fn delete(&mut self) {
        panic!("attempt to delete volume directory")
    }
}

impl Directory for KeyBlock<SubDirHeader> {
    fn total_blocks(&self) -> Option<usize> {
        None
    }
    fn parent_entry_loc(&self) -> Option<EntryLocation> {
        return Some(EntryLocation {
            block: u16::from_le_bytes(self.header.parent_ptr),
            idx: self.header.parent_entry_num as usize
        });
    }
    fn inc_file_count(&mut self) {
        self.header.inc_file_count();
    }
    fn dec_file_count(&mut self) {
        self.header.dec_file_count();
    }
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        self.header.standardize(offset)
    }
    fn delete(&mut self) {
        self.header.stor_len_nibs = 0;
    }
}

impl Directory for EntryBlock {
    fn total_blocks(&self) -> Option<usize> {
        None
    }
    fn parent_entry_loc(&self) -> Option<EntryLocation> {
        panic!("attempt to get parent from EntryBlock");
    }
    fn inc_file_count(&mut self) {
        panic!("attempt to access header from EntryBlock");
    }
    fn dec_file_count(&mut self) {
        panic!("attempt to access header from EntryBlock");
    }
    fn standardize(&mut self,_offset: usize) -> Vec<usize> {
        Vec::new()
    }
    fn delete(&mut self) {
        panic!("attempt to delete entry block")
    }
}