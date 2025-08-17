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
//! Some functions take a CP/M version argument.  This allows for deliberate rejection of CP/M
//! features beyond a given version number.  Setting the version allows for handling of disks
//! up to and including that version.

pub mod types;
mod directory;
mod display;
mod pack;

use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;
use types::*;
use pack::*;
use directory::*;
use super::{Block,Attributes};
use crate::bios::dpb::DiskParameterBlock;
use crate::img;
use crate::fs::FileImage;

use crate::{STDRESULT,DYNERR};
const RCH: &str = "unreachable was reached";

pub const FS_NAME: &str = "cpm";

/// Given a CP/M extended filename string, get the access and fs_type fields
/// to be used in a file image.  These are tied together because of the way
/// CP/M stores the access bits and file type.
fn std_access_and_typ(xname: &str) -> Result<(Vec<u8>,Vec<u8>),DYNERR> {
    let (_,name) = pack::split_user_filename(xname)?;
    let mut access = vec![0x20;11];
    let mut temp_fs_type = vec![0x20;3];
    // assumes is_xname_valid was true; 
    let upper = name.to_uppercase();
    let it: Vec<&str> = upper.split('.').collect();
    let base = it[0].as_bytes().to_vec();
    let ext = match it.len() {
        1 => Vec::new(),
        _ => it[1].as_bytes().to_vec()
    };
    for i in 0..8 {
        if i<base.len() {
            access[i] = base[i];
        } else {
            access[i] = 0x20;
        }
    }
    for i in 0..3 {
        if i<ext.len() {
            access[8+i] = ext[i];
            temp_fs_type[i] = ext[i];
        } else {
            access[8+i] = 0x20;
            temp_fs_type[i] = 0x20;
        }
    }
    let fs_type = vec![temp_fs_type[0],temp_fs_type[1],temp_fs_type[2]];
    Ok((access,fs_type))
}

/// Load directory structure from a borrowed disk image.
/// This is used to test images, as well as being called during FS operations.
fn get_directory(img: &mut Box<dyn img::DiskImage>,dpb: &DiskParameterBlock) -> Option<Directory> {
    if dpb.disk_capacity() != img.byte_capacity() {
        log::debug!("size mismatch: DPB has {}, img has {}",dpb.disk_capacity(),img.byte_capacity());
        return None;
    } else {
        log::debug!("size matched: DPB and img both have {}",dpb.disk_capacity());
    }
    let mut buf: Vec<u8> = Vec::new();
    for iblock in 0..dpb.dir_blocks() {
        match img.read_block(Block::CPM((iblock,dpb.bsh,dpb.off))) { Ok(dat) => {
            buf.append(&mut dat.clone());
        } _ => {
            log::debug!("cannot read CP/M block {}",iblock);
            return None;
        }}
    }
    let buf_size = dpb.dir_entries() * DIR_ENTRY_SIZE;
    Some(Directory::from_bytes(&buf[0..buf_size]).expect(RCH))
}

pub fn new_fimg(chunk_len: usize,set_time: bool,xname: &str) -> Result<FileImage,DYNERR> {
    if !is_xname_valid(xname) {
        return Err(Box::new(Error::BadFormat))
    }
    let created = match set_time {
        true => pack::pack_date(None).to_vec(),
        false => vec![]
    };
    let (access,fs_type) = std_access_and_typ(xname)?;
    Ok(FileImage {
        fimg_version: FileImage::fimg_version(),
        file_system: String::from(FS_NAME),
        fs_type,
        aux: vec![],
        eof: vec![0;4],
        accessed: created.clone(),
        created,
        modified: vec![],
        access,
        version: vec![],
        min_version: vec![],
        chunk_len,
        full_path: xname.to_string(),
        chunks: HashMap::new()
    })
}

pub struct Packer {
}

/// The primary interface for disk operations.
/// The "Disk Parameter Block" that is provided upon creation
/// should be in correspondence with DRI specifications.
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
    pub fn from_img(img: Box<dyn img::DiskImage>,dpb: DiskParameterBlock,cpm_vers: [u8;3]) -> Result<Self,DYNERR> {
        if !dpb.verify() {
            return Err(Box::new(Error::BadFormat));
        }
        Ok(Self {
            cpm_vers,
            dpb,
            img
        })
    }
    /// Test an image for the CP/M file system.
    /// Will not accept images with directory structures corresponding to CP/M versions higher than `cpm_vers`.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>,dpb: &DiskParameterBlock,cpm_vers: [u8;3]) -> bool {
        // test the volume directory header
        if let Some(directory) = get_directory(img,dpb) {
            if let Err(_e) = directory.build_files(dpb,cpm_vers) {
                log::debug!("Unable to build CP/M file directory");
                return false;
            }
            return true;
        }
        log::debug!("CP/M directory was not readable");
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
            if let Some(fx) = directory.get_entry::<Extent>(&Ptr::ExtentEntry(idx)) {
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
            if let Some(fx) = dir.get_entry::<Extent>(&Ptr::ExtentEntry(idx)) {
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
            EntryType::Deleted | EntryType::Unknown => true,
            _ => false
        }
    }
    /// extents available in the directory, each can reference up to (EXM+1)*16K of data
    fn num_free_extents(&self,dir: &Directory) -> usize {
        let mut ans: usize = 0;
        for i in 0..dir.num_entries() {
            log::trace!("check entry {}",i);
            match dir.get_type(&Ptr::ExtentEntry(i)) {
                EntryType::Deleted | EntryType::Unknown => {
                    ans += 1
                },
                _ => { log::debug!("entry {} is used",i); }
            }
        }
        log::debug!("found {} free extents",ans);
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
        self.img.write_block(Block::CPM((iblock,self.dpb.bsh,self.dpb.off)), &data[offset..offset+actual_len])
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
    /// Format disk for the CP/M file system.
    /// If CP/M version is 2.x, arguments are ignored.
    /// If CP/M version is 3.x:
    /// * `vol_name.len()>0` causes creation of a label with the specified name
    /// * `time.is_some()` causes creation of a label (maybe default name) and timestamps
    pub fn format(&mut self, vol_name: &str, time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        if self.cpm_vers[0] >= 3 && vol_name.len()>0 && !is_name_valid(vol_name) {
            log::error!("CP/M volume name invalid");
            return Err(Box::new(Error::BadFormat));
        }
        // For CP/M 2, formatting is nothing more than filling all user sectors with
        // the deleted file mark.  If we want to put the OS in the reserved tracks
        // we cannot use `write_block` (we need to use blocks with OFF=0).
        for iblock in 0..self.dpb.user_blocks() {
            self.write_block(&vec![DELETED;self.dpb.block_size()],iblock,0)?;
        }
        if self.cpm_vers[0] >= 3 {
            let mut lab = Label::create();
            if vol_name.len() > 0 {
                let (name,typ) = string_to_file_name(vol_name);
                lab.set(name,typ);
            }
            lab.set_timestamp_for_label(time, time);
            if time.is_some() {
                lab.timestamp_creation(true);
                lab.timestamp_update(true);
            }
            let mut dir = self.get_directory();
            if vol_name.len() > 0 || time.is_some() {
                dir.set_entry::<Label>(&Ptr::ExtentEntry(0), &lab);
            }
           let final_dir = match time.is_some() {
                true => dir.add_timestamps()?,
                false => dir
            };
            self.save_directory(&final_dir)?;
        }
        return Ok(());
    }
    /// Read any file into a file image. Use `FileImage::sequence` to make the result sequential.
    fn read_file(&mut self,xname: &str) -> Result<FileImage,DYNERR> {
        log::trace!("attempt to read {}",xname);
        let mut dir = self.get_directory();
        let files = dir.build_files(&self.dpb,self.cpm_vers)?;
        if let Some(finfo) = get_file(xname,&files) {
            let pointers: Vec<&Ptr> = finfo.entries.values().collect();
            let mut ans = new_fimg(self.dpb.block_size(),false,xname)?;
            let mut buf = vec![0;self.dpb.block_size()];
            let mut block_count = 0;
            let mut prev_lx_count = 0;
            for meta in &pointers {
                if let Some(fx) = dir.get_entry::<Extent>(&meta) {
                    // For CP/M the access info is encoded in the 8+3 filename.
                    // Furthermore, there is no type beyond the filename extension.
                    // We Therefore store the 8+3 bytes as access, and the 3 bytes as type, all bits being kept.
                    // The following metadata is redundantly extracted from every extent entry
                    ans.fs_type = fx.get_name_and_flags()[8..11].to_vec();
                    ans.access = fx.get_name_and_flags().to_vec();
                    ans.eof = u32::to_le_bytes(fx.get_eof() as u32).to_vec();
                    // For CP/M access and create are mutually exclusive, so use created for either
                    if let Some(access_time) = finfo.access_time {
                        ans.created = access_time.to_vec();
                    }
                    if let Some(create_time) = finfo.create_time {
                        ans.created = create_time.to_vec();
                    }
                    if let Some(update_time) = finfo.update_time {
                        ans.modified = update_time.to_vec();
                    }
                    // Add any prior holes by adding to the block count, and check ordering
                    let curr_lx_count = fx.get_data_ptr().unwrap() + 1;
                    if curr_lx_count == prev_lx_count {
                        log::error!("repeated extent index");
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
                            log::info!("possible extended data disk (not supported)");
                            return Err(Box::new(Error::ReadError));
                        }
                        if iblock>0 {
                            log::debug!("read block {}",iblock);
                            self.read_block(&mut buf, iblock as usize, 0)?;
                            ans.chunks.insert(block_count,buf.clone());
                        }
                        block_count += 1;
                    }
                    prev_lx_count = curr_lx_count;
                }
            }
            // update the timestamp if applicable
            if let (Some(lab),Some(lx0)) = (dir.find_label(),pointers.get(0)) {
                Timestamp::maybe_set_access(&mut dir, &lab, &lx0, None)?;
                self.save_directory(&dir)?;
            }
            return Ok(ans);
        }
        return Err(Box::new(Error::FileNotFound));
    }
    /// Used to create extents as a file is being written
    fn open_extent(&self,name: &str,user: u8,fimg: &FileImage,dir: &Directory,first: &mut Option<Ptr>) -> (Ptr,Option<Extent>) {
        // First sort out filename and access
        let (base,typ) = string_to_file_name(name);
        let img_typ = fimg.fs_type.clone();
        if typ[0]!=img_typ[0] || typ[1]!=img_typ[1] || typ[2]!=img_typ[2] {
            log::warn!("CP/M file image type and extension are inconsistent");
        }
        let (flgs1,flgs2) = match fimg.access.len() {
            11 => (fimg.access[0..8].to_vec(),fimg.access[8..11].to_vec()),
            _ => {
                log::warn!("CP/M file image has bad access field (ignoring)");
                (vec![0;8],vec![0;3])
            }
        };
        let mut fx = Extent::new();
        fx.set_name(base,typ);
        fx.set_flags(flgs1.clone().try_into().expect(RCH),flgs2.clone().try_into().expect(RCH));
        fx.user = user;
        let entry_idx = self.get_available_extent(&dir).unwrap();
        if first.is_none() {
            *first = Some(Ptr::ExtentEntry(entry_idx));
        }
        return (Ptr::ExtentEntry(entry_idx),Some(fx));
    }
    /// Update extent data and save to directory buffer
    fn close_extent(&self,entry_ptr: &Ptr,fx: &mut Extent,dir: &mut Directory,lx_count: usize,is_last: bool,fimg: &FileImage) {
        log::trace!("close extent with index {}",lx_count-1);
        log::trace!("block pointers {:?}",fx.block_list);
        fx.set_data_ptr(Ptr::ExtentData(lx_count-1));
        let eof = fimg.get_eof();
        let mut remainder = eof % self.dpb.extent_capacity();
        if (!is_last && eof>0) || (remainder==0 && eof>0) {
            remainder = self.dpb.extent_capacity();
        }
        fx.set_eof(remainder,self.cpm_vers);
        dir.set_entry(entry_ptr,fx);
    }
    /// Write any file from a file image.  Use `FileImage::desequence` to convert sequential data.
    fn write_file(&mut self,xname: &str,fimg: &FileImage) -> Result<usize,DYNERR> {
        let (user,name) = split_user_filename(xname)?;
        if !is_name_valid(&name) {
            log::error!("invalid CP/M filename");
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        let files = dir.build_files(&self.dpb,self.cpm_vers)?;
        if get_file(xname,&files).is_some() {
            log::error!("overwriting is not allowed");
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
        log::debug!("file requires {} data blocks and {} extents",data_blocks,extents);
        log::debug!("{} data blocks and {} extents are available",self.num_free_blocks(&dir),self.num_free_extents(&dir));
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
        let mut maybe_entry1: Option<Ptr> = None;
        for x in 0..extents {
            // loop over 16K logical extents
            log::debug!("write physical extent {}",x);
            let mut lx_used_in_x = 0;
            for lx in 0..lx_per_x {
                for loc_slot in 0..slots_per_lx {
                    let glob_slot = x*slots_per_extent + lx*slots_per_lx + loc_slot;
                    if fimg.chunks.contains_key(&glob_slot) {
                        let iblock = self.get_available_block(&dir).unwrap();
                        log::trace!("map logical extent {} slot {} to block {}",lx,loc_slot,iblock);
                        // if there is no extent yet create it
                        if maybe_fx==None {
                            (entry_ptr,maybe_fx) = self.open_extent(&name,user,fimg,&dir,&mut maybe_entry1);
                            lx_used_in_x += 1;
                        }
                        if let Some(fx) = maybe_fx.as_mut() {
                            fx.set_block_ptr(loc_slot, lx, iblock, &self.dpb);
                            dir.set_entry(&entry_ptr, fx);
                            self.write_block(&fimg.chunks[&glob_slot], iblock as usize, 0)?;
                        }
                    }
                }
            }
            // if not the last extent, consider all logical extents used
            if x+1 < extents {
                lx_used_in_x = lx_per_x;
            }
            log::debug!("extent used {} logical extents",lx_used_in_x);
            // update totals and save the extent to the directory buffer
            lx_count_tot += lx_used_in_x;
            if let Some(fx) = maybe_fx.as_mut() {
                self.close_extent(&entry_ptr, fx, &mut dir, lx_count_tot, x+1 == extents, &fimg);
                maybe_fx = None;
                x_created_count += 1;
            }
        }
        // if the file is still empty create an empty extent
        if x_created_count==0 {
            (entry_ptr,maybe_fx) = self.open_extent(&name,user,fimg,&dir,&mut maybe_entry1);
            self.close_extent(&entry_ptr,&mut maybe_fx.unwrap(),&mut dir,1,true,&fimg);
        }
        // update the timestamp if applicable
        if let (Some(lab),Some(lx0)) = (dir.find_label(),maybe_entry1) {
            Timestamp::maybe_set_create(&mut dir, &lab, &lx0, None)?;
        }
        // save the directory changes
        self.save_directory(&dir)?;
        return Ok(0);
    }
    /// Modify a file, optionally rename, retype, change access flags.
    /// The access array has this code: negative=set high bit low, 0=leave high bit alone, positive=set high bit high.
    /// For this function, filenames include the user, as in `0:fname`, `1:fname`, etc.
    fn modify(&mut self,old_xname: &str,maybe_new_xname: Option<&str>,access: [i8;11]) -> STDRESULT {
        let (_,old_name) = split_user_filename(old_xname)?;
        if !is_name_valid(&old_name) {
            log::error!("invalid CP/M filename");
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        let files = dir.build_files(&self.dpb,self.cpm_vers)?;
        if let Some(finfo) = get_file(old_xname,&files) {
            // Rename
            if let Some(new_xname) = maybe_new_xname {
                let (new_user,new_name) = split_user_filename(new_xname)?;
                if !is_name_valid(&new_name) {
                    log::error!("invalid CP/M filename");
                    return Err(Box::new(Error::BadFormat));
                }
                log::debug!("renaming to {}, user {}",new_name,new_user);
                if get_file(new_xname,&files).is_none() {
                    for entry in finfo.entries.values() {
                        if let Some(mut fx) = dir.get_entry::<Extent>(&entry) {
                            if fx.get_flags()[8]>0 {
                                log::error!("{} is read only, unlock first",old_name);
                                return Err(Box::new(Error::FileReadOnly));
                            }
                            let (base,typ) = string_to_file_name(&new_name);
                            fx.user = new_user;
                            fx.set_name(base,typ);
                            dir.set_entry(&entry,&fx);
                        }
                    }
                } else {
                    return Err(Box::new(Error::FileExists));
                }
            }
            // Change access
            for entry in finfo.entries.values() {
                if let Some(mut fx) = dir.get_entry::<Extent>(&entry) {
                    let curr_flags = fx.get_flags();
                    let mut new_flags: [u8;11] = [0;11];
                    for i in 0..11 {
                        new_flags[i] = match access[i] {
                            x if x<0 => 0,
                            x if x>0 => 0x80,
                            _ => curr_flags[i]
                        };
                    }
                    fx.set_flags(new_flags[0..8].try_into().expect(RCH),new_flags[8..11].try_into().expect(RCH));
                    dir.set_entry(&entry,&fx);
                }
            }
            if let (Some(lab),Some(lx0)) = (dir.find_label(),finfo.entries.values().next()) {
                Timestamp::maybe_set_update(&mut dir, &lab, &lx0, None)?;
            }
            self.save_directory(&dir)?;
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
    fn protect(&mut self,xname: &str,permissions: Attributes,password: &str) -> STDRESULT {
        if password.len()==0 || !is_password_valid(password) {
            log::error!("password is invalid");
            return Err(Box::new(Error::BadFormat));
        }
        let mut dir = self.get_directory();
        if dir.find_label().is_none() {
            log::error!("no label on this disk, cannot protect");
            return Err(Box::new(Error::BadFormat));
        }
        let files = dir.build_files(&self.dpb,self.cpm_vers)?;
        if let Some(_) = get_file(xname,&files) {
            let (user,name_string) = split_user_filename(xname)?;
            let (name,typ) = string_to_file_name(&name_string);
            // first try updating an existing one
            for i in 0..dir.num_entries() {
                if let Some(mut px) = dir.get_entry::<Password>(&Ptr::ExtentEntry(i)) {
                    if px.user==user+16 && px.name==name && px.typ==typ {
                        px.merge(password,permissions);
                        dir.set_entry::<Password>(&Ptr::ExtentEntry(i), &px);
                        return self.save_directory(&dir);
                    }
                }
            }
            // if we are still here we need to make a new one
            let mut completion: i32 = 0;
            let new_px = Password::create(password, user, &name_string, permissions);
            for i in 0..dir.num_entries() {
                if let Some(mut lab) = dir.get_entry::<Label>(&Ptr::ExtentEntry(i)) {
                    // TODO: is this protecting the label, or enabling file protection?
                    lab.protect(true);
                    dir.set_entry::<Label>(&Ptr::ExtentEntry(i), &lab);
                    completion += 1;
                }
                if dir.get_type(&Ptr::ExtentEntry(i))==EntryType::Deleted {
                    dir.set_entry::<Password>(&Ptr::ExtentEntry(i), &new_px);
                    completion += 1;
                }
                if completion==2 {
                    return self.save_directory(&dir);
                }
            }
            return Err(Box::new(Error::DirectoryFull));
        }
        return Err(Box::new(Error::FileNotFound));        
    }
    fn unprotect(&mut self,xname: &str) -> STDRESULT {
        let mut found = false;
        let mut dir = self.get_directory();
        log::debug!("removing password for {}",xname);
        let (user,name_string) = split_user_filename(xname)?;
        let (name,typ) = string_to_file_name(&name_string);
        for i in 0..dir.num_entries() {
            if let Some(mut px) = dir.get_entry::<Password>(&Ptr::ExtentEntry(i)) {
                if px.user==user+16 && px.name==name && px.typ==typ {
                    px.user = DELETED;
                    dir.set_entry::<Password>(&Ptr::ExtentEntry(i), &px);
                    found = true;
                }
            }
        }
        match found {
            true => self.save_directory(&dir),
            false => Err(Box::new(Error::FileNotFound))
        }
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self, chunk_len: Option<usize>,set_time: bool,path: &str) -> Result<FileImage,DYNERR> {
        match chunk_len {
            Some(l) => new_fimg(l,set_time,path),
            None => new_fimg(self.dpb.block_size(),set_time,path)
        }
    }
    fn stat(&mut self) -> Result<super::Stat,DYNERR> {
        let dir = &self.get_directory();
        Ok(super::Stat {
            fs_name: FS_NAME.to_string(),
            label: match dir.find_label() {
                Some(label) => {
                    let (mut res,typ) = label.get_split_string();
                    if typ.len()>0 {
                        res += ".";
                        res += &typ;
                    }
                    res
                },
                None => "".to_string()
            },
            users: dir.get_users().iter().map(|u| u.to_string()).collect(),
            block_size: self.dpb.block_size(),
            block_beg: 0,
            block_end: self.dpb.user_blocks(),
            free_blocks: self.num_free_blocks(dir) as usize,
            raw: self.dpb.to_json(Some(1))
        })
    }
    fn catalog_to_stdout(&mut self, opt: &str) -> STDRESULT {
        let dir = self.get_directory();
        match opt {
            "/" => display::dir(&dir,&self.dpb,""),
            _ => display::dir(&dir,&self.dpb,opt)
        }
    }
    fn catalog_to_vec(&mut self, path: &str) -> Result<Vec<String>,DYNERR> {
        if path!="/" && path!="" {
            return Err(Box::new(Error::FileNotFound));
        }
        let dir = self.get_directory();
        match dir.build_files(&self.dpb, [3,0,0]) {
            Ok(files) => {
                let mut ans = Vec::new();
                let mut multi_user = false;
                for (_,info) in &files {
                    if info.user != 0 {
                        multi_user = true;
                    }
                }
                for (name,info) in files {
                    match multi_user {
                        true => ans.push(super::universal_row(&info.typ,info.blocks_allocated,&name)),
                        false => ans.push(super::universal_row(&info.typ, info.blocks_allocated, &info.name))
                    }
                }
                Ok(ans)
            },
            Err(e) => Err(e)
        }
    }
    fn glob(&mut self,pattern: &str,case_sensitive: bool) -> Result<Vec<String>,DYNERR> {
        let mut ans = Vec::new();
        let glob = match case_sensitive {
            true => globset::Glob::new(pattern)?.compile_matcher(),
            false => globset::Glob::new(&pattern.to_uppercase())?.compile_matcher()
        };
        let dir = self.get_directory();
        let files = dir.build_files(&self.dpb, self.cpm_vers)?;
        for (name,_info) in files {
            let name = match case_sensitive {
                true => name.clone(),
                false => name.to_uppercase()
            };
            if glob.is_match(&name) {
                ans.push(name);
            }
        }
        Ok(ans)
    }
    fn tree(&mut self,include_meta: bool,indent: Option<u16>) -> Result<String,DYNERR> {
        let dir = self.get_directory();
        display::tree(&dir,&self.dpb,include_meta,indent)
    }
    fn create(&mut self,_path: &str) -> STDRESULT {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
    fn delete(&mut self,xname: &str) -> STDRESULT {
        let mut dir = self.get_directory();
        let files = dir.build_files(&self.dpb,self.cpm_vers)?;
        if let Some(finfo) = get_file(xname,&files) {
            let pointers: Vec<&Ptr> = finfo.entries.values().collect();
            for ptr in &pointers {
                if let Some(mut fx) = dir.get_entry::<Extent>(ptr) {
                    if fx.get_flags()[8]>0 {
                        log::error!("{} is read only, unlock first",xname);
                        return Err(Box::new(Error::FileReadOnly));
                    }
                    fx.user = DELETED;
                    dir.set_entry(ptr,&fx);
                }
            }
            self.save_directory(&dir)?;
            return Ok(());
        } else {
            return Err(Box::new(Error::FileNotFound));
        }
    }
    fn set_attrib(&mut self,xname: &str,permissions: Attributes,maybe_password: Option<&str>) -> STDRESULT {
        if let Some(password) = maybe_password {
            if password.len() == 0 {
                return self.unprotect(xname);
            } else {
                return self.protect(xname,permissions,password);
            }
        }
        match permissions.write {
            Some(false) => self.modify(xname,None,[0,0,0,0,0,0,0,0,1,0,0]),
            Some(true) => self.modify(xname,None,[0,0,0,0,0,0,0,0,-1,0,0]),
            None => {
                log::warn!("set access resulted in no changes");
                Ok(())
            }
        }
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
            log::error!("new type must be `dir` or `sys`");
            return Err(Box::new(Error::Select));
        }
    }
    fn read_block(&mut self,num: &str) -> Result<Vec<u8>,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => self.img.read_block(Block::CPM((block,self.dpb.bsh,self.dpb.off))),
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
    fn get(&mut self,xname: &str) -> Result<FileImage,DYNERR> {
        self.read_file(xname)
    }
    fn put(&mut self,fimg: &FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!=FS_NAME {
            log::error!("cannot write {} file image to cpm",fimg.file_system);
            return Err(Box::new(Error::Select));
        }
        if fimg.chunk_len!=self.dpb.block_size() {
            log::error!("chunk length {} is incompatible with the DPB for this CP/M",fimg.chunk_len);
            return Err(Box::new(Error::Select));
        }
        self.write_file(&fimg.full_path,fimg)
    }
    fn standardize(&mut self,_ref_con: u16) -> HashMap<Block,Vec<usize>> {
        // TODO: this is rather specialized for the particular test that uses it
        let mut ans: HashMap<Block,Vec<usize>> = HashMap::new();
        let mut dat: Vec<u8> = vec![0;self.dpb.block_size()];
        for block in 0..self.dpb.dir_blocks() {
            self.read_block(&mut dat, block, 0).expect("failed to read block");
            for i in (0..self.dpb.block_size()).step_by(32) {
                if dat[i]==TIMESTAMP {
                    super::add_ignorable_offsets(&mut ans, Block::CPM((block,self.dpb.bsh,self.dpb.off)), 
                        (i+11..i+19).collect());
                }
            }
        }
        return ans;
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