//! ## CP/M file system module
//! 
//! CP/M encompasses a broad space of computer hardware and disk formats.
//! This module attempts to follow the disk abstractions that were actually used.
//! Each disk is described by a Disk Parameter Block (DPB).
//! In a real CP/M implementation, the DPB was in BIOS or generated on the fly.
//! Here, the `DiskFS` trait object takes ownership of a `types::DiskParameterBlock`.
//! Generally, when we are handed a disk image, we are not given the DPB, but we may
//! have some idea of its contents if we know the kind of disk.
//! 
//! The key concept of the CP/M directory is the "extent," which is a subset of a file's data.
//! In CP/M v1, directory entries and extent entries are one and the same, and the extent capacity is always 16K.
//! In later versions, entries were used for things other than extents, and extent capacities
//! could be multiples of 16K, while indexing remained in terms of 16K units (called logical extents).
//! So we have four important units of quantization in the file system:
//! (i) records of 128 bytes, (ii) blocks of 1K,2K,4K,8K, or 16K (depending on disk type),
//! (iii) logical extents of 16K, and (iv) file extents with a capacity determined by the
//! Disk Parameter Block (DPB).
//! 
//! The module contains components for CP/M versions up to 3, but
//! CP/M 2 behaviors are the default.

pub mod types;
mod directory;

use colored::*;
use chrono::{Timelike,Duration};
use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;
use log::{trace,info,debug,warn,error};
use types::*;
use directory::*;
use super::Chunk;
use crate::img;
use crate::commands::ItemType;

fn pack_date(time: Option<chrono::NaiveDateTime>) -> [u8;4] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let ref_date = chrono::NaiveDate::from_ymd(1978, 1, 1);
    let days = u16::to_le_bytes(now.signed_duration_since(ref_date.and_hms(0, 0, 0)).num_days() as u16);
    let hours = (now.hour() % 100 - now.hour() % 10)*16 + now.hour() % 10;
    let minutes = (now.minute() % 100 - now.minute() % 10)*16 + now.minute() % 10;
    return [days[0],days[1],hours as u8,minutes as u8];
}

fn unpack_date(cpm_date: [u8;4]) -> chrono::NaiveDateTime {
    let ref_date = chrono::NaiveDate::from_ymd(1978, 1, 1);
    let now = ref_date + Duration::days(u16::from_le_bytes([cpm_date[0],cpm_date[1]]).into());
    let hours = (cpm_date[2] & 0x0f) + 10*(cpm_date[2] >> 4);
    let minutes = (cpm_date[3] & 0x0f) + 10*(cpm_date[3] >> 4);
    return now.and_hms(hours.into(), minutes.into(), 0);
}

/// Accepts lower case, case is raised by string_to_file_name
fn is_name_valid(s: &str) -> bool {
    let it: Vec<&str> = s.split('.').collect();
    if it.len()>2 {
        return false;
    }
    let base = it[0];
    let ext = match it.len() {
        1 => "",
        _ => it[1]
    };

    for char in [base,ext].concat().chars() {
        if !char.is_ascii() || INVALID_CHARS.contains(char) || char.is_ascii_control() {
            info!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if base.len()>8 {
        info!("base name too long, max 8");
        return false;
    }
    if ext.len()>3 {
        info!("extension name too long, max 3");
        return false;
    }
    true
}

fn file_name_to_string(name: [u8;8],typ: [u8;3]) -> String {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    [
        crate::escaped_ascii_from_bytes(&base, true, false).trim_end(),
        ".",
        crate::escaped_ascii_from_bytes(&ext, true, false).trim_end()
    ].concat()
}

fn fx_to_string(fx: &Extent) -> String {
    file_name_to_string(fx.name,fx.typ)
}

fn lx_to_string(lx: &Label) -> String {
    file_name_to_string(lx.name, lx.typ)
}

fn string_to_file_name(s: &str) -> ([u8;8],[u8;3]) {
    let mut ans: ([u8;8],[u8;3]) = ([0;8],[0;3]);
    // assumes is_name_valid was true; 
    let upper = s.to_uppercase();
    let it: Vec<&str> = upper.split('.').collect();
    let base = it[0].as_bytes().to_vec();
    let ext = match it.len() {
        1 => Vec::new(),
        _ => it[1].as_bytes().to_vec()
    };
    for i in 0..8 {
        if i<base.len() {
            ans.0[i] = base[i];
        } else {
            ans.0[i] = 0x20;
        }
    }
    for i in 0..3 {
        if i<ext.len() {
            ans.1[i] = ext[i];
        } else {
            ans.1[i] = 0x20;
        }
    }
    return ans;
}

/// Given a CP/M filename string, update the file image
/// with standard access and equate type with extension
fn update_fimg_with_name(fimg: &mut super::FileImage,s: &str) {
    fimg.access = vec![0x20;11];
    let mut temp_fs_type = vec![0x20;3];
    // assumes is_name_valid was true; 
    let upper = s.to_uppercase();
    let it: Vec<&str> = upper.split('.').collect();
    let base = it[0].as_bytes().to_vec();
    let ext = match it.len() {
        1 => Vec::new(),
        _ => it[1].as_bytes().to_vec()
    };
    for i in 0..8 {
        if i<base.len() {
            fimg.access[i] = base[i];
        } else {
            fimg.access[i] = 0x20;
        }
    }
    for i in 0..3 {
        if i<ext.len() {
            fimg.access[8+i] = ext[i];
            temp_fs_type[i] = ext[i];
        } else {
            fimg.access[8+i] = 0x20;
            temp_fs_type[i] = 0x20;
        }
    }
    fimg.fs_type = u32::from_le_bytes([temp_fs_type[0],temp_fs_type[1],temp_fs_type[2],0]);
}

/// Take string such as `2:USER2.TXT` and return (2,"USER2.TXT")
fn split_user_filename(xname: &str) -> Result<(u8,String),Box<dyn std::error::Error>> {
    let parts: Vec<&str> = xname.split(':').collect();
    if parts.len()==1 {
        return Ok((0,xname.to_string()));
    } else {
        if let Ok(user) = u8::from_str(parts[0]) {
            if user<USER_END {
                return Ok((user,parts[1].to_string()));
            } else {
                error!("invalid user number");
                return Err(Box::new(Error::BadFormat));
            }
        }
        error!("prefix in this context should be a user number");
        return Err(Box::new(Error::BadFormat));
    }
}

/// Load directory structure from a borrowed disk image.
/// This is used to test images, as well as being called during FS operations.
fn get_directory(img: &Box<dyn img::DiskImage>,dpb: &DiskParameterBlock) -> Option<Directory> {
    if dpb.disk_capacity() != img.byte_capacity() {
        debug!("size mismatch: DPB has {}, img has {}",dpb.disk_capacity(),img.byte_capacity());
        return None;
    }
    let mut buf: Vec<u8> = Vec::new();
    for iblock in 0..dpb.dir_blocks() {
        if let Ok(dat) = img.read_chunk(Chunk::CPM((iblock,dpb.bsh,dpb.off))) {
            buf.append(&mut dat.clone());
        } else {
            debug!("cannot read CP/M block {}",iblock);
            return None;
        }
    }
    Some(Directory::from_bytes(&buf))
}

/// Build a map from user-stamped filenames to ordered entry pointers, while checking consistency.
/// N.b. each file can have multiple entries.
/// Success can be used as an indication that we have a CP/M file system.
fn build_files(dir: &Directory,dpb: &DiskParameterBlock,cpm_vers: [u8;3]) -> Result<HashMap<String,Vec<Ptr>>,Error> {
    let mut ans: HashMap<String,Vec<Ptr>> = HashMap::new();
    // count number of actions needed to build
    let mut req_actions = 0;
    let mut num_actions = 0;
    for i in 0..dir.num_entries() {
        if  dir.get_type(&Ptr::ExtentEntry(i))==ExtentType::Unknown {
            debug!("unknown extent type in entry {}",i);
            return Err(Error::BadFormat);
        }
        if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(i)) {
            if fx.name[3]>0x7f || fx.name[4]>0x7f || fx.name[5]>0x7f || fx.name[6]>0x7f {
                debug!("unexpected high bits in file name");
                return Err(Error::BadFormat);
            }
            if fx.get_data_ptr().unwrap() >= MAX_LOGICAL_EXTENTS[cpm_vers[0] as usize] {
                debug!("index of extent too large ({})",fx.get_data_ptr().unwrap());
                return Err(Error::BadFormat);
            }
            if fx.user<USER_END {
                trace!("found file {}:{}",fx.user,fx_to_string(&fx));
                req_actions += 1;
            }
        }
    }
    // make as many passes as there could be extents per file
    let max_extents = MAX_LOGICAL_EXTENTS[cpm_vers[0] as usize] / (dpb.exm as usize + 1);
    for x_count in 0..max_extents {
        for dir_idx in 0..dir.num_entries() {
            if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(dir_idx)) {
                let lx_count = x_count*(dpb.exm as usize + 1);
                if fx.user<USER_END && lx_count==fx.get_data_ptr().unwrap() {
                    let mut pointers: Vec<Ptr> = Vec::new();
                    let fname = fx.user.to_string() + ":" + &fx_to_string(&fx);
                    trace!("processing extent {} of file {}",x_count,fname);
                    if let Some(buf) = ans.get(&fname) {
                        pointers.append(&mut buf.clone());
                    }
                    if pointers.len()!=x_count {
                        debug!("{} has {} pointers, but extent count is {}",fname,pointers.len(),x_count);
                        return Err(Error::BadFormat);
                    }
                    for i in fx.get_block_list(dpb) {
                        if i as usize >= dpb.user_blocks() {
                            debug!("bad block pointer in file extent");
                            return Err(Error::BadFormat);
                        }
                    }
                    pointers.push(Ptr::ExtentEntry(dir_idx));
                    ans.insert(fname,pointers);
                    num_actions += 1;
                }
            }
        }
        if num_actions==req_actions {
            return Ok(ans);
        }
    }
    debug!("could not build file list");
    Err(Error::BadFormat)
}

/// The primary interface for disk operations.
/// The "Disk Parameter Block" that is provided upon creation
/// is in 1-to-1 correspondence with the structure that CP/M
/// maintains in its BIOS (or generates somehow)
pub struct Disk
{
    cpm_vers: [u8;3],
    dpb: DiskParameterBlock,
    img: Box<dyn img::DiskImage>
}

impl Disk
{
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image and DPB.
    pub fn from_img(img: Box<dyn img::DiskImage>,dpb: DiskParameterBlock,cpm_vers: [u8;3]) -> Self {
        if !dpb.verify() {
            panic!("disk parameters were invalid");
        }
        return Self {
            cpm_vers,
            dpb,
            img
        }
    }
    /// Test an image for the CP/M file system.
    pub fn test_img(img: &Box<dyn img::DiskImage>,dpb: &DiskParameterBlock,cpm_vers: [u8;3]) -> bool {
        // test the volume directory header
        if let Some(directory) = get_directory(img,dpb) {
            if let Err(_e) = build_files(&directory,dpb,cpm_vers) {
                debug!("Unable to build CP/M file directory");
                return false;
            }
            return true;
        }
        debug!("CP/M directory was not readable");
        return false;
    }
    fn get_directory(&self) -> Directory {
        return get_directory(&self.img,&self.dpb).expect("directory broken");
    }
    fn save_directory(&mut self,dir: &Directory) {
        let buf = dir.to_bytes();
        for iblock in 0..self.dpb.dir_blocks() {
            self.write_block(&buf,iblock,iblock * self.dpb.block_size());
        }
    }
    fn is_block_free(&self,iblock: usize,directory: &Directory) -> bool {
        if iblock < self.dpb.dir_blocks() || iblock >= self.dpb.user_blocks() {
            return false;
        }
        for idx in 0..self.dpb.dir_entries() {
            if let Some(fx) = directory.get_file(&Ptr::ExtentEntry(idx)) {
                for ptr in fx.get_block_list(&self.dpb) {
                    if iblock==ptr as usize {
                        return false;
                    }
                }    
            }
        }
        return true;
    }
    /// number of blocks available for file data
    fn num_free_blocks(&self,dir: &Directory) -> u16 {
        let mut used: usize = self.dpb.dir_blocks();
        for idx in 0..self.dpb.dir_entries() {
            if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(idx)) {
                for ptr in fx.get_block_list(&self.dpb) {
                    if ptr>0 && fx.user<USER_END {
                        used += 1;
                    }
                }    
            }
        }
        return self.dpb.user_blocks() as u16 - used as u16;
    }
    fn is_extent_free(&self,ptr: Ptr,dir: &Directory) -> bool {
        match dir.get_type(&ptr) {
            ExtentType::Deleted | ExtentType::Unknown => true,
            _ => false
        }
    }
    /// extents available in the directory, each can reference up to 16K of data (usually 16 blocks)
    fn num_free_extents(&self,dir: &Directory) -> usize {
        let mut ans: usize = 0;
        for i in 0..dir.num_entries() {
            trace!("check entry {}",i);
            match dir.get_type(&Ptr::ExtentEntry(i)) {
                ExtentType::Deleted | ExtentType::Unknown => {
                    ans += 1
                },
                _ => { debug!("entry {} is used",i); }
            }
        }
        debug!("found {} free extents",ans);
        return ans;
    }
    /// Read a block of data into buffer `data` starting at `offset` within the buffer.
    /// Will read as many bytes as will fit in the buffer starting at `offset`.
    fn read_block(&self,data: &mut Vec<u8>, iblock: usize, offset: usize) {
        let bytes = self.dpb.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        if let Ok(buf) = self.img.read_chunk(Chunk::CPM((iblock,self.dpb.bsh,self.dpb.off))) {
            for i in 0..actual_len {
                data[offset + i] = buf[i];
            }
        } else {
            panic!("read failed for block {}",iblock);
        }
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    /// Same as zap since there is no track bitmap in CP/M file system.
    fn write_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        self.zap_block(data,iblock,offset);
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    fn zap_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        let bytes = self.dpb.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        self.img.write_chunk(Chunk::CPM((iblock,self.dpb.bsh,self.dpb.off)), &data[offset..offset+actual_len].to_vec()).
            expect("write failed");
    }
    fn get_available_block(&self) -> Option<u16> {
        let directory = self.get_directory();
        for block in 0..self.dpb.user_blocks() {
            if self.is_block_free(block,&directory) {
                return Some(block as u16);
            }
        }
        return None;
    }
    fn get_available_extent(&self,dir: &Directory) -> Option<usize> {
        for i in 0..self.dpb.dir_entries() {
            if self.is_extent_free(Ptr::ExtentEntry(i), dir) {
                return Some(i);
            }
        }
        return None;
    }
    /// Format disk for the CP/M file system.  The `time` argument is currently ignored, because we
    /// are not using the disk label extent.
    pub fn format(&mut self, vol_name: &str, _time: Option<chrono::NaiveDateTime>) -> Result<(),Error> {
        if !is_name_valid(vol_name) {
            error!("CP/M volume name invalid");
            return Err(Error::BadFormat);
        }
        // Formatting an empty disk is nothing more than filling all user sectors with
        // the deleted file mark.  If we want to put the OS in the reserved tracks
        // we cannot use `write_block` (we need to use chunks with OFF=0).
        for iblock in 0..self.dpb.user_blocks() {
            self.write_block(&vec![DELETED;self.dpb.block_size()],iblock,0);
        }
        return Ok(());
    }

    /// Scan the directory to find the user's named file and return ordered extent entries.
    /// Try first with the given case, if not found try again with all caps.
    fn get_file_metadata(&self,files: &HashMap<String,Vec<Ptr>>,name: &str,user: u8) -> Option<Vec<Ptr>> {
        let fname = user.to_string() + ":" + name;
        if let Some(pointers) = files.get(&fname) {
            return Some(pointers.clone());
        }
        let fname = user.to_string() + ":" + &name.to_uppercase();
        if let Some(pointers) = files.get(&fname) {
            return Some(pointers.clone());
        }
        return None;
    }
    /// Read any file into the sparse file format. Use `FileImage::sequence` to make the result sequential.
    fn read_file(&self,name: &str,user: u8) -> Result<super::FileImage,Box<dyn std::error::Error>> {
        let dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        if let Some(pointers) = self.get_file_metadata(&files,name,user) {
            let mut ans = super::FileImage::new(self.dpb.block_size());
            ans.file_system = String::from("a2 CP/M");
            let mut buf = vec![0;self.dpb.block_size()];
            let mut count = 0;
            for meta in pointers {
                if let Some(fx) = dir.get_file(&meta) {
                    // For CP/M the access info is encoded in the 8+3 filename.
                    // Furthermore, there is no type beyond the filename extension.
                    // We Therefore store the 8+3 bytes as access, and the 3 bytes as type, all bits being kept.
                    // The following metadata is redundantly extracted from every extent entry
                    ans.fs_type = u32::from_le_bytes([fx.typ[0],fx.typ[1],fx.typ[2],0]);
                    ans.access = [fx.name.to_vec(),fx.typ.to_vec()].concat();
                    ans.eof = fx.get_eof(&self.dpb) as u32;
                    // Get the data
                    for iblock in fx.get_block_list(&self.dpb) {
                        if iblock as usize >= self.dpb.user_blocks() {
                            info!("possible extended data disk (not supported)");
                            return Err(Box::new(Error::ReadError));
                        }
                        if iblock>0 {
                            debug!("read user block {}",iblock);
                            self.read_block(&mut buf, iblock as usize, 0);
                            ans.chunks.insert(count,buf.clone());
                        }
                        count += 1;
                    }
                }
            }
            return Ok(ans);
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Write any file using the sparse file format.  Use `FileImage::desequence` to convert sequential data.
    fn write_file(&mut self,name: &str, user: u8,fimg: &super::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if !is_name_valid(name) {
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        let pointers = self.get_file_metadata(&files,name, user);
        for i in 0..dir.num_entries() {
            if dir.get_type(&Ptr::ExtentEntry(i))==ExtentType::Timestamp {
                error!("CP/M 3 timestamp extents found, cannot write");
                return Err(Box::new(Error::WriteError));
            }
        }
        if pointers==None {
            // this is a new file
            // we do not write anything unless there is room
            // empty files are permitted
            let data_blocks = fimg.chunks.len();
            let mut extents = fimg.end() * self.dpb.block_size() / self.dpb.extent_capacity();
            if (fimg.end() * self.dpb.block_size()) % self.dpb.extent_capacity() > 0 {
                extents += 1;
            }
            // CP/M allows us to create an empty file; it uses an entry, but no data blocks are allocated.
            if extents==0 {
                extents = 1;
            }
            debug!("file requires {} data blocks and {} extents",data_blocks,extents);
            if self.num_free_blocks(&dir) as usize >= data_blocks {
                if self.num_free_extents(&dir) >= extents {
                    // sort out filename and access
                    let (base,typ) = string_to_file_name(name);
                    let img_typ = u32::to_le_bytes(fimg.fs_type);
                    if typ[0]!=img_typ[0] || typ[1]!=img_typ[1] || typ[2]!=img_typ[2] {
                        warn!("CP/M file image type and extension are inconsistent");
                    }
                    let (flgs1,flgs2) = match fimg.access.len() {
                        11 => (fimg.access[0..8].to_vec(),fimg.access[8..11].to_vec()),
                        _ => {
                            warn!("CP/M file image has bad access field (ignoring)");
                            (vec![0;8],vec![0;3])
                        }
                    };
                    // closure to create an extent entry
                    let create_x = |disk: &Disk,dir: &Directory,lx_count: &mut usize,is_last: bool| -> (Ptr,Extent) {
                        trace!("create new extent with index {}",*lx_count);
                        let mut fx = Extent::new();
                        fx.set_name(base,typ);
                        fx.set_flags(flgs1.clone().try_into().expect("unreachable"),flgs2.clone().try_into().expect("unreachable"));
                        fx.user = user;
                        fx.set_data_ptr(Ptr::ExtentData(*lx_count));
                        *lx_count += disk.dpb.exm as usize + 1;
                        if is_last {
                            let mut remainder = fimg.eof as usize % disk.dpb.extent_capacity();
                            if remainder==0 && fimg.eof>0 {
                                remainder = disk.dpb.extent_capacity();
                            }
                            fx.set_eof(remainder,disk.cpm_vers);
                        } else {
                            fx.set_eof(disk.dpb.extent_capacity(),disk.cpm_vers);
                        }
                        let entry_idx = disk.get_available_extent(&dir).unwrap();
                        return (Ptr::ExtentEntry(entry_idx),fx);
                    };
                    // create at least one extent
                    let mut lx_count = 0; // 16K logical extent count
                    let (mut ptr,mut fx) = create_x(&self,&dir,&mut lx_count,fimg.end()<2);
                    dir.set_file(&ptr,&fx);
                    // save blocks (including holes) creating new extents as needed
                    for i in 0..fimg.end() {
                        let n = self.dpb.ptr_size(); // expect 1 or 2
                        let block_idx = i % (16/n); // next block pointer to be used
                        if block_idx==0 && i!=0 {
                            (ptr,fx) = create_x(&self,&dir,&mut lx_count,i==fimg.end()-1);
                        }
                        if fimg.chunks.contains_key(&i) {
                            trace!("write data block {}",block_idx);
                            let iblock = self.get_available_block().unwrap();
                            let block_ptr = u16::to_le_bytes(iblock);
                            for byte in 0..n {
                                fx.block_list[n*block_idx+byte] = block_ptr[byte];
                            }
                            self.write_block(&fimg.chunks[&i],iblock as usize,0);
                            dir.set_file(&ptr,&fx);
                        }
                    }
                    // save the directory changes
                    self.save_directory(&dir);
                    return Ok(0);
                }
                error!("Required {} directory extents not available",extents);
                return Err(Box::new(Error::DirectoryFull));
            } else {
                error!("not enough space");
                return Err(Box::new(Error::DiskFull));
            }
        } else {
            error!("overwriting is not allowed");
            return Err(Box::new(Error::FileExists));
        }
    }
    /// Modify a file, optionally rename, retype, change access flags.
    /// The access array has this code: negative=set high bit low, 0=leave high bit alone, positive=set high bit high.
    /// For this function, filenames include the user, as in `0:fname`, `1:fname`, etc.
    fn modify(&mut self,old_xname: &str,maybe_new_xname: Option<&str>,access: [i8;11]) -> Result<(),Box<dyn std::error::Error>> {
        let (old_user,old_name) = split_user_filename(old_xname)?;
        if !is_name_valid(&old_name) {
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        if let Some(pointers) = self.get_file_metadata(&files,&old_name,old_user) {
            // Rename
            if let Some(new_user_and_name) = maybe_new_xname {
                let (new_user,new_name) = split_user_filename(new_user_and_name)?;
                if !is_name_valid(&new_name) {
                    return Err(Box::new(Error::BadFormat));
                }
                debug!("renaming to {}, user {}",new_name,new_user);
                if let None = self.get_file_metadata(&files,&new_name,new_user) {
                    for ptr in &pointers {
                        if let Some(mut fx) = dir.get_file(ptr) {
                            if fx.get_flags()[8]>0 {
                                error!("{} is read only, unlock first",old_name);
                                return Err(Box::new(Error::FileReadOnly));
                            }
                            let (base,typ) = string_to_file_name(&new_name);
                            fx.user = new_user;
                            fx.set_name(base,typ);
                            dir.set_file(ptr,&fx);
                        }
                    }
                } else {
                    return Err(Box::new(Error::FileExists));
                }
            }
            // Change access
            for ptr in &pointers {
                if let Some(mut fx) = dir.get_file(ptr) {
                    let curr_flags = fx.get_flags();
                    let mut new_flags: [u8;11] = [0;11];
                    for i in 0..11 {
                        new_flags[i] = match access[i] {
                            x if x<0 => 0,
                            x if x>0 => 0x80,
                            _ => curr_flags[i]
                        };
                    }
                    fx.set_flags(new_flags[0..8].try_into().expect("unreachable"),new_flags[8..11].try_into().expect("unreachable"));
                    dir.set_file(ptr,&fx);
                }
            }
            self.save_directory(&dir);
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
}

impl super::DiskFS for Disk {
    fn catalog_to_stdout(&self, _path: &str) -> Result<(),Box<dyn std::error::Error>> {
        let dir = self.get_directory();
        for i in 0..dir.num_entries() {
            if let Some(label) = dir.get_label(&Ptr::ExtentEntry(i)) {
                println!();
                println!("{}",lx_to_string(&label));
            }
        }
        let mut user_count = 0;
        let mut total_count = 0;
        for user in 0..USER_END {
            let mut count = 0;
            for i in 0..dir.num_entries() {
                if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(i)) {
                    match (fx.user,fx.get_data_ptr()) {
                        (u,Ptr::ExtentData(0)) if u==user => {
                            if count==0 {
                                user_count += 1;
                                println!();
                                println!("A>USER {}",user);
                                println!("A>DIR");
                            }
                            let full_name = fx_to_string(&fx);
                            let mut nm = full_name.split(".");
                            let base = match nm.next() {
                                Some(x) => x,
                                None => ""
                            };
                            let ext = match nm.next() {
                                Some(x) => x,
                                None => ""
                            };
                            match count % 4 {
                                0 => { println!(); print!("A") },
                                _ => { print!(" ") },
                            }
                            match fx.get_flags() {
                                // R/O directory file
                                [_,_,_,_,_,_,_,_,0x80,0x00,_] => print!(": {:8} {:3}",base.red(),ext.red()),
                                // R/W system file (hidden)
                                [_,_,_,_,_,_,_,_,0x00,0x80,_] => print!(": {:8} {:3}",base.dimmed(),ext.dimmed()),
                                // R/O system file (hidden)
                                [_,_,_,_,_,_,_,_,0x80,0x80,_] => print!(": {:8} {:3}",base.dimmed().red(),ext.dimmed().red()),
                                // normal
                                _ => print!(": {:8} {:3}",base,ext),
                            }
                            count += 1;
                            total_count += 1;
                        },
                        _ => {}
                    }
                }
            }
            if count>0 {
                println!();
            }
        }
        if total_count>0 {
            println!();
            println!("found {} user{}",user_count,match user_count { 1=>"",_=>"s"});
            println!();
        } else {
            println!();
            println!("NO FILES");
            println!();
        }
        return Ok(());
    }
    fn create(&mut self,_path: &str) -> Result<(),Box<dyn std::error::Error>> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn delete(&mut self,xname: &str) -> Result<(),Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        let mut dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        if let Some(pointers) = self.get_file_metadata(&files,&name,user) {
            for ptr in &pointers {
                if let Some(mut fx) = dir.get_file(ptr) {
                    if fx.get_flags()[8]>0 {
                        error!("{} is read only, unlock first",name);
                        return Err(Box::new(Error::FileReadOnly));
                    }
                    fx.user = DELETED;
                    dir.set_file(ptr,&fx);
                }
            }
            self.save_directory(&dir);
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
    fn lock(&mut self,xname: &str) -> Result<(),Box<dyn std::error::Error>> {
        // CP/M v2 or higher uses bit 7 of typ[0] for read only
        return self.modify(xname,None,[0,0,0,0,0,0,0,0,1,0,0]);
    }
    fn unlock(&mut self,xname: &str) -> Result<(),Box<dyn std::error::Error>> {
        // CP/M v2 or higher uses bit 7 of typ[0] for read only
        return self.modify(xname,None,[0,0,0,0,0,0,0,0,-1,0,0]);
    }
    fn rename(&mut self,old_xname: &str,new_xname: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(old_xname,Some(new_xname),[0;11]);
    }
    fn retype(&mut self,xname: &str,new_type: &str,_sub_type: &str) -> Result<(),Box<dyn std::error::Error>> {
        // CP/M v2 or higher uses bit 7 of typ[1] for system file (hidden file)
        if new_type=="sys" {
            return self.modify(xname,None,[0,0,0,0,0,0,0,0,0,1,0]);
        } else if new_type=="dir" {
            return self.modify(xname,None,[0,0,0,0,0,0,0,0,0,-1,0]);
        }
        else {
            error!("new type must be `dir` or `sys`");
            return Err(Box::new(Error::Select));
        }
    }
    fn bload(&self,xname: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        match self.read_file(&name,user) {
            Ok(sd) => Ok((0,sd.sequence())),
            Err(e) => Err(e)
        }
    }
    fn bsave(&mut self,xname: &str, dat: &Vec<u8>,_start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        let padded = match trailing {
            Some(v) => [dat.clone(),v.clone()].concat(),
            None => dat.clone()
        };
        let mut fimg = super::FileImage::desequence(self.dpb.block_size(),&padded);
        update_fimg_with_name(&mut fimg, &name);
        return self.write_file(&name,user,&fimg);
    }
    fn load(&self,_name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn save(&mut self,_name: &str, _dat: &Vec<u8>, _typ: ItemType, _trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn read_text(&self,xname: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        match self.read_file(&name,user) {
            Ok(sd) => Ok((0,sd.sequence())),
            Err(e) => Err(e)
        }
    }
    fn write_text(&mut self,xname: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        let mut fimg = super::FileImage::desequence(self.dpb.block_size(), dat);
        update_fimg_with_name(&mut fimg, &name);
        return self.write_file(&name, user, &fimg);
    }
    fn read_records(&self,_name: &str,_record_length: usize) -> Result<super::Records,Box<dyn std::error::Error>> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn write_records(&mut self,_name: &str, _records: &super::Records) -> Result<usize,Box<dyn std::error::Error>> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(block) => {
                match self.img.read_chunk(Chunk::CPM((block,self.dpb.bsh,self.dpb.off))) {
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
                if dat.len() > self.dpb.block_size() {
                    return Err(Box::new(Error::Select));
                }
                match self.img.write_chunk(Chunk::CPM((block,self.dpb.bsh,self.dpb.off)), dat) {
                    Ok(()) => Ok(dat.len()),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&self,xname: &str) -> Result<super::FileImage,Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        return self.read_file(&name,user);
    }
    fn write_any(&mut self,xname: &str,dat: &super::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        let (user,name) = split_user_filename(xname)?;
        if dat.chunk_len as usize!=self.dpb.block_size() {
            error!("chunk length {} is incompatible with CP/M",dat.chunk_len);
            return Err(Box::new(Error::Select));
        }
        return self.write_file(&name,user,dat);
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
    fn standardize(&self,_ref_con: u16) -> HashMap<Chunk,Vec<usize>> {
        return HashMap::new();
    }
    fn compare(&self,path: &std::path::Path,ignore: &HashMap<Chunk,Vec<usize>>) {
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        for block in 0..self.dpb.user_blocks() {
            let addr = Chunk::CPM((block,self.dpb.bsh,self.dpb.off));
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
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage> {
        &mut self.img
    }
}