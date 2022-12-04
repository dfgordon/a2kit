//! # Pascal file system module
//! 
//! This module is *not* for the Pascal language, but rather the Pascal file system.
//! Tested only with UCSD Pascal version 1.2.

pub mod types;
mod boot;
mod directory;

use chrono::Datelike;
use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;
use log::{info,debug,error};
use num_traits::FromPrimitive;
use types::*;
use crate::disk_base;
use super::ChunkSpec;
use directory::*;
use crate::create_fs_from_file;

fn pack_date(time: Option<chrono::NaiveDateTime>) -> [u8;2] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let (_is_common_era,year) = now.year_ce();
    let packed_date = (now.month() + (now.day() << 4) + ((year%100) << 9)) as u16;
    return u16::to_le_bytes(packed_date);
}

fn unpack_date(pascal_date: [u8;2]) -> chrono::NaiveDateTime {
    let date = u16::from_le_bytes(pascal_date);
    let year = 1900 + (date >> 9); // choose to stay in the 20th century (Y2K bug)
    let month = date & 15;
    let day = (date >> 4) & 31;
    return chrono::NaiveDate::from_ymd(year as i32,month as u32,day as u32)
        .and_hms(0, 0, 0);
}

/// This will accept lower case; case will be automatically converted as appropriate
fn is_name_valid(s: &str,is_vol: bool) -> bool {
    for char in s.chars() {
        if !char.is_ascii() || INVALID_CHARS.contains(char) || char.is_ascii_control() {
            info!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if s.len()>7 && is_vol {
        info!("volume name too long, max 7");
        return false;
    }
    if s.len()>15 && !is_vol {
        info!("file name too long, max 15");
        return false;
    }
    true
}

fn file_name_to_string(fname: [u8;15],len: u8) -> String {
    // UTF8 failure will cause panic
    let copy = fname[0..len as usize].to_vec();
    if let Ok(result) = String::from_utf8(copy) {
        return result.trim_end().to_string();
    }
    panic!("encountered a bad file name");
}

fn vol_name_to_string(fname: [u8;7],len: u8) -> String {
    // UTF8 failure will cause panic
    let copy = fname[0..len as usize].to_vec();
    if let Ok(result) = String::from_utf8(copy) {
        return result.trim_end().to_string();
    }
    panic!("encountered a bad file name");
}

fn string_to_file_name(s: &str) -> [u8;15] {
    // this panics if the argument is invalid; 
    let mut ans: [u8;15] = [0;15]; // load with null
    let mut i = 0;
    if !is_name_valid(s, false) {
        panic!("attempt to create a bad file name")
    }
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    return ans;
}

fn string_to_vol_name(s: &str) -> [u8;7] {
    // this panics if the argument is invalid; 
    let mut ans: [u8;7] = [0;7]; // load with null
    let mut i = 0;
    if !is_name_valid(s, true) {
        panic!("attempt to create a bad volume name")
    }
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    return ans;
}

/// Load directory structure from a borrowed disk image.
/// This is used to test images, as well as being called during FS operations.
fn get_directory(img: &Box<dyn disk_base::DiskImage>) -> Option<Directory> {
    let mut ans = Directory::new();
    if let Ok(mut buf) = img.read_chunk(ChunkSpec::PO(VOL_HEADER_BLOCK)) {
        ans.header = VolDirHeader::from_bytes(&buf[0..ENTRY_SIZE].to_vec());
        let beg0 = u16::from_le_bytes(ans.header.begin_block);
        let beg = VOL_HEADER_BLOCK as u16;
        let end = u16::from_le_bytes(ans.header.end_block);
        if beg0!=0 || end<=beg || (end as usize)>ans.total_blocks() {
            debug!("bad header: begin block {}, end block {}",beg,end);
            return None;
        }
        // gather up all the directory blocks in a contiguous buffer; this is convenient
        // since the entries are allowed to span 2 blocks.
        buf = vec![0;BLOCK_SIZE*(end as usize - beg as usize)];
        for iblock in beg..end {
            let offset = (iblock-beg) as usize * BLOCK_SIZE;
            if let Ok(temp) = img.read_chunk(ChunkSpec::PO(iblock as usize)) {
                for i in 0..BLOCK_SIZE {
                    buf[offset+i] = temp[i];
                }
            } else {
                debug!("can't get block {}",iblock);
                return None;
            }
        }
        // create all possible entries whether in use or not
        let max_num_entries = buf.len()/ENTRY_SIZE - 1;
        let mut offset = ENTRY_SIZE;
        for _i in 0..max_num_entries {
            ans.entries.push(DirectoryEntry::from_bytes(&buf[offset..offset+ENTRY_SIZE].to_vec()));
            offset += ENTRY_SIZE;
        }
        return Some(ans);
    }
    return None;
}

/// The primary interface for disk operations.
pub struct Disk
{
    img: Box<dyn disk_base::DiskImage>
}

impl Disk
{
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    pub fn from_img(img: Box<dyn disk_base::DiskImage>) -> Self {
        return Self {
            img
        }
    }
    /// Test an image for the Pascal file system.
    pub fn test_img(img: &Box<dyn disk_base::DiskImage>) -> bool {
        // test the volume directory header
        if let Some(directory) = get_directory(img) {
            let beg0 = u16::from_le_bytes(directory.header.begin_block);
            let beg = VOL_HEADER_BLOCK as u16;
            let end = u16::from_le_bytes(directory.header.end_block);
            let tot = u16::from_le_bytes(directory.header.total_blocks);
            if beg0!=0 || end<=beg || end>20 {
                debug!("header begin {} end {}",beg0,end);
                return false;
            }
            // if (tot as usize) != block_count {
            //     debug!("header total blocks {}",tot);
            //     return false;
            // }
            if directory.header.name_len>7 || directory.header.name_len==0 {
                debug!("header name length {}",directory.header.name_len);
                return false;
            }
            if directory.header.file_type != [0,0] {
                debug!("header type {}",u16::from_le_bytes(directory.header.file_type));
                return false;
            }
            for i in 0..directory.header.name_len {
                let c = directory.header.name[i as usize];
                if c<32 || c>126 {
                    debug!("header name character {}",c);
                    return false;
                }
            }
            // test every directory entry
            for i in 0..u16::from_le_bytes(directory.header.num_files) {
                let entry = directory.entries[i as usize];
                let ebeg = u16::from_le_bytes(entry.begin_block);
                let eend = u16::from_le_bytes(entry.end_block);
                if ebeg>0 {
                    if ebeg<end || eend<=ebeg || (eend as u16) > tot {
                        debug!("entry begin {} end {}",ebeg,eend);
                        return false;
                    }
                    if entry.name_len>15 || entry.name_len==0 {
                        debug!("entry name length {}",entry.name_len);
                        return false;
                    }
                    for i in 0..entry.name_len {
                        let c = entry.name[i as usize];
                        if c<32 || c>126 {
                            debug!("entry name char {}",c);
                            return false;
                        }
                    }
                }
            }
            return true;
        }
        debug!("pascal directory was not readable");
        return false;
    }
    fn get_directory(&self) -> Directory {
        return get_directory(&self.img).expect("directory broken");
    }
    fn save_directory(&mut self,dir: &Directory) {
        let beg = VOL_HEADER_BLOCK as u16;
        let end = u16::from_le_bytes(dir.header.end_block);
        let buf = dir.to_bytes();
        for iblock in beg..end {
            self.write_block(&buf,iblock as usize,(iblock-beg) as usize * BLOCK_SIZE);
        }
    }
    fn is_block_free(&self,iblock: usize,directory: &Directory) -> bool {
        if iblock < u16::from_le_bytes(directory.header.end_block) as usize {
            return false;
        }
        for i in 0..u16::from_le_bytes(directory.header.num_files) {
            let beg = u16::from_le_bytes(directory.entries[i as usize].begin_block);
            let end = u16::from_le_bytes(directory.entries[i as usize].end_block);
            if (iblock as u16) >= beg && (iblock as u16) < end {
                return false;
            }
        }
        return true;
    }
    /// Return tuple with (free blocks,largest contiguous span of blocks)
    fn num_free_blocks(&self) -> (u16,u16) {
        let directory = self.get_directory();
        let mut free: u16 = 0;
        let mut count: u16 = 0;
        let mut largest: u16 = 0;
        for i in 0..directory.total_blocks() {
            if self.is_block_free(i,&directory) {
                count += 1;
                free += 1;
            } else {
                if count > largest {
                    largest = count;
                }
                count = 0;
            }
        }
        if count > largest {
            largest = count;
        }
        (free,largest)
    }
    /// Read a block of data into buffer `data` starting at `offset` within the buffer.
    /// Will read as many bytes as will fit in the buffer starting at `offset`.
    fn read_block(&self,data: &mut Vec<u8>, iblock: usize, offset: usize) {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x,
            _ => bytes
        };
        if let Ok(buf) = self.img.read_chunk(ChunkSpec::PO(iblock)) {
            for i in 0..actual_len as usize {
                data[offset + i] = buf[i];
            }
        } else {
            panic!("read failed for block {}",iblock);
        }
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    /// Same as zap since there is no track bitmap in Pascal file system.
    fn write_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        self.zap_block(data,iblock,offset);
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    fn zap_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        self.img.write_chunk(ChunkSpec::PO(iblock), &data[offset..offset+actual_len].to_vec()).
            expect("write failed");
    }
    /// Try to find `num` contiguous free blocks.  If found return the first block index.
    fn get_available_blocks(&self,num: u16) -> Option<u16> {
        let directory = self.get_directory();
        let mut start = 0;
        let mut count = 0;
        for block in 0..directory.total_blocks() as u16 {
            if self.is_block_free(block as usize,&directory) {
                if count==0 {
                    start = block;
                    count += 1;
                } else {
                    count += 1;
                }
                if count==num {
                    return Some(start);
                }
            } else {
                count = 0;
                start = 0;
            }
        }
        return None;
    }
    /// Format disk for the Pascal file system
    /// TODO: why don't we put the DiskKind with the underlying image?
    pub fn format(&mut self, vol_name: &str, fill: u8, disk_kind: &disk_base::DiskKind, time: Option<chrono::NaiveDateTime>) -> Result<(),Error> {
        if !is_name_valid(vol_name, true) {
            return Err(Error::BadTitle);
        }
        let num_blocks = match disk_kind {
            disk_base::DiskKind::A2_525_16 => 280,
            disk_base::DiskKind::A2_35 => 1600,
            disk_base::DiskKind::A2Max => 65535,
            _ => return Err(Error::NoDev)
        };
        // Zero boot and directory blocks
        for iblock in 0..6 {
            self.write_block(&[0;BLOCK_SIZE].to_vec(),iblock,0);
        }
        // Put `fill` value in all remaining blocks
        for iblock in 6..num_blocks {
            self.write_block(&[fill;BLOCK_SIZE].to_vec(),iblock,0);
        }
        // Setup volume directory
        let mut dir = Directory::new();
        dir.header.begin_block = u16::to_le_bytes(0); // points to first boot block, not header
        dir.header.end_block = u16::to_le_bytes(6);
        dir.header.file_type = u16::to_le_bytes(0);
        dir.header.name_len = vol_name.len() as u8;
        dir.header.name = string_to_vol_name(vol_name);
        dir.header.total_blocks = u16::to_le_bytes(num_blocks as u16);
        dir.header.num_files = u16::to_le_bytes(0);
        dir.header.last_access_date = u16::to_le_bytes(0);
        dir.header.last_set_date = pack_date(time);
        dir.header.pad = [0,0,0,0];
        // only need to write the first block, in fact, only first 22 bytes have data
        self.write_block(&dir.to_bytes(),VOL_HEADER_BLOCK,0);

        // boot loader blocks
        match disk_kind {
            disk_base::DiskKind::A2_525_16 => {
                self.write_block(&boot::PASCAL_525_BLOCK0.to_vec(), 0, 0);
                self.write_block(&boot::PASCAL_525_BLOCK1.to_vec(), 1, 0);
            },
            disk_base::DiskKind::A2_35 => {
                error!("unsupported disk type");
                return Err(Error::NoDev)
            },
            _ => {
                error!("unsupported disk type");
                return Err(Error::NoDev)
            }
        }
        return Ok(());
    }

    /// Scan the directory to find the named file and return (Option<entry index>, directory).
    /// N.b. Pascal FS always keeps files in contiguous blocks.
    fn get_file_entry(&self,name: &str) -> (Option<usize>,Directory) {
        let directory = self.get_directory();
        let fname = string_to_file_name(name);
        for i in 0..u16::from_le_bytes(directory.header.num_files) {
            let entry = &directory.entries[i as usize];
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg>0 && end>beg && (end as usize)<directory.total_blocks() {
                if fname[0..entry.name_len as usize]==entry.name[0..entry.name_len as usize] {
                    return (Some(i as usize),directory);
                }
            }
        }
        return (None,directory);
    }
    /// Read any file into the sparse file format.  The fact that the Pascal FS does not
    /// have sparse files presents no difficulty, since `FileImage` is quite general.
    /// As usual we can use `FileImage::sequence` to make the result sequential.
    fn read_file(&self,name: &str) -> Result<disk_base::FileImage,Box<dyn std::error::Error>> {
        if let (Some(idx),dir) = self.get_file_entry(name) {
            let entry = &dir.entries[idx];
            let mut ans = disk_base::FileImage::new(BLOCK_SIZE);
            ans.file_system = String::from("a2 pascal");
            let mut buf = vec![0;BLOCK_SIZE];
            let mut count: usize = 0;
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            let ftype = u16::from_le_bytes(entry.file_type);
            for iblock in beg..end {
                self.read_block(&mut buf, iblock as usize, 0);
                ans.chunks.insert(count,buf.clone());
                count += 1;
            }
            ans.fs_type = ftype as u32;
            ans.eof = BLOCK_SIZE as u32*ans.chunks.len() as u32 - u16::from_le_bytes(entry.bytes_remaining) as u32;
            ans.modified = u16::from_le_bytes(entry.mod_date) as u32;
            return Ok(ans);
        }
        return Err(Box::new(Error::NoFile));
    }
    /// Write any file using the sparse file format.  The caller must ensure that the
    /// chunks are sequential (Pascal only supports sequential data).  This is easy:
    /// use `FileImage::desequence` to put sequential data into the sparse file format.
    fn write_file(&mut self,name: &str, dat: &disk_base::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if !is_name_valid(name,false) {
            return Err(Box::new(Error::BadFormat));
        }
        let (maybe_idx,mut dir) = self.get_file_entry(name);
        if maybe_idx==None {
            // this is a new file
            // we do not write anything unless there is room
            assert!(dat.chunks.len()>0);
            let data_blocks = dat.chunks.len();
            if let Some(fs_type) = FileType::from_u32(dat.fs_type) {
                if let Some(beg) = self.get_available_blocks(data_blocks as u16) {
                    for i in 0..dir.entries.len() {
                        if dir.entries[i].begin_block==[0,0] {
                            debug!("using entry {}, {}",i,file_name_to_string(dir.entries[i].name,dir.entries[i].name_len));
                            dir.entries[i].begin_block = u16::to_le_bytes(beg);
                            dir.entries[i].end_block = u16::to_le_bytes(beg+data_blocks as u16);
                            dir.entries[i].file_type = u16::to_le_bytes(fs_type as u16);
                            dir.entries[i].name_len = name.len() as u8;
                            dir.entries[i].name = string_to_file_name(name);
                            dir.entries[i].bytes_remaining = u16::to_le_bytes((BLOCK_SIZE*data_blocks - dat.eof as usize) as u16);
                            dir.entries[i].mod_date = pack_date(None); // None means use system clock
                            dir.header.num_files = u16::to_le_bytes(u16::from_le_bytes(dir.header.num_files)+1);
                            dir.header.last_access_date = pack_date(None);
                            self.save_directory(&dir);
                            for b in 0..data_blocks {
                                self.write_block(&dat.chunks[&b],beg as usize+b,0);
                            }
                            return Ok(data_blocks);
                        }
                    }
                    error!("directory is full");
                    return Err(Box::new(Error::NoRoom));
                } else {
                    error!("not enough contiguous space");
                    return Err(Box::new(Error::NoRoom));
                }
            } else {
                error!("unknown file type");
                return Err(Box::new(Error::BadMode));
            }
        } else {
            error!("overwriting is not allowed");
            return Err(Box::new(Error::DuplicateFilename));
        }
    }
    /// modify a file entry, optionally rename, retype.
    fn modify(&mut self,name: &str,maybe_new_name: Option<&str>,maybe_ftype: Option<&str>) -> Result<(),Box<dyn std::error::Error>> {
        if !is_name_valid(name, false) {
            return Err(Box::new(Error::BadFormat));
        }
        if let (Some(idx),mut dir) = self.get_file_entry(name) {
            let mut entry = &mut dir.entries[idx];
            if let Some(new_name) = maybe_new_name {
                if !is_name_valid(new_name,false) {
                    return Err(Box::new(Error::BadFormat));
                }
                entry.name = string_to_file_name(new_name);
            }
            if let Some(ftype) = maybe_ftype {
                match FileType::from_str(ftype) {
                    Ok(typ) => entry.file_type = u16::to_le_bytes(typ as u16),
                    Err(e) => return Err(Box::new(e))
                }
            }
            self.save_directory(&dir);
            return Ok(());
        } else {
            return Err(Box::new(Error::NoFile));
        }
    }
}

impl disk_base::DiskFS for Disk {
    fn catalog_to_stdout(&self, _path: &str) -> Result<(),Box<dyn std::error::Error>> {
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let dir = self.get_directory();
        let total = dir.total_blocks();
        println!();
        println!("{}:",vol_name_to_string(dir.header.name,dir.header.name_len));
        let expected_count = u16::from_le_bytes(dir.header.num_files);
        let mut file_count = 0;
        for entry in dir.entries {
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg!=0 && end>beg && (end as usize)<total {
                let name = file_name_to_string(entry.name,entry.name_len);
                let blocks = end - beg;
                let mut date = "<NO DATE>".to_string();
                if entry.mod_date!=[0,0] {
                    date = unpack_date(entry.mod_date).format("%d-%b-%y").to_string();
                }
                let typ = match typ_map.get(&entry.file_type[0]) {
                    Some(s) => s,
                    None => "????"
                };
                println!("{:15} {:4} {:9}  {:4}",name,blocks,date,typ);
                file_count += 1;
            }
        }
        println!();
        let (free,largest) = self.num_free_blocks();
        let used = total-free as usize;
        println!("{}/{} files<listed/in-dir>, {} blocks used, {} unused, {} in largest",file_count,expected_count,used,free,largest);
        println!();
        return Ok(());
    }
    fn create(&mut self,_path: &str) -> Result<(),Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn delete(&mut self,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        if let (Some(idx),mut dir) = self.get_file_entry(name) {
            for i in idx..dir.entries.len() {
                if i+1 < dir.entries.len() {
                    dir.entries[i] = dir.entries[i+1];
                } else {
                    dir.entries[i].begin_block = [0,0];
                    dir.entries[i].end_block = [0,0];
                }
            }
            dir.header.num_files = u16::to_le_bytes(u16::from_le_bytes(dir.header.num_files)-1);
            self.save_directory(&dir);
            return Ok(());
        } else {
            return Err(Box::new(Error::NoFile));
        }
    }
    fn lock(&mut self,_name: &str) -> Result<(),Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn unlock(&mut self,_name: &str) -> Result<(),Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn rename(&mut self,old_name: &str,new_name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(old_name,Some(new_name),None);
    }
    fn retype(&mut self,name: &str,new_type: &str,_sub_type: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(name, None, Some(new_type));
    }
    fn bload(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(sd) => Ok((0,sd.sequence())),
            Err(e) => Err(e)
        }
    }
    fn bsave(&mut self,name: &str, dat: &Vec<u8>,_start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let padded = match trailing {
            Some(v) => [dat.clone(),v.clone()].concat(),
            None => dat.clone()
        };
        let mut bin_file = disk_base::FileImage::desequence(BLOCK_SIZE,&padded);
        bin_file.fs_type = FileType::from_str("bin").expect("unreachable") as u32;
        return self.write_file(name,&bin_file);
    }
    fn load(&self,_name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn save(&mut self,_name: &str, _dat: &Vec<u8>, _typ: disk_base::ItemType, _trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn read_text(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(sd) => Ok((0,sd.sequence())),
            Err(e) => Err(e)
        }
    }
    fn write_text(&mut self,_name: &str, _dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn read_records(&self,_name: &str,_record_length: usize) -> Result<disk_base::Records,Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn write_records(&mut self,_name: &str, _records: &disk_base::Records) -> Result<usize,Box<dyn std::error::Error>> {
        eprintln!("pascal implementation does not support operation");
        return Err(Box::new(Error::DevErr));
    }
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(block) => {
                match self.img.read_chunk(ChunkSpec::PO(block)) {
                    Ok(buf) => Ok((0,buf)),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_chunk(&mut self, num: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > BLOCK_SIZE {
                    return Err(Box::new(Error::DevErr));
                }
                match self.img.write_chunk(ChunkSpec::PO(block), dat) {
                    Ok(()) => Ok(dat.len()),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&self,name: &str) -> Result<disk_base::FileImage,Box<dyn std::error::Error>> {
        return self.read_file(name);
    }
    fn write_any(&mut self,name: &str,dat: &disk_base::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if dat.chunk_len as usize!=BLOCK_SIZE {
            eprintln!("chunk length {} is incompatible with Pascal",dat.chunk_len);
            return Err(Box::new(Error::DevErr));
        }
        return self.write_file(name,dat);
    }
    fn decode_text(&self,dat: &Vec<u8>) -> String {
        let file = types::SequentialText::from_bytes(&dat);
        return file.to_string();
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&self,_ref_con: u16) -> HashMap<ChunkSpec,Vec<usize>> {
        // want to ignore dates, these occur at offest 18 and 20 in the header,
        // and at offset 24 in every entry
        let mut ans: HashMap<ChunkSpec,Vec<usize>> = HashMap::new();
        let dir = self.get_directory();
        let beg = VOL_HEADER_BLOCK; // begin_block points to boot block
        let end = u16::from_le_bytes(dir.header.end_block);
        let mut offset = 0;
        ans.insert(ChunkSpec::PO(beg),vec![18,19,20,21]);
        offset += dir.header.len();
        loop {
            if offset+25 >= BLOCK_SIZE*(end as usize) {
                break;
            }
            let key = ChunkSpec::PO(beg+offset/BLOCK_SIZE);
            let val = vec![(24+offset)%BLOCK_SIZE, (25+offset)%BLOCK_SIZE];
            super::add_ignorable_offsets(&mut ans, key, val);
            offset += ENTRY_SIZE;
        }
        return ans;
    }
    fn compare(&self,path: &std::path::Path,ignore: &HashMap<ChunkSpec,Vec<usize>>) {
        let mut emulator_disk = create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        let dir = self.get_directory();
        for block in 0..dir.total_blocks() {
            let addr = ChunkSpec::PO(block);
            let mut actual = self.img.read_chunk(addr).expect("bad sector access");
            let mut expected = emulator_disk.get_img().read_chunk(addr).expect("bad sector access");
            if let Some(ignorable) = ignore.get(&addr) {
                for offset in ignorable {
                    actual[*offset] = 0;
                    expected[*offset] = 0;
                }
            }
            for row in 0..16 {
                let mut fmt_actual = String::new();
                let mut fmt_expected = String::new();
                let offset = row*32;
                write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
                write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
                assert_eq!(fmt_actual,fmt_expected," at block {}, row {}",block,row)
            }
        }
    }
    fn get_ordering(&self) -> disk_base::DiskImageType {
        return disk_base::DiskImageType::PO;
    }
    fn get_img(&mut self) -> &mut Box<dyn disk_base::DiskImage> {
        &mut self.img
    }
}