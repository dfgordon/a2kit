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
use std::collections::{HashSet,HashMap};
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;
use log::{trace,info,debug,warn,error};
use types::*;
use directory::*;
use super::Block;
use crate::bios::dpb::DiskParameterBlock;
use crate::img;
use crate::commands::ItemType;

use crate::{STDRESULT,DYNERR};

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
            debug!("bad file name character `{}` (codepoint {})",char,char as u32);
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

/// put the filename bytes as an ASCII string, result can be tested for validity
/// with `is_name_valid`
fn file_name_to_string(name: [u8;8],typ: [u8;3]) -> String {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    [
        &String::from_utf8(base).expect("unreachable").trim_end(),
        ".",
        &String::from_utf8(ext).expect("unreachable").trim_end(),
    ].concat()
}

fn fx_to_string(fx: &Extent) -> String {
    file_name_to_string(fx.name,fx.typ)
}

fn lx_to_string(lx: &Label) -> String {
    file_name_to_string(lx.name, lx.typ)
}

/// put the file name bytes as an ASCII string with hex escapes
fn file_name_to_string_escaped(name: [u8;8],typ: [u8;3]) -> String {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    let base_str = crate::escaped_ascii_from_bytes(&base, true, false);
    let ext_str = crate::escaped_ascii_from_bytes(&ext, true, false);
    match ext_str.trim_end().len() {
        0 => base_str.trim_end().to_string(),
        _ => [base_str.trim_end(),".",ext_str.trim_end()].concat()
    }
}

fn fx_to_string_escaped(fx: &Extent) -> String {
    file_name_to_string_escaped(fx.name,fx.typ)
}

fn lx_to_string_escaped(lx: &Label) -> String {
    file_name_to_string_escaped(lx.name, lx.typ)
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
    fimg.fs_type = vec![temp_fs_type[0],temp_fs_type[1],temp_fs_type[2]];
}

/// Take string such as `2:USER2.TXT` and return (2,"USER2.TXT")
fn split_user_filename(xname: &str) -> Result<(u8,String),DYNERR> {
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
fn get_directory(img: &mut Box<dyn img::DiskImage>,dpb: &DiskParameterBlock) -> Option<Directory> {
    if dpb.disk_capacity() != img.byte_capacity() {
        debug!("size mismatch: DPB has {}, img has {}",dpb.disk_capacity(),img.byte_capacity());
        return None;
    } else {
        debug!("size matched: DPB and img both have {}",dpb.disk_capacity());
    }
    let mut buf: Vec<u8> = Vec::new();
    for iblock in 0..dpb.dir_blocks() {
        if let Ok(dat) = img.read_block(Block::CPM((iblock,dpb.bsh,dpb.off))) {
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
        if dir.get_type(&Ptr::ExtentEntry(i))==ExtentType::Unknown {
            debug!("unknown extent type in entry {}",i);
            return Err(Error::BadFormat);
        }
        if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(i)) {
            if fx.name[3]>0x7f || fx.name[4]>0x7f || fx.name[5]>0x7f || fx.name[6]>0x7f {
                debug!("unexpected high bits in file name");
                return Err(Error::BadFormat);
            }
            if fx.get_data_ptr().unwrap() >= MAX_LOGICAL_EXTENTS[cpm_vers[0] as usize - 1] {
                debug!("index of extent too large ({})",fx.get_data_ptr().unwrap());
                return Err(Error::BadFormat);
            }
            if fx.user<USER_END {
                trace!("found file {}:{}",fx.user,fx_to_string_escaped(&fx));
                req_actions += 1;
            }
        }
    }
    // check filenames; allow a few illegal ones
    let mut bad_names = 0;
    for dir_idx in 0..dir.num_entries() {
        if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(dir_idx)) {
            if !is_name_valid(&fx_to_string(&fx)) {
                bad_names += 1;
            }
            if bad_names>3 {
                debug!("found {} bad filenames, aborting",bad_names);
                return Err(Error::BadFormat);
            }
        }
    }
    // Make as many passes as there could be logical extents per file (simple minded sorting).
    // We have to assume the worst case, that only 1 logical extent per extent is utilized.
    // For efficiency, we break out of the loop as soon as all actions are accounted for.
    // This allows for possibility of extent indices out of order - do we need to do this?
    for lx_count in 0..MAX_LOGICAL_EXTENTS[cpm_vers[0] as usize - 1] {
        for dir_idx in 0..dir.num_entries() {
            if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(dir_idx)) {
                if fx.user<USER_END && lx_count==fx.get_data_ptr().unwrap() {
                    let mut pointers: Vec<Ptr> = Vec::new();
                    let fname = fx.user.to_string() + ":" + &fx_to_string_escaped(&fx);
                    trace!("processing extent index {} of file {}",lx_count,fname);
                    if let Some(buf) = ans.get(&fname) {
                        pointers.append(&mut buf.clone());
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
/// is in correspondence with what would be stored in BIOS.
pub struct Disk
{
    cpm_vers: [u8;3],
    dpb: DiskParameterBlock,
    img: Box<dyn img::DiskImage>
}

impl Disk
{
    fn new_fimg(chunk_len: usize) -> super::FileImage {
        super::FileImage {
            fimg_version: super::FileImage::fimg_version(),
            file_system: String::from("cpm"),
            fs_type: vec![0;3],
            aux: vec![],
            eof: vec![0;4],
            created: vec![],
            modified: vec![],
            access: vec![0;11],
            version: vec![],
            min_version: vec![],
            chunk_len,
            chunks: HashMap::new()
        }
    }
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
    pub fn test_img(img: &mut Box<dyn img::DiskImage>,dpb: &DiskParameterBlock,cpm_vers: [u8;3]) -> bool {
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
    fn get_directory(&mut self) -> Directory {
        return get_directory(&mut self.img,&self.dpb).expect("directory broken");
    }
    fn save_directory(&mut self,dir: &Directory) -> STDRESULT {
        let buf = dir.to_bytes();
        for iblock in 0..self.dpb.dir_blocks() {
            self.write_block(&buf,iblock,iblock * self.dpb.block_size())?;
        }
        Ok(())
    }
    fn is_block_free(&self,iblock: usize,directory: &Directory) -> bool {
        if self.dpb.is_reserved(iblock) || iblock >= self.dpb.user_blocks() {
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
        let mut used: usize = self.dpb.reserved_blocks();
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
    fn read_block(&mut self,data: &mut [u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = self.dpb.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        let buf = self.img.read_block(Block::CPM((iblock,self.dpb.bsh,self.dpb.off)))?;
        for i in 0..actual_len {
            data[offset + i] = buf[i];
        }
        Ok(())
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    /// Same as zap since there is no track bitmap in CP/M file system.
    fn write_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        self.zap_block(data,iblock,offset)
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    fn zap_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = self.dpb.block_size() as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        self.img.write_block(Block::CPM((iblock,self.dpb.bsh,self.dpb.off)), &data[offset..offset+actual_len].to_vec())
    }
    fn get_available_block(&mut self,dir: &Directory) -> Option<u16> {
        for block in 0..self.dpb.user_blocks() {
            if self.is_block_free(block,&dir) {
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
    pub fn format(&mut self, vol_name: &str, _time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        if !is_name_valid(vol_name) {
            error!("CP/M volume name invalid");
            return Err(Box::new(Error::BadFormat));
        }
        // Formatting an empty disk is nothing more than filling all user sectors with
        // the deleted file mark.  If we want to put the OS in the reserved tracks
        // we cannot use `write_block` (we need to use blocks with OFF=0).
        for iblock in 0..self.dpb.user_blocks() {
            self.write_block(&vec![DELETED;self.dpb.block_size()],iblock,0)?;
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
    /// Read any file into a file image. Use `FileImage::sequence` to make the result sequential.
    fn read_file(&mut self,name: &str,user: u8) -> Result<super::FileImage,DYNERR> {
        let dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        if let Some(pointers) = self.get_file_metadata(&files,name,user) {
            let mut ans = Disk::new_fimg(self.dpb.block_size());
            let mut buf = vec![0;self.dpb.block_size()];
            let mut block_count = 0;
            let mut prev_lx_count = 0;
            for meta in pointers {
                if let Some(fx) = dir.get_file(&meta) {
                    // For CP/M the access info is encoded in the 8+3 filename.
                    // Furthermore, there is no type beyond the filename extension.
                    // We Therefore store the 8+3 bytes as access, and the 3 bytes as type, all bits being kept.
                    // The following metadata is redundantly extracted from every extent entry
                    ans.fs_type = vec![fx.typ[0],fx.typ[1],fx.typ[2]];
                    ans.access = [fx.name.to_vec(),fx.typ.to_vec()].concat();
                    ans.eof = u32::to_le_bytes(fx.get_eof() as u32).to_vec();
                    // Add any prior holes by adding to the block count, and check ordering
                    let curr_lx_count = fx.get_data_ptr().unwrap() + 1;
                    if curr_lx_count == prev_lx_count {
                        error!("repeated extent index");
                        return Err(Box::new(Error::BadFormat));
                    }
                    let lx_lower_bound = (curr_lx_count - 1) & (usize::MAX ^ self.dpb.exm as usize);
                    if lx_lower_bound < prev_lx_count {
                        panic!("unreachable: extents were not sorted");
                    }
                    block_count += (lx_lower_bound - prev_lx_count) * LOGICAL_EXTENT_SIZE / self.dpb.block_size();
                    // Get the data
                    for iblock in fx.get_block_list(&self.dpb) {
                        if iblock as usize >= self.dpb.user_blocks() {
                            info!("possible extended data disk (not supported)");
                            return Err(Box::new(Error::ReadError));
                        }
                        if iblock>0 {
                            debug!("read block {}",iblock);
                            self.read_block(&mut buf, iblock as usize, 0)?;
                            ans.chunks.insert(block_count,buf.clone());
                        }
                        block_count += 1;
                    }
                    prev_lx_count = curr_lx_count;
                }
            }
            return Ok(ans);
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Used to create extents as a file is being written
    fn open_extent(&self,name: &str,user: u8,fimg: &super::FileImage,dir: &Directory) -> (Ptr,Option<Extent>) {
        // First sort out filename and access
        let (base,typ) = string_to_file_name(name);
        let img_typ = fimg.fs_type.clone();
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
        let mut fx = Extent::new();
        fx.set_name(base,typ);
        fx.set_flags(flgs1.clone().try_into().expect("unreachable"),flgs2.clone().try_into().expect("unreachable"));
        fx.user = user;
        let entry_idx = self.get_available_extent(&dir).unwrap();
        return (Ptr::ExtentEntry(entry_idx),Some(fx));
    }
    /// Update extent data and save to directory buffer
    fn close_extent(&self,entry_ptr: &Ptr,fx: &mut Extent,dir: &mut Directory,lx_count: usize,is_last: bool,fimg: &super::FileImage) {
        trace!("close extent with index {}",lx_count-1);
        trace!("block pointers {:?}",fx.block_list);
        fx.set_data_ptr(Ptr::ExtentData(lx_count-1));
        let eof = super::FileImage::usize_from_truncated_le_bytes(&fimg.eof);
        let mut remainder = eof % self.dpb.extent_capacity();
        if (is_last && eof>0) || (remainder==0 && eof>0) {
            remainder = self.dpb.extent_capacity();
        }
        fx.set_eof(remainder,self.cpm_vers);
        dir.set_file(entry_ptr,fx);
    }
    /// Write any file from a file image.  Use `FileImage::desequence` to convert sequential data.
    fn write_file(&mut self,name: &str, user: u8,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if !is_name_valid(name) {
            error!("invalid CP/M filename");
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
        if pointers!=None {
            error!("overwriting is not allowed");
            return Err(Box::new(Error::FileExists));
        }
        // this is a new file
        // first see if there is enough room for it, if not we abort
        let data_blocks = fimg.chunks.len();
        let block_ptr_slots = fimg.end();
        let slots_per_extent = self.dpb.extent_capacity() / self.dpb.block_size();
        let mut max_extents_needed = block_ptr_slots / slots_per_extent;
        if block_ptr_slots % slots_per_extent > 0 {
            max_extents_needed += 1;
        }
        let mut extents = 0;
        for i in 0..max_extents_needed {
            for slot in i*slots_per_extent..(i+1)*slots_per_extent {
                if fimg.chunks.contains_key(&slot) {
                    extents += 1;
                    break;
                }
            }
        }
        // CP/M allows us to create an empty file; it uses an entry, but no data blocks are allocated.
        // So always ask for 1 extent.
        if extents==0 {
            extents = 1;
        }
        debug!("file requires {} data blocks and {} extents",data_blocks,extents);
        debug!("{} data blocks and {} extents are available",self.num_free_blocks(&dir),self.num_free_extents(&dir));
        if (self.num_free_blocks(&dir) as usize) < data_blocks {
            return Err(Box::new(Error::DiskFull));
        }
        if self.num_free_extents(&dir) < extents {
            return Err(Box::new(Error::DirectoryFull));
        }
        // All checks passed.
        // Save blocks (or mark a hole) creating new extents as needed.
        // Extents that are entirely filled with holes are not created.
        let mut maybe_fx: Option<Extent> = None;
        let mut entry_ptr: Ptr = Ptr::ExtentEntry(0);
        let lx_per_x = self.dpb.exm as usize + 1;
        let slots_per_lx = slots_per_extent / lx_per_x;
        let mut lx_count_tot = 0;
        let mut x_created_count = 0;
        for x in 0..extents {
            // loop over 16K logical extents
            debug!("write physical extent {}",x);
            let mut lx_used_in_x = 0;
            for lx in 0..lx_per_x {
                for loc_slot in 0..slots_per_lx {
                    let glob_slot = x*slots_per_extent + lx*slots_per_lx + loc_slot;
                    if fimg.chunks.contains_key(&glob_slot) {
                        let iblock = self.get_available_block(&dir).unwrap();
                        trace!("map logical extent {} slot {} to block {}",lx,loc_slot,iblock);
                        // if there is no extent yet create it
                        if maybe_fx==None {
                            (entry_ptr,maybe_fx) = self.open_extent(name,user,fimg,&dir);
                            lx_used_in_x += 1;
                        }
                        if let Some(fx) = maybe_fx.as_mut() {
                            fx.set_block_ptr(loc_slot, lx, iblock, &self.dpb);
                            dir.set_file(&entry_ptr, &fx);
                            self.write_block(&fimg.chunks[&glob_slot], iblock as usize, 0)?;
                        }
                    }
                }
            }
            // if not the last extent, consider all logical extents used
            if x+1 < extents {
                lx_used_in_x = lx_per_x;
            }
            debug!("extent used {} logical extents",lx_used_in_x);
            // update totals and save the extent to the directory buffer
            lx_count_tot += lx_used_in_x;
            if let Some(fx) = maybe_fx.as_mut() {
                self.close_extent(&entry_ptr, fx, &mut dir, lx_count_tot, x+1 < extents, &fimg);
                maybe_fx = None;
                x_created_count += 1;
            }
        }
        // if the file is still empty create an empty extent
        if x_created_count==0 {
            (entry_ptr,maybe_fx) = self.open_extent(name,user,fimg,&dir);
            self.close_extent(&entry_ptr,&mut maybe_fx.unwrap(),&mut dir,1,true,&fimg);
        }
        // save the directory changes
        self.save_directory(&dir)?;
        return Ok(0);
    }
    /// Modify a file, optionally rename, retype, change access flags.
    /// The access array has this code: negative=set high bit low, 0=leave high bit alone, positive=set high bit high.
    /// For this function, filenames include the user, as in `0:fname`, `1:fname`, etc.
    fn modify(&mut self,old_xname: &str,maybe_new_xname: Option<&str>,access: [i8;11]) -> STDRESULT {
        let (old_user,old_name) = split_user_filename(old_xname)?;
        if !is_name_valid(&old_name) {
            error!("invalid CP/M filename");
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        let files = build_files(&dir, &self.dpb, self.cpm_vers)?;
        if let Some(pointers) = self.get_file_metadata(&files,&old_name,old_user) {
            // Rename
            if let Some(new_user_and_name) = maybe_new_xname {
                let (new_user,new_name) = split_user_filename(new_user_and_name)?;
                if !is_name_valid(&new_name) {
                    error!("invalid CP/M filename");
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
            self.save_directory(&dir)?;
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self,chunk_len: usize) -> super::FileImage {
        Disk::new_fimg(chunk_len)
    }
    fn catalog_to_stdout(&mut self, _path: &str) -> STDRESULT {
        let dir = self.get_directory();
        for i in 0..dir.num_entries() {
            if let Some(label) = dir.get_label(&Ptr::ExtentEntry(i)) {
                println!();
                println!("{}",lx_to_string_escaped(&label));
            }
        }
        let mut user_count = 0;
        let mut total_count = 0;
        for user in 0..USER_END {
            let mut count = 0;
            let mut file_set: HashSet<Vec<u8>> = HashSet::new();
            for i in 0..dir.num_entries() {
                if let Some(fx) = dir.get_file(&Ptr::ExtentEntry(i)) {
                    if fx.user==user && file_set.insert([vec![user],fx.name.to_vec(),fx.typ.to_vec()].concat()) {
                        if count==0 {
                            user_count += 1;
                            println!();
                            println!("A>USER {}",user);
                            println!("A>DIR");
                        }
                        let full_name = fx_to_string_escaped(&fx);
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
    fn create(&mut self,_path: &str) -> STDRESULT {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn delete(&mut self,xname: &str) -> STDRESULT {
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
            self.save_directory(&dir)?;
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
    fn lock(&mut self,xname: &str) -> STDRESULT {
        // CP/M v2 or higher uses bit 7 of typ[0] for read only
        return self.modify(xname,None,[0,0,0,0,0,0,0,0,1,0,0]);
    }
    fn unlock(&mut self,xname: &str) -> STDRESULT {
        // CP/M v2 or higher uses bit 7 of typ[0] for read only
        return self.modify(xname,None,[0,0,0,0,0,0,0,0,-1,0,0]);
    }
    fn rename(&mut self,old_xname: &str,new_xname: &str) -> STDRESULT {
        return self.modify(old_xname,Some(new_xname),[0;11]);
    }
    fn retype(&mut self,xname: &str,new_type: &str,_sub_type: &str) -> STDRESULT {
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
    fn bload(&mut self,xname: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        match self.read_file(&name,user) {
            Ok(ans) => {
                let eof = super::FileImage::usize_from_truncated_le_bytes(&ans.eof);
                Ok((0,ans.sequence_limited(eof)))
            },
            Err(e) => Err(e)
        }
    }
    fn bsave(&mut self,xname: &str, dat: &[u8],_start_addr: u16,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        let padded = match trailing {
            Some(v) => [dat.to_vec(),v.to_vec()].concat(),
            None => dat.to_vec()
        };
        let mut fimg = self.new_fimg(self.dpb.block_size());
        fimg.desequence(&padded);
        update_fimg_with_name(&mut fimg, &name);
        return self.write_file(&name,user,&fimg);
    }
    fn load(&mut self,_name: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn save(&mut self,_name: &str, _dat: &[u8], _typ: ItemType, _trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn read_text(&mut self,xname: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        match self.read_file(&name,user) {
            Ok(sd) => Ok((0,sd.sequence())),
            Err(e) => Err(e)
        }
    }
    fn write_text(&mut self,xname: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        let mut fimg = self.new_fimg(self.dpb.block_size());
        fimg.desequence(&dat);
        update_fimg_with_name(&mut fimg, &name);
        return self.write_file(&name, user, &fimg);
    }
    fn read_records(&mut self,_name: &str,_record_length: usize) -> Result<super::Records,DYNERR> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn write_records(&mut self,_name: &str, _records: &super::Records) -> Result<usize,DYNERR> {
        error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn read_block(&mut self,num: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                match self.img.read_block(Block::CPM((block,self.dpb.bsh,self.dpb.off))) {
                    Ok(buf) => Ok((0,buf)),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > self.dpb.block_size() {
                    return Err(Box::new(Error::Select));
                }
                match self.img.write_block(Block::CPM((block,self.dpb.bsh,self.dpb.off)), dat) {
                    Ok(()) => Ok(dat.len()),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&mut self,xname: &str) -> Result<super::FileImage,DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        return self.read_file(&name,user);
    }
    fn write_any(&mut self,xname: &str,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        if fimg.file_system!="cpm" {
            error!("cannot write {} file image to cpm",fimg.file_system);
            return Err(Box::new(Error::Select));
        }
        if fimg.chunk_len!=self.dpb.block_size() {
            error!("chunk length {} is incompatible with the DPB for this CP/M",fimg.chunk_len);
            return Err(Box::new(Error::Select));
        }
        return self.write_file(&name,user,fimg);
    }
    fn decode_text(&self,dat: &[u8]) -> Result<String,DYNERR> {
        let file = types::SequentialText::from_bytes(&dat.to_vec());
        Ok(file.to_string())
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,DYNERR> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&mut self,_ref_con: u16) -> HashMap<Block,Vec<usize>> {
        return HashMap::new();
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        for block in 0..self.dpb.user_blocks() {
            let addr = Block::CPM((block,self.dpb.bsh,self.dpb.off));
            let mut actual = self.img.read_block(addr).expect("bad sector access");
            let mut expected = emulator_disk.get_img().read_block(addr).expect("bad sector access");
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