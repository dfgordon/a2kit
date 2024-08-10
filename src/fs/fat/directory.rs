//! ### FAT directory structures
//! 
//! This module encapsulates the FAT directory.  The FAT itself is implemented in
//! `crate::bios::fat`.  The BPB is in `crate::bios::bpb`.

use std::collections::BTreeMap;
use chrono::{NaiveDate,NaiveTime};
use log::{debug,warn,trace};
use super::types::*;
use crate::fs::FileImage;
use crate::{STDRESULT,DYNERR};

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;

/// Size of the directory entry in bytes, always 32
pub const DIR_ENTRY_SIZE: usize = 32;
/// first name byte for a free entry.
const FREE: u8 = 0xe5;
/// first name byte for a free entry, but also indicating no more entries to follow.
const FREE_AND_NO_MORE: u8 = 0x00;

pub const READ_ONLY: u8 = 1;
pub const HIDDEN: u8 = 2;
pub const SYSTEM: u8 = 4;
pub const VOLUME_ID: u8 = 8;
pub const DIRECTORY: u8 = 16;
pub const ARCHIVE: u8 = 32;
pub const LONG_NAME: u8 = 15;
pub const LONG_NAME_SUB: u8 = 63;

/// Convenient collection of information about a file.
/// Flags are broken out into their own variables.
/// This is the value of the map produced by Directory::build_files.
#[derive(Clone)]
pub struct FileInfo {
    pub is_root: bool,
    pub wildcard: String,
    pub idx: usize,
    pub name: String,
    pub typ: String,
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub volume_id: bool,
    pub directory: bool,
    pub archived: bool,
    pub long_name: bool,
    pub long_name_sub: bool,
    pub create_date: Option<NaiveDate>,
    pub create_time: Option<NaiveTime>,
    pub write_date: Option<NaiveDate>,
    pub write_time: Option<NaiveTime>,
    pub access_date: Option<NaiveDate>,
    pub eof: usize,
    pub cluster1: Option<Ptr>
}

#[derive(PartialEq)]
pub enum EntryType {
    Free,
    FreeAndNoMore,
    File,
    Directory,
    VolumeLabel,
    LongName
}

/// encapsulates information needed to manipulate a directory entry
pub struct EntryLocation {
    /// starting cluster of the directory (not the entry!), if cluster1.is_none(), this is a FAT12/16 root directory
    pub cluster1: Option<Ptr>,
    /// the entry in this directory we are interested in
    pub entry: Ptr,
    /// the entire directory as a vector of entries
    pub dir: Directory
}

#[derive(DiskStruct)]
pub struct Entry {
    name: [u8;8],
    ext: [u8;3],
    /// RO=1,hidden=2,sys=4,vol=8,dir=16,archive=32,long_name=15.
    /// If this is the volume label, cluster1=0.
    /// If this is a directory, file_size=0.
    attr: u8,
    nt_res: u8,
    /// tenths of a second, 0-199 according to MS (typo?)
    creation_tenth: u8,
    /// to the nearest 2 secs
    creation_time: [u8;2],
    creation_date: [u8;2],
    access_date: [u8;2],
    cluster1_high: [u8;2],
    /// set at creation time also
    write_time: [u8;2],
    /// set at creation date also
    write_date: [u8;2],
    cluster1_low: [u8;2],
    file_size: [u8;4]
}

/// Directory is merely a packed sequence of entries.
pub struct Directory {
    entries: Vec<[u8;DIR_ENTRY_SIZE]>
}

impl FileInfo {
    /// for FAT12 or FAT16 set cluster1=0
    pub fn create_root(cluster1: usize) -> Self {
        Self {
            is_root: true,
            wildcard: String::new(),
            idx: 0,
            name: "".to_string(),
            typ: "".to_string(),
            read_only: false,
            hidden: false,
            system: false,
            volume_id: false,
            directory: true,
            archived: false,
            long_name: false,
            long_name_sub: false,
            create_date: None,
            create_time: None,
            write_date: None,
            write_time: None,
            access_date: None,
            eof: 0,
            cluster1: match cluster1 {
                0 => None,
                _ => Some(Ptr::Cluster(cluster1))
            }
        }
    }
    /// represent file info as a wildcard pattern
    pub fn create_wildcard(pattern: &str) -> Self {
        Self {
            is_root: false,
            wildcard: String::from(pattern),
            idx: 0,
            name: "".to_string(),
            typ: "".to_string(),
            read_only: false,
            hidden: false,
            system: false,
            volume_id: false,
            directory: true,
            archived: false,
            long_name: false,
            long_name_sub: false,
            create_date: None,
            create_time: None,
            write_date: None,
            write_time: None,
            access_date: None,
            eof: 0,
            cluster1: None
        }
    }
}

impl Entry {
    /// Create an entry with given name and timestamp (time==None means use current time).
    /// Not to be used to create a label entry.
    pub fn create(name: &str, time: Option<chrono::NaiveDateTime>) -> Self {
        let now = match time {
            Some(t) => t,
            None => chrono::Local::now().naive_local()
        };
        let tenths = super::pack::pack_tenths(Some(now));
        let time = super::pack::pack_time(Some(now));
        let date = super::pack::pack_date(Some(now));
        let (base,ext) = super::pack::string_to_file_name(name);
        Self {
            name: base,
            ext,
            attr: 0,
            nt_res: 0,
            creation_tenth: tenths,
            creation_time: time,
            creation_date: date,
            access_date: date,
            cluster1_high: [0,0],
            write_time: time,
            write_date: date,
            cluster1_low: [0,0],
            file_size: [0,0,0,0]
        }
    }
    /// Create a label entry with given name and timestamp (time==None means use current time).
    pub fn create_label(name: &str, time: Option<chrono::NaiveDateTime>) -> Self {
        let now = match time {
            Some(t) => t,
            None => chrono::Local::now().naive_local()
        };
        let tenths = super::pack::pack_tenths(Some(now));
        let time = super::pack::pack_time(Some(now));
        let date = super::pack::pack_date(Some(now));
        let (base,ext) = super::pack::string_to_label_name(name);
        Self {
            name: base,
            ext,
            attr: VOLUME_ID,
            nt_res: 0,
            creation_tenth: tenths,
            creation_time: time,
            creation_date: date,
            access_date: date,
            cluster1_high: [0,0],
            write_time: time,
            write_date: date,
            cluster1_low: [0,0],
            file_size: [0,0,0,0]
        }
    }
    pub fn erase(&mut self,none_follow: bool) {
        match none_follow {
            true => self.name[0] = FREE_AND_NO_MORE,
            false => self.name[0] = FREE
        }
    }
    pub fn set_cluster(&mut self,cluster: usize) {
        let [b1,b2,b3,b4] = u32::to_le_bytes(cluster as u32);
        self.cluster1_low = [b1,b2];
        self.cluster1_high = [b3,b4];
    }
    /// Create a subdirectory at `new_cluster`, in directory at `parent_cluster` (0 if root even for FAT32).
    /// Return the (parent entry, directory buffer), where the buffer includes the dot and dotdot entries.
    /// The clusters are expected to be written by the caller.
    pub fn create_subdir(name: &str,parent_cluster: usize,new_cluster: usize,block_size: usize,time: Option<chrono::NaiveDateTime>) -> (Self,Vec<u8>) {
        let mut dot = Entry::create(".",time);
        let mut dotdot = Entry::create("..",time);
        dot.attr = DIRECTORY;
        dot.set_cluster(new_cluster);
        dotdot.attr = DIRECTORY;
        dotdot.set_cluster(parent_cluster);
        let mut dir = Directory::new();
        dir.expand(block_size/DIR_ENTRY_SIZE);
        dir.set_entry(&Ptr::Entry(0),&dot);
        dir.set_entry(&Ptr::Entry(1),&dotdot);
        dot.rename(name);
        (dot,dir.to_bytes())
    }
    pub fn name(&self,label: bool) -> String {
        let prim = super::pack::file_name_to_string(self.name, self.ext);
        match label {
            true => prim.replace(".",""),
            false => prim
        }
    }
    pub fn rename(&mut self,new_name: &str) {
        let (name,ext) = super::pack::string_to_file_name(new_name);
        self.name = name;
        self.ext = ext;
    }
    pub fn eof(&self) -> usize {
        u32::from_le_bytes(self.file_size) as usize
    }
    /// access date is lost with this version of file image
    pub fn metadata_to_fimg(&self,fimg: &mut FileImage) {
        fimg.set_eof(self.eof());
        fimg.access = vec![self.attr];
        fimg.fs_type = self.ext.to_vec();
        fimg.aux = vec![];
        fimg.created = [vec![self.creation_tenth],self.creation_time.to_vec(),self.creation_date.to_vec()].concat();
        fimg.modified = [self.write_time.to_vec(),self.write_date.to_vec()].concat();
        fimg.version = vec![];
        fimg.min_version = vec![];
    }
    /// access date is set to modified date
    pub fn fimg_to_metadata(&mut self,fimg: &FileImage,use_fimg_time: bool) -> STDRESULT {
        self.file_size = match fimg.eof[0..4].try_into() {
            Ok(x) => x,
            Err(e) => return Err(Box::new(e))
        };
        self.attr = fimg.access[0];
        if use_fimg_time {
            self.creation_tenth =fimg.created[0];
            self.creation_time = match fimg.created[1..3].try_into() {
                Ok(x) => x,
                Err(e) => return Err(Box::new(e))
            };
            self.creation_date = match fimg.created[3..5].try_into() {
                Ok(x) => x,
                Err(e) => return Err(Box::new(e))
            };
            self.write_time = match fimg.modified[0..2].try_into() {
                Ok(x) => x,
                Err(e) => return Err(Box::new(e))
            };
            self.write_date = match fimg.modified[2..4].try_into() {
                Ok(x) => x,
                Err(e) => return Err(Box::new(e))
            };
            self.access_date = match fimg.modified[2..4].try_into() {
                Ok(x) => x,
                Err(e) => return Err(Box::new(e))
            };
        }
        Ok(())
    }
    pub fn get_attr(&self,mask: u8) -> bool {
        (self.attr & mask) > 0
    }
    /// set bits high wherever mask is high (attr | mask)
    pub fn set_attr(&mut self,mask: u8) {
        self.attr |= mask;
    }
    /// set bits low wherever mask is high (attr & !mask)
    pub fn clear_attr(&mut self,mask: u8) {
        self.attr &= !mask;
    }
    pub fn standardize(offset: usize) -> Vec<usize> {
        // relative to the entry start
        // creation date, access date, write date
        let ans = vec![13,14,15,16,17,18,19,22,23,24,25];
        ans.iter().map(|x| x + offset).collect()
    }
    // pub fn cluster1(&self) -> usize {
    //     u32::from_le_bytes([self.cluster1_low[0],self.cluster1_low[1],self.cluster1_high[0],self.cluster1_high[1]]) as usize
    // }
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
        let num_entries = bytes.len()/DIR_ENTRY_SIZE;
        if bytes.len()%DIR_ENTRY_SIZE!=0 {
            warn!("directory buffer wrong size");
        }
        for i in 0..num_entries {
            let entry_buf = match bytes[i*DIR_ENTRY_SIZE..(i+1)*DIR_ENTRY_SIZE].try_into() {
                Ok(buf) => buf,
                Err(_) => return Err(DiskStructError::OutOfData)
            };
            self.entries.push(entry_buf);
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
    pub fn expand(&mut self,count: usize) {
        for _i in 0..count {
            self.entries.push([0;32]);
        }
    }
    pub fn get_type(&self,ptr: &Ptr) -> EntryType {
        let (idx,nm0,attr) = match ptr {
            Ptr::Entry(i) => (*i,self.entries[*i][0],self.entries[*i][11]),
            _ => panic!("wrong pointer type")
        };
        trace!("entry {} has name[0] {} and attr {}",idx,nm0,attr);
        match (nm0,attr) {
            (0xe5,_) => EntryType::Free,
            (0x00,_) => EntryType::FreeAndNoMore,
            (_,a) if a & LONG_NAME >= LONG_NAME => EntryType::LongName,
            (_,a) if a & VOLUME_ID > 0 => EntryType::VolumeLabel,
            (_,a) if a & DIRECTORY > 0 => EntryType::Directory,
            _ => EntryType::File
        }
    }
    pub fn get_raw_entry(&self,ptr: &Ptr) -> [u8;DIR_ENTRY_SIZE] {
        match ptr {
            Ptr::Entry(i) => self.entries[*i].clone(),
            _ => panic!("wrong pointer type")
        }
    }
    pub fn get_entry(&self,ptr: &Ptr) -> Entry {
        match ptr {
            Ptr::Entry(idx) => Entry::from_bytes(&self.entries[*idx]).expect("unexpected size"),
            _ => panic!("wrong pointer type")
        }
    }
    pub fn set_entry(&mut self,ptr: &Ptr,entry: &Entry) {
        match ptr {
            Ptr::Entry(idx) => {
                self.entries[*idx] = entry.to_bytes().try_into().expect("unexpected size")
            },
            _ => panic!("wrong pointer type")
        }
    }
    /// If this is the root directory there may be a disk label entry
    pub fn find_label(&self) -> Option<Entry> {
        for i in 0..self.num_entries() {
            let ptr = Ptr::Entry(i);
            if self.get_type(&ptr)==EntryType::VolumeLabel {
                return Some(self.get_entry(&ptr));
            }
        }
        None
    }
    /// Build an alphabetized map of file names to file info.
    pub fn build_files(&self) -> Result<BTreeMap<String,FileInfo>,DYNERR> {
        let mut bad_names = 0;
        let mut ans = BTreeMap::new();
        // first pass collects everything except passwords
        for i in 0..self.num_entries() {
            let etyp = self.get_type(&Ptr::Entry(i));
            if etyp==EntryType::Free {
                continue;
            }
            if etyp==EntryType::FreeAndNoMore {
                break;
            }
            let entry = self.get_entry(&Ptr::Entry(i));    
            let (name,typ) = super::pack::file_name_to_split_string(entry.name, entry.ext);
            let key = [name.clone(),".".to_string(),typ.clone()].concat();
            if !super::pack::is_name_valid(&key) {
                bad_names += 1;
            }
            if bad_names > 2 {
                debug!("after {} bad file names rejecting disk",bad_names);
                return Err(Box::new(Error::Syntax));
            }
            trace!("entry in use: {}",key);
            if ans.contains_key(&key) {
                debug!("duplicate file {} in directory",key);
                return Err(Box::new(Error::DuplicateFile));
            }
            let cluster1 = Ptr::Cluster(u16::from_le_bytes(entry.cluster1_low) as usize + 
                (u16::MAX as usize) * (u16::from_le_bytes(entry.cluster1_high) as usize));
            let finfo: FileInfo = FileInfo {
                is_root: false,
                wildcard: String::new(),
                idx: i,
                name,
                typ,
                read_only: (entry.attr & READ_ONLY) > 0,
                hidden: (entry.attr & HIDDEN) > 0,
                system: (entry.attr & SYSTEM) > 0,
                volume_id: (entry.attr & VOLUME_ID) > 0,
                directory: (entry.attr & DIRECTORY) > 0,
                archived: (entry.attr & ARCHIVE) > 0,
                long_name: (entry.attr & LONG_NAME) > 0,
                long_name_sub: (entry.attr & LONG_NAME_SUB) > 0,
                write_date: super::pack::unpack_date(entry.write_date),
                write_time: super::pack::unpack_time(entry.write_time,0),
                create_date: super::pack::unpack_date(entry.creation_date),
                create_time: super::pack::unpack_time(entry.creation_time,entry.creation_tenth),
                access_date: super::pack::unpack_date(entry.access_date),
                eof: u32::from_le_bytes(entry.file_size) as usize,
                cluster1: Some(cluster1)
            };
            ans.insert(key.clone(),finfo);
        }
        Ok(ans)
    }
    /// Sort the files based on the order of appearance in the directory.
    /// Panics if there is a file with an empty entry list.
    pub fn sort_on_entry_index(&self,files: &BTreeMap<String,FileInfo>) -> BTreeMap<usize,FileInfo> {
        let mut ans = BTreeMap::new();
        for f in files.values() {
            ans.insert(f.idx,f.clone());
        }
        ans 
    }
 }

/// Search for a file in the map produced by `Directory::build_files`.
/// This will try with the given case, and then with upper case.
/// This will also handle empty extensions reliably.
pub fn get_file<'a>(name: &str,files: &'a BTreeMap<String,FileInfo>) -> Option<&'a FileInfo> {
    let mut trimmed = name.trim_end().to_string();
    if !name.contains(".") {
        trimmed += ".";
    }
    // the order of these attempts is significant
    if let Some(finfo) = files.get(&trimmed) {
        return Some(finfo);
    }
    if let Some(finfo) = files.get(&trimmed.to_uppercase()) {
        return Some(finfo);
    }
    return None;
}

