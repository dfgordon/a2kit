
use chrono::{Datelike,Timelike};
use std::fmt;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::collections::HashMap;
use regex::Regex;
use colored::*;
use super::types::*;

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;

fn pack_time(time: Option<chrono::NaiveDateTime>) -> [u8;4] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let (_is_common_era,year) = now.year_ce();
    let packed_date = (now.day() + (now.month() << 5) + ((year-2000) << 9)) as u16;
    let packed_time = (now.minute() + (now.hour() << 8)) as u16;
    let bytes_date = u16::to_le_bytes(packed_date);
    let bytes_time = u16::to_le_bytes(packed_time);
    return [bytes_date[0],bytes_date[1],bytes_time[0],bytes_time[1]];
}

fn unpack_time(prodos_date_time: [u8;4]) -> chrono::NaiveDateTime {
    let date = u16::from_le_bytes([prodos_date_time[0],prodos_date_time[1]]);
    let time = u16::from_le_bytes([prodos_date_time[2],prodos_date_time[3]]);
    let year = 2000 + (date >> 9);
    let month = (date >> 5) & 15;
    let day = date & 31;
    let hour = (time >> 8) & 255;
    let minute = time & 255;
    return chrono::NaiveDate::from_ymd(year as i32,month as u32,day as u32).
        and_hms(hour as u32,minute as u32,0);
}

/// Convert filename bytes to a string.
/// Must pass the stor_len_nibs field into nibs.
fn file_name_to_string(nibs: u8, fname: [u8;15]) -> String {
    let name_len = nibs & 0x0f;
    let fname_patt = Regex::new(r"^[A-Z][A-Z0-9.]{0,14}$").unwrap();
    if let Ok(result) = String::from_utf8(fname[0..name_len as usize].to_vec()) {
        if fname_patt.is_match(&result) {
            return result;
        }
    }
    panic!("encountered a bad file name on disk");
}
/// Convert storage type and String to (stor_len_nibs,fname).
fn string_to_file_name(stype: &StorageType, s: &str) -> (u8,[u8;15]) {
    let fname_patt = Regex::new(r"^[A-Z][A-Z0-9.]{0,14}$").unwrap();
    if !fname_patt.is_match(&s.to_uppercase()) {
        panic!("attempt to create a bad file name");
    }
    let new_nibs = ((*stype as u8) << 4) + s.len() as u8;
    let mut ans: [u8;15] = [0;15];
    let mut i = 0;
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    return (new_nibs,ans);
}

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

pub const VOL_KEY_BLOCK: u16 = 2;

pub trait Header {
    fn file_count(&self) -> u16;
    fn inc_file_count(&mut self);
    fn dec_file_count(&mut self);
    fn set_access(&mut self,what: Access,which: bool);
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
    pub fn format(&mut self, blocks: u16, vol_name: &String, create_time: Option<chrono::NaiveDateTime>) {
        let (nibs,fname) = string_to_file_name(&StorageType::VolDirHeader, vol_name);
        self.stor_len_nibs = nibs;
        self.name = fname;
        self.create_time = pack_time(create_time);
        self.vers = 0;
        self.min_vers = 0;
        self.access = 1+2+32+64+128; // enable all R W B RN D
        self.entry_len = 0x27;
        self.entries_per_block = 13;
        self.file_count = [0,0];
        self.bitmap_ptr = [6,0];
        self.total_blocks = u16::to_le_bytes(blocks);
    }
}

impl SubDirHeader {
    pub fn create(&mut self, name: &String, parent_ptr: u16, parent_entry_num: u8, create_time: Option<chrono::NaiveDateTime>) {
        let (nibs,fname) = string_to_file_name(&StorageType::SubDirHeader, name);
        self.stor_len_nibs = nibs;
        self.name = fname;
        self.create_time = pack_time(create_time);
        self.vers = 0;
        self.min_vers = 0;
        self.access = 1+2+32+64+128; // enable R W B RN D
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
    pub fn get_header(&self) -> u16 {
        return u16::from_le_bytes(self.header_ptr);
    }
    pub fn eof(&self) -> usize {
        return u32::from_le_bytes([self.eof[0],self.eof[1],self.eof[2],0]) as usize;
    }
    pub fn aux(&self) -> u16 {
        return u16::from_le_bytes(self.aux_type);
    }
    pub fn ftype(&self) -> u8 {
        return self.file_type;
    }
    pub fn set_eof(&mut self,bytes: usize) {
        let inc = u32::to_le_bytes(bytes as u32);
        self.eof = [inc[0],inc[1],inc[2]];
    }
    pub fn delta_blocks(&mut self,delta: i32) {
        let new_val = u16::from_le_bytes(self.blocks_used) as i32 + delta;
        self.blocks_used = u16::to_le_bytes(new_val as u16);
    }
    fn create_generic(&mut self,
        name: &str,
        stype: StorageType,
        ftype: FileType,
        aux: u16,
        key_ptr: u16,
        header_ptr: u16,
        create_time: Option<chrono::NaiveDateTime>) {
        let (nibs,fname) = string_to_file_name(&stype, name);
        self.stor_len_nibs = nibs;
        self.name = fname;
        self.file_type = ftype as u8;
        self.key_ptr = u16::to_le_bytes(key_ptr);
        self.blocks_used = [0,0];
        self.eof = [0,0,0];
        self.create_time = pack_time(create_time);
        self.vers = 0;
        self.min_vers = 0;
        self.access = 1+2+32+64+128;
        self.aux_type = u16::to_le_bytes(aux);
        self.last_mod = pack_time(create_time);
        self.header_ptr = u16::to_le_bytes(header_ptr);
    }
    pub fn create_subdir(name: &str,key_ptr: u16,header_ptr: u16,create_time: Option<chrono::NaiveDateTime>) -> Entry {
        let mut ans = Self::new();
        ans.create_generic(name,StorageType::SubDirEntry,FileType::Directory,0,key_ptr,header_ptr,create_time);
        return ans;
    }
    pub fn create_file(name: &str,ftype: FileType,aux: u16,key_ptr: u16,header_ptr: u16,create_time: Option<chrono::NaiveDateTime>) -> Entry {
        let mut ans = Self::new();
        ans.create_generic(name,StorageType::Seedling,ftype,aux,key_ptr,header_ptr,create_time);
        return ans;
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
    pub fn rename(&mut self,name: &str) {
        let stor = self.storage_type();
        let (nibs,fname) = string_to_file_name(&stor, name);
        self.stor_len_nibs = nibs;
        self.name = fname;
    }
    pub fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // relative to the entry start
        let mut ans = vec![0x18,0x19,0x1a,0x1b,0x01c,0x1d,0x1e,0x21,0x22,0x23,0x24];
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
}

/// Allows the entry to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
/// Intended use is for CATALOG.
impl fmt::Display for Entry {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let mut create_time = "<NO DATE>".to_string();
        let mut mod_time = "<NO DATE>".to_string();
        if self.create_time!=[0,0,0,0] {
            create_time = unpack_time(self.create_time).format("%d-%b-%y %H:%M").to_string();
        }
        if self.last_mod!=[0,0,0,0] {
            mod_time = unpack_time(self.last_mod).format("%d-%b-%y %H:%M").to_string();
        }
        let mut write_protect = "*".to_string();
        if self.access & 0x02 == 0x02 {
            write_protect = " ".to_string();
        }
        //"NAME","TYPE","BLOCKS","MODIFIED","CREATED","ENDFILE","SUBTYPE");
        write!(f,"{}{:15} {:4} {:6} {:16} {:16} {:7} {:7}",
            write_protect,
            match self.file_type { 0x0f => self.name().blue().bold(), _ => self.name().normal() },
            typ_map.get(&self.file_type).expect("unexpected file type"),
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
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // these are relative to the block start
        let mut ans: Vec<usize> = Vec::new();
        for i in 0x14..0x23 {
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
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        // these are relative to the block start
        let mut ans: Vec<usize> = Vec::new();
        for i in 0x14..0x23 {
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
        return Entry::from_bytes(&self.entries[loc.idx-2].to_bytes());
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
        return Entry::from_bytes(&self.entries[loc.idx-1].to_bytes());
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
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        self.prev_block = [bytes[0],bytes[1]];
        self.next_block = [bytes[2],bytes[3]];
        let mut offset = 4;
        self.header.update_from_bytes(&bytes[offset..self.header.len()+offset].to_vec());
        offset += self.header.len();
        for i in 0..self.entries.len() {
            self.entries[i].update_from_bytes(&bytes[offset..offset+self.entries[i].len()].to_vec());
            offset += self.entries[i].len();
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes);
        return ans;
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
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        self.prev_block = [bytes[0],bytes[1]];
        self.next_block = [bytes[2],bytes[3]];
        let mut offset = 4;
        for i in 0..self.entries.len() {
            self.entries[i].update_from_bytes(&bytes[offset..offset+self.entries[i].len()].to_vec());
            offset += self.entries[i].len();
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self where Self: Sized {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes);
        return ans;
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
        match FromPrimitive::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
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
        match FromPrimitive::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
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
        match FromPrimitive::from_u8((self.stor_len_nibs & 0xf0) >> 4) {
            Some(t) => t,
            _ => panic!("encountered unknown storage type")
        }
    }
}

impl Directory for KeyBlock<VolDirHeader> {
    fn parent_entry_loc(&self) -> Option<EntryLocation> {
        return None;
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
        panic!("attempt to delete volume directory")
    }
}

impl Directory for KeyBlock<SubDirHeader> {
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
    fn parent_entry_loc(&self) -> Option<EntryLocation> {
        panic!("attempt to get parent from EntryBlock");
    }
    fn inc_file_count(&mut self) {
        panic!("attempt to access header from EntryBlock");
    }
    fn dec_file_count(&mut self) {
        panic!("attempt to access header from EntryBlock");
    }
    fn standardize(&mut self,offset: usize) -> Vec<usize> {
        Vec::new()
    }
    fn delete(&mut self) {
        panic!("attempt to delete entry block")
    }
}