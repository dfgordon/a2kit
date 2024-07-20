//! ## ProDOS file system module
//! 
//! This manipulates disk images containing one ProDOS volume.
//! 
//! * Single volume images only

mod boot;
pub mod types;
mod directory;

use std::collections::HashMap;
use a2kit_macro::DiskStruct;
use std::str::FromStr;
use std::fmt::Write;
use colored::*;
use log::{trace,debug,error};
use types::*;
use directory::*;
use super::{Block,TextEncoder};
use crate::lang::applesoft;
use crate::img;
use crate::commands::ItemType;
use crate::{DYNERR,STDRESULT};

pub const FS_NAME: &str = "prodos";

/// The primary interface for disk operations.
pub struct Disk {
    img: Box<dyn img::DiskImage>,
    total_blocks: usize,
    maybe_bitmap: Option<Vec<u8>>,
    bitmap_blocks: Vec<usize>
}

/// put a u16 into an index block in the prescribed fashion
fn pack_index_ptr(buf: &mut [u8],ptr: u16,idx: usize) {
    let bytes = u16::to_le_bytes(ptr);
    buf[idx] = bytes[0];
    buf[idx+256] = bytes[1];
}

impl Disk {
    fn new_fimg(chunk_len: usize) -> super::FileImage {
        super::FileImage {
            fimg_version: super::FileImage::fimg_version(),
            file_system: String::from(FS_NAME),
            fs_type: vec![0],
            aux: vec![0;2],
            eof: vec![0;3],
            created: vec![0;4],
            modified: vec![0;4],
            access: vec![0],
            version: vec![0],
            min_version: vec![0],
            chunk_len,
            chunks: HashMap::new()
        }
    }
    /// Use the given image as storage for a new DiskFS.
    /// The DiskFS takes ownership of the image.
    /// The image may or may not be formatted.
    pub fn from_img(img: Box<dyn img::DiskImage>) -> Self {
        let total_blocks = img.byte_capacity()/512;
        Self {
            img,
            total_blocks,
            // bitmap buffer is designed to work transparently
            maybe_bitmap: None,
            bitmap_blocks: Vec::new()
        }
    }
    /// Test an image for the ProDOS file system.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>) -> bool {
        // test the volume directory header to see if this is ProDOS
        if let Ok(buf) = img.read_block(Block::PO(2)) {
            let first_char_patt = "ABCDEFGHIJKLMNOPQRSTUVWXYZ.";
            let char_patt = [first_char_patt,"0123456789"].concat();
            let vol_key: KeyBlock<VolDirHeader> = KeyBlock::from_bytes(&buf);
            let (nibs,name) = vol_key.header.fname();
            let total_blocks = u16::from_le_bytes([buf[0x29],buf[0x2A]]);
            if total_blocks<280 {
                debug!("peculiar block count {}",total_blocks);
                return false;
            }
            if buf[0x23]!=0x27 || (buf[0x24]!=0x0D && buf[0x24]!=0x0C) {
                debug!("unexpected header bytes {}, {}",buf[0x23],buf[0x24]);
                return false;
            }
            if vol_key.prev()!=0 || vol_key.next()!=3 || (nibs >> 4)!=15 {
                debug!("unexpected volume name length or links");
                return false;
            }
            if !first_char_patt.contains(name[0] as char) {
                debug!("volume name unexpected character");
                return false;
            }
            for i in 1..(nibs & 0x0F) {
                if !char_patt.contains(name[i as usize] as char) {
                    debug!("volume name unexpected character");
                    return false;
                }
            }
            return true;
        }
        debug!("ProDOS volume directory was not readable");
        return false;
    }
    /// Open buffer if not already present.  Will usually be called indirectly.
    fn open_bitmap_buffer(&mut self) -> STDRESULT {
        if self.maybe_bitmap==None {
            self.bitmap_blocks = Vec::new();
            let bitmap_block_count = 1 + self.total_blocks / 4096;
            let mut ans = Vec::new();
            let bptr = u16::from_le_bytes(self.get_vol_header()?.bitmap_ptr) as usize;
            for iblock in bptr..bptr+bitmap_block_count {
                let mut buf = [0;512].to_vec();
                self.read_block(&mut buf,iblock,0)?;
                ans.append(&mut buf);
                self.bitmap_blocks.push(iblock);
            }
            self.maybe_bitmap = Some(ans);
        }
        Ok(())
    }
    /// Get the buffer, if it doesn't exist it will be opened.
    fn get_bitmap_buffer(&mut self) -> Result<&mut Vec<u8>,DYNERR> {
        self.open_bitmap_buffer()?;
        if let Some(buf) = self.maybe_bitmap.as_mut() {
            return Ok(buf);
        }
        panic!("bitmap buffer failed to open");
    }
    /// Buffer needs to be written back when an external caller
    /// asks, directly or indirectly, for the underlying image.
    fn writeback_bitmap_buffer(&mut self) -> STDRESULT {
        let buf = match self.maybe_bitmap.as_ref() {
            Some(bitmap) => bitmap.clone(),
            None => return Ok(())
        };
        if self.bitmap_blocks.len()>0 {
            let first = self.bitmap_blocks[0];
            let bitmap_block_count = 1 + self.total_blocks / 4096;
            for iblock in first..first+bitmap_block_count {
                self.zap_block(&buf,iblock,(iblock-first)*512)?;
            }    
        }
        Ok(())
    }
    fn allocate_block(&mut self,iblock: usize) -> STDRESULT {
        let buf = self.get_bitmap_buffer()?;
        let byte = iblock / 8;
        let bit = 7 - iblock % 8;
        buf[byte] &= (1 << bit as u8) ^ u8::MAX;
        Ok(())
    }
    fn deallocate_block(&mut self,iblock: usize) -> STDRESULT {
        let buf = self.get_bitmap_buffer()?;
        let byte = iblock / 8;
        let bit = 7 - iblock % 8;
        buf[byte] |= 1 << bit as u8;
        Ok(())
    }
    fn is_block_free(&mut self,iblock: usize) -> Result<bool,DYNERR> {
        let buf = self.get_bitmap_buffer()?;
        let byte = iblock / 8;
        let bit = 7 - iblock % 8;
        Ok((buf[byte] & (1 << bit as u8)) > 0)
    }
    fn num_free_blocks(&mut self) -> Result<u16,DYNERR> {
        let mut free: u16 = 0;
        for i in 0..self.total_blocks {
            if self.is_block_free(i)? {
                free += 1;
            }
        }
        Ok(free)
    }
    /// Read a block; if it is a bitmap block get it from the buffer.
    fn read_block(&mut self,data: &mut [u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x,
            _ => bytes
        };
        if self.bitmap_blocks.contains(&iblock) {
            let first = self.bitmap_blocks[0];
            let buf = self.get_bitmap_buffer()?;
            for i in 0..actual_len as usize {
                data[offset + i] = buf[(iblock-first)*512 + i];
            }
            return Ok(());
        }
        match self.img.read_block(Block::PO(iblock)) {
            Ok(buf) => {
                for i in 0..actual_len as usize {
                    data[offset + i] = buf[i];
                }
                Ok(())
            }
            Err(e) => Err(e)
        }
    }
    /// Write and allocate the block in one step.
    /// If it is a bitmap block panic; we should only be zapping bitmap blocks.
    fn write_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        if self.bitmap_blocks.contains(&iblock) {
            panic!("attempt to write bitmap block, zap it instead");
        }
        self.zap_block(data,iblock,offset)?;
        self.allocate_block(iblock)
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    fn zap_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        if self.bitmap_blocks.contains(&iblock) {
            self.maybe_bitmap = None;
        }
        self.img.write_block(Block::PO(iblock), &data[offset..offset+actual_len].to_vec())
    }
    fn get_available_block(&mut self) -> Result<Option<u16>,DYNERR> {
        for block in 0..self.total_blocks {
            if self.is_block_free(block)? {
                return Ok(Some(block as u16));
            }
        }
        return Ok(None);
    }
    /// Format a disk with the ProDOS file system
    pub fn format(&mut self, vol_name: &str, floppy: bool, time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        // make sure we start with all 0
        trace!("formatting: zero all");
        for iblock in 0..self.total_blocks {
            self.zap_block(&[0;512].to_vec(),iblock,0)?;
        }
        // calculate volume parameters and setup volume directory
        let mut volume_dir = KeyBlock::<VolDirHeader>::new();
        let bitmap_blocks = 1 + self.total_blocks / 4096;
        volume_dir.set_links(Some(0), Some(VOL_KEY_BLOCK+1));
        volume_dir.header.format(self.total_blocks as u16,vol_name,time);
        let first = u16::from_le_bytes(volume_dir.header.bitmap_ptr) as usize;

        // zap in the volume key block
        trace!("formatting: volume key");
        self.zap_block(&volume_dir.to_bytes(),VOL_KEY_BLOCK as usize,0)?;

        // mark all blocks as free
        trace!("formatting: free all");
        for b in 0..self.total_blocks {
            self.deallocate_block(b)?;
        }

        // mark volume key and bitmap blocks as used
        trace!("formatting: allocate key and bitmap");
        self.allocate_block(VOL_KEY_BLOCK as usize)?;
        for b in first..first + bitmap_blocks {
            self.allocate_block(b)?;
        }
        
        // boot loader blocks
        trace!("formatting: boot loader");
        if floppy {
            self.write_block(&boot::FLOPPY_BLOCK0.to_vec(),0,0)?;
        }
        else {
            self.write_block(&boot::HD_BLOCK0.to_vec(), 0, 0)?;
        }
        self.write_block(&vec![0;512],1,0)?;

        // next 3 volume directory blocks
        trace!("formatting: volume directory");
        for b in 3..6 {
            let mut this = EntryBlock::new();
            if b==5 {
                this.set_links(Some(b-1), Some(0));
            } else {
                this.set_links(Some(b-1), Some(b+1));
            }
            self.write_block(&this.to_bytes(),b as usize,0)?;
        }
        Ok(())
    }
    fn get_vol_header(&mut self) -> Result<VolDirHeader,DYNERR> {
        let mut buf: Vec<u8> = vec![0;512];
        self.read_block(&mut buf,VOL_KEY_BLOCK as usize,0)?;
        let volume_dir = KeyBlock::<VolDirHeader>::from_bytes(&buf);
        Ok(volume_dir.header)
    }
    /// Return the correct trait object assuming this block is a directory block.
    /// May return a key block or an entry block.
    fn get_directory(&mut self,iblock: usize) -> Result<Box<dyn Directory>,DYNERR> {
        let mut buf: Vec<u8> = vec![0;512];
        self.read_block(&mut buf,iblock,0)?;
        match (iblock==VOL_KEY_BLOCK as usize,buf[0]==0 && buf[1]==0) {
            (true,true) => Ok(Box::new(KeyBlock::<VolDirHeader>::from_bytes(&buf))),
            (true,false) => Ok(Box::new(KeyBlock::<VolDirHeader>::from_bytes(&buf))),
            (false,true) => Ok(Box::new(KeyBlock::<SubDirHeader>::from_bytes(&buf))),
            (false,false) => Ok(Box::new(EntryBlock::from_bytes(&buf)))
        }
    }
    /// Find the key block assuming this block is a directory block, and return the
    /// block pointer and corresponding trait object in a tuple.
    fn get_key_directory(&mut self,ptr: u16) -> Result<(u16,Box<dyn Directory>),DYNERR> {
        let mut curr = ptr;
        for _try in 0..100 {
            let test_dir = self.get_directory(curr as usize)?;
            if test_dir.prev()==0 {
                return Ok((curr,test_dir));
            }
            curr = test_dir.prev();
        }
        error!("directory block count not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Given an entry location get the entry from disk
    fn read_entry(&mut self,loc: &EntryLocation) -> Result<Entry,DYNERR> {
        let dir = self.get_directory(loc.block as usize)?;
        Ok(dir.get_entry(loc))
    }
    /// Given a modified entry and location, write the change to disk.
    /// Any other unsaved changes in the block are lost.  Maybe this should go away.
    fn write_entry(&mut self,loc: &EntryLocation,entry: &Entry) -> STDRESULT {
        let mut dir = self.get_directory(loc.block as usize)?;
        dir.set_entry(loc,*entry);
        let buf = dir.to_bytes();
        self.write_block(&buf,loc.block as usize,0)
    }
    /// Try to add another entry block to the directory with the given parent entry.
    /// If successful return the location of the first entry in the new block.
    /// This is called when the directory runs out of entries.
    fn expand_directory(&mut self, parent_loc: &EntryLocation) -> Result<EntryLocation,DYNERR> {
        let mut entry = self.read_entry(&parent_loc)?;
        if entry.storage_type()!=StorageType::SubDirEntry {
            return Err(Box::new(Error::FileTypeMismatch));
        }
        let mut curr = entry.get_ptr();
        for _try in 0..100 {
            let mut dir = self.get_directory(curr as usize)?;
            if dir.next()==0 {
                if let Some(avail) = self.get_available_block()? {
                    // update the parent entry
                    entry.set_eof(entry.eof()+512);
                    entry.delta_blocks(1);
                    self.write_entry(parent_loc,&entry)?;
                    // link to new block
                    dir.set_links(None, Some(avail));
                    self.write_block(&dir.to_bytes(),curr as usize,0)?;
                    // fill new block
                    dir = Box::new(EntryBlock::new());
                    dir.set_links(Some(curr),Some(0));
                    self.write_block(&dir.to_bytes(),avail as usize,0)?;
                    return Ok(EntryLocation { block: avail, idx: 1});
                } else {
                    return Err(Box::new(Error::DiskFull));
                }
            }
            curr = dir.next();
        }
        error!("directory block count not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Get the next available entry location.
    /// Will try to expand the directory if necessary.
    fn get_available_entry(&mut self, key_block: u16) -> Result<EntryLocation,DYNERR> {
        let mut curr = key_block;
        for _try in 0..100 {
            let mut dir = self.get_directory(curr as usize)?;
            let locs = dir.entry_locations(curr);
            for loc in locs {
                if !dir.get_entry(&loc).is_active() {
                    return Ok(loc);
                }
            }
            curr = dir.next();
            if curr==0 {
                dir = self.get_directory(key_block as usize)?;
                if let Some(parent_loc) = dir.parent_entry_loc() {
                    return match self.expand_directory(&parent_loc) {
                        Ok(loc) => Ok(loc),
                        Err(e) => Err(e)
                    }
                } else {
                    // this is the volume directory which we cannot expand
                    return Err(Box::new(Error::DirectoryFull));
                }
            }
        }
        error!("directory block count not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    // Find specific entry in directory with the given key block
    fn search_entries(&mut self,stype: &Vec<StorageType>,name: &String,key_block: u16) -> Result<Option<EntryLocation>,DYNERR> {
        if !is_name_valid(&name) {
            error!("invalid ProDOS name {}",&name);
            return Err(Box::new(Error::Syntax));
        }
        let mut curr = key_block;
        for _try in 0..100 {
            let dir = self.get_directory(curr as usize)?;
            let locs = dir.entry_locations(curr);
            for loc in locs {
                let entry = dir.get_entry(&loc);
                if entry.is_active() && is_file_match::<Entry>(stype,name,&entry) {
                    return Ok(Some(loc));
                }
            }
            curr = dir.next();
            if curr==0 {
                return Ok(None);
            }
        }
        error!("directory block count not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Put path as [volume,subdir,subdir,...,last] where last could be an empty string,
    /// which indicates this is a directory.  If last is not empty, it could be either directory or file.
    /// Also check that the path is not too long accounting for prefix rules.
    fn normalize_path(&mut self,vol_name: &str,path: &str) -> Result<Vec<String>,DYNERR> {
        let mut path_nodes: Vec<String> = path.split("/").map(|s| s.to_string().to_uppercase()).collect();
        if &path[0..1]!="/" {
            path_nodes.insert(0,vol_name.to_string());
        } else {
            path_nodes = path_nodes[1..].to_vec();
        }
        // check prefix/path length
        let mut prefix_len = 0;
        let mut rel_path_len = 0;
        for s in path_nodes.iter() {
            if rel_path_len>0 {
                rel_path_len += 1 + s.len();
            } else {
                prefix_len += 1 + s.len();
                if prefix_len>64 {
                    prefix_len -= 1 + s.len();
                    rel_path_len += 1 + s.len();
                }
            }
        }
        if rel_path_len>64 {
            error!("ProDOS path too long, prefix {}, relative {}",prefix_len,rel_path_len);
            return Err(Box::new(Error::Range));
        }
        return Ok(path_nodes);
    }
    /// split the path into the last node (file or directory) and its parent path
    fn split_path(&mut self,vol_name: &str,path: &str) -> Result<[String;2],DYNERR> {
        let mut path_nodes = self.normalize_path(vol_name,path)?;
        // if last node is empty, remove it (means we have a directory)
        if path_nodes[path_nodes.len()-1].len()==0 {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let name = path_nodes[path_nodes.len()-1].clone();
        if path_nodes.len()<2 {
            return Err(Box::new(Error::PathNotFound));
        } else {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let parent_path: String = path_nodes.iter().map(|s| "/".to_string() + s).collect::<Vec<String>>().concat();
        return Ok([parent_path,name]);
    }
    fn search_volume(&mut self,file_types: &Vec<StorageType>,path: &str) -> Result<EntryLocation,DYNERR> {
        let vhdr = self.get_vol_header()?;
        let path_nodes = self.normalize_path(&vhdr.name(),path)?;
        if &path_nodes[0]!=&vhdr.name() {
            return Err(Box::new(Error::PathNotFound));
        }
        // path_nodes = [volume,dir,dir,...,dir|file|empty]
        let n = path_nodes.len();
        // There is no entry for the volume itself, so if that is the search, return an error
        if n<3 && path_nodes[n-1]=="" {
            return Err(Box::new(Error::PathNotFound));
        }
        // walk the tree
        let mut curr: u16 = VOL_KEY_BLOCK;
        for level in 1..n {
            let subdir = path_nodes[level].clone();
            let file_types_now = match level {
                l if l==n-1 => file_types.clone(),
                _ => vec![StorageType::SubDirEntry]
            };
            if let Some(loc) = self.search_entries(&file_types_now, &subdir, curr)? {
                // success conditions:
                // 1. this is the terminus
                // 2. this is the last subdirectory, terminus is empty, directory was requested
                if level==n-1 || level==n-2 && path_nodes[n-1]=="" && file_types.contains(&types::StorageType::SubDirEntry) {
                    return Ok(loc);
                }
                let entry = self.read_entry(&loc)?;
                curr = entry.get_ptr();
            } else {
                return Err(Box::new(Error::PathNotFound));
            }
        }
        return Err(Box::new(Error::PathNotFound));
    }
    fn find_file(&mut self,path: &str) -> Result<EntryLocation,DYNERR> {
        self.search_volume(&vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree],path)
    }
    /// Find the directory and return the key block pointer
    fn find_dir_key_block(&mut self,path: &str) -> Result<u16,DYNERR> {
        let vhdr = self.get_vol_header()?;
        let vname = "/".to_string() + &vhdr.name().to_lowercase();
        let vname2 = vname.clone() + "/";
        if path=="/" || path=="" || path.to_lowercase()==vname || path.to_lowercase()==vname2 {
            return Ok(VOL_KEY_BLOCK);
        }
        if let Ok(loc) = self.search_volume(&vec![StorageType::SubDirEntry], path) {
            let entry = self.read_entry(&loc)?;
            return Ok(entry.get_ptr());
        }
        return Err(Box::new(Error::PathNotFound));
    }
    /// Read the data referenced by a single index block
    fn read_index_block(
        &mut self,entry: &Entry,index_ptr: u16,buf: &mut [u8],fimg: &mut super::FileImage,count: &mut usize,eof: &mut usize
    ) -> STDRESULT {
        self.read_block(buf,index_ptr as usize,0)?;
        let index_block = buf.to_vec();
        for idx in 0..256 {
            let ptr = u16::from_le_bytes([index_block[idx],index_block[idx+256]]);
            let mut bytes = 512;
            if *eof + bytes > entry.eof() {
                bytes = entry.eof() - *eof;
            }
            if ptr>0 {
                self.read_block(buf,ptr as usize,0)?;
                fimg.chunks.insert(*count,buf.to_vec());
            }
            *count += 1;
            *eof += bytes;
        }
        Ok(())
    }
    /// Deallocate the index block and all data blocks referenced by it
    fn deallocate_index_block(&mut self,index_ptr: u16,buf: &mut [u8]) -> STDRESULT {
        self.read_block(buf,index_ptr as usize,0)?;
        let index_block = buf.to_vec();
        for idx in 0..256 {
            let ptr = u16::from_le_bytes([index_block[idx],index_block[idx+256]]);
            if ptr>0 {
                self.deallocate_block(ptr as usize)?;
            }
        }
        // ProDOS evidently swaps the index block halves upon deletion (why?)
        let swapped = [index_block[256..512].to_vec(),index_block[0..256].to_vec()].concat();
        self.write_block(&swapped,index_ptr as usize,0)?;
        self.deallocate_block(index_ptr as usize)?;
        Ok(())
    }
    /// Deallocate all the blocks associated with any entry
    fn deallocate_file_blocks(&mut self,entry: &Entry) -> STDRESULT {
        let mut buf: Vec<u8> = vec![0;512];
        let master_ptr = entry.get_ptr();
        match entry.storage_type() {
            StorageType::Seedling => {
                self.deallocate_block(master_ptr as usize)?;
            },
            StorageType::Sapling => {
                self.deallocate_index_block(master_ptr, &mut buf)?;
            },
            StorageType::Tree => {
                self.read_block(&mut buf,master_ptr as usize,0)?;
                let master_block = buf.clone();
                for idx in 0..256 {
                    let ptr = u16::from_le_bytes([master_block[idx],master_block[idx+256]]);
                    if ptr>0 {
                        self.deallocate_index_block(ptr, &mut buf)?;
                    }
                }
                // ProDOS evidently swaps the master index block halves upon deletion (why?)
                let swapped = [master_block[256..512].to_vec(),master_block[0..256].to_vec()].concat();
                self.write_block(&swapped,master_ptr as usize,0)?;
                self.deallocate_block(master_ptr as usize)?;
            }
            _ => panic!("cannot read file of this type")
        }
        Ok(())
    }
    /// Read any file into the sparse file format.  Use `FileImage.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&mut self,entry: &Entry) -> Result<super::FileImage,DYNERR> {
        let mut fimg = Disk::new_fimg(512);
        entry.metadata_to_fimg(&mut fimg);
        let mut buf: Vec<u8> = vec![0;512];
        let master_ptr = entry.get_ptr();
        let mut eof: usize = 0;
        let mut count: usize = 0;
        match entry.storage_type() {
            StorageType::Seedling => {
                self.read_block(&mut buf, master_ptr as usize, 0)?;
                fimg.chunks.insert(0, buf.clone());
                return Ok(fimg);
            },
            StorageType::Sapling => {
                self.read_index_block(&entry, master_ptr, &mut buf, &mut fimg, &mut count, &mut eof)?;
                return Ok(fimg);
            },
            StorageType::Tree => {
                self.read_block(&mut buf,master_ptr as usize,0)?;
                let master_block = buf.clone();
                for idx in 0..256 {
                    let ptr = u16::from_le_bytes([master_block[idx],master_block[idx+256]]);
                    if ptr>0 {
                        self.read_index_block(entry, ptr, &mut buf, &mut fimg, &mut count, &mut eof)?;
                    } else {
                        count += 256;
                        eof += 256*512;
                    }
                }
                return Ok(fimg);
            }
            _ => panic!("cannot read file of this type")
        }
    }
    /// Verify that the new name does not already exist
    fn ok_to_rename(&mut self,path: &str,new_name: &str) -> STDRESULT {
        if !is_name_valid(new_name) {
            error!("invalid ProDOS name {}",new_name);
            return Err(Box::new(Error::Syntax));
        }
        let types = vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree,StorageType::SubDirEntry];
        let vhdr = self.get_vol_header()?;
        let [parent_path,_old_name] = self.split_path(&vhdr.name(),path)?;
        if let Ok(key_block) = self.find_dir_key_block(&parent_path) {
            if let Some(_loc) = self.search_entries(&types, &new_name.to_string(), key_block)? {
                return Err(Box::new(Error::DuplicateFilename));
            }
        }
        return Ok(());
    }
    /// Prepare a directory for a new file or subdirectory.  This will modify the disk only if the directory needs to grow.
    fn prepare_to_write(&mut self,path: &str) -> Result<(String,u16,EntryLocation,u16),DYNERR> {
        let types = vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree,StorageType::SubDirEntry];
        let vhdr = self.get_vol_header()?;
        let [parent_path,name] = self.split_path(&vhdr.name(),path)?;
        if !is_name_valid(&name) {
            error!("invalid ProDOS name {}",&name);
            return Err(Box::new(Error::Syntax));
        }
        // find the parent key block, entry location, and new data block (or new key block if directory)
        if let Ok(key_block) = self.find_dir_key_block(&parent_path) {
            if let Some(_loc) = self.search_entries(&types, &name, key_block)? {
                return Err(Box::new(Error::DuplicateFilename));
            }
            match self.get_available_entry(key_block) {
                Ok(loc) => {
                    if let Some(new_block) = self.get_available_block()? {
                        return Ok((name,key_block,loc,new_block));
                    } else {
                        return Err(Box::new(Error::DiskFull));
                    }
                },
                Err(e) => return Err(e)
            }
        } else {
            return Err(Box::new(Error::PathNotFound));
        }
    }
    // Write a data block or account for a hole.
    // It is up to the creator of SparseFileData to ensure that the first block is not empty. 
    fn write_data_block_or_not(&mut self,count: usize,end: usize,ent: &mut Entry,buf_maybe: Option<&Vec<u8>>) -> Result<u16,DYNERR> {
        let mut eof = ent.eof();
        if let Some(buf) = buf_maybe {
            if let Some(data_block) = self.get_available_block()? {
                self.write_block(&buf,data_block as usize,0)?;
                eof += match count {
                    c if c+1 < end => 512,
                    _ => buf.len()
                };
                ent.delta_blocks(1);
                ent.set_eof(eof);
                return Ok(data_block);
            } else {
                error!("block not available, but it should have been");
                return Err(Box::new(Error::DiskFull));
            }
        } else {
            eof += 512;
            ent.set_eof(eof);
            return Ok(0);
        }
    }
    /// Write any sparse or sequential file.  Use `FileImage::desequence` to put sequential data
    /// into the file image format, with no loss of generality.
    /// The entry must already exist and point to the next available block.
    /// The creator of `FileImage` must ensure that the first block is allocated.
    /// This writes blocks more often than necessary, would be inadequate for an actual disk.
    fn write_file(&mut self,loc: EntryLocation,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.chunks.len()==0 {
            error!("empty data is not allowed for ProDOS file images");
            return Err(Box::new(Error::EndOfData));
        }
        let mut storage = StorageType::Seedling;
        let mut master_buf: Vec<u8> = vec![0;512];
        let mut master_ptr: u16 = 0;
        let mut master_count: u16 = 0;
        let mut index_buf: Vec<u8> = vec![0;512];
        let mut index_ptr: u16 = 0;
        let mut index_count: u16 = 0;
        let dir = self.get_directory(loc.block as usize)?;
        let mut entry = dir.get_entry(&loc);
        entry.set_eof(0);

        for count in 0..fimg.end() {

            let buf_maybe = fimg.chunks.get(&count);

            if master_count > 127 {
                return Err(Box::new(Error::DiskFull));
            }

            // Check that enough free blocks are available to proceed with this stage.
            let blocks_needed = match storage {
                StorageType::Seedling => {
                    match (buf_maybe,count) {
                        (_,i) if i==0 => 1, // data block
                        (None,_) => 1, // index block
                        (Some(_v),_) => 2 // index and data blocks
                    }
                },
                StorageType::Sapling => {
                    match (buf_maybe,index_count) {
                        (None,i) if i<256 => 0,
                        (Some(_v),i) if i<256 => 1, // data block
                        (None,_) => 1, // master block
                        (Some(_v),_) => 3 // master, index, and data blocks
                    }
                },
                StorageType::Tree => {
                    match (buf_maybe,index_count,index_ptr) {
                        (None,_,_) => 0,
                        (Some(_v),i,p) if i<256 && p>0 => 1, // data block
                        (Some(_v),i,p) if i<256 && p==0 => 2, // index and data blocks
                        (Some(_v),_,_) => 2 // index and data blocks
                    }
                }
                _ => panic!("unexpected storage type during write")
            };
            if blocks_needed > self.num_free_blocks()? {
                return Err(Box::new(Error::DiskFull));
            }
            
            match storage {
                StorageType::Seedling => {
                    if count>0 {
                        storage = StorageType::Sapling;
                        entry.change_storage_type(storage);
                        index_ptr = self.get_available_block().expect("unreachable").unwrap();
                        self.allocate_block(index_ptr as usize)?;
                        entry.delta_blocks(1);
                        pack_index_ptr(&mut index_buf,entry.get_ptr(),0);
                        entry.set_ptr(index_ptr);
                        index_count += 1;
                        let curr = self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                        pack_index_ptr(&mut index_buf, curr, index_count as usize);
                        self.write_block(&index_buf, index_ptr as usize, 0)?;
                        index_count += 1;
                    } else {
                        self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                        // index does not exist yet
                    }
                },
                StorageType::Sapling => {
                    if index_count > 255 {
                        storage = StorageType::Tree;
                        entry.change_storage_type(storage);
                        master_ptr = self.get_available_block().expect("unreachable").unwrap();
                        self.allocate_block(master_ptr as usize)?;
                        entry.set_ptr(master_ptr);
                        entry.delta_blocks(1);
                        pack_index_ptr(&mut master_buf,index_ptr,0);
                        master_count += 1;
                        index_ptr = 0;
                        index_count = 0;
                        index_buf = vec![0;512];
                        if buf_maybe!=None {
                            index_ptr = self.get_available_block().expect("unreachable").unwrap();
                            self.allocate_block(index_ptr as usize)?;
                            entry.delta_blocks(1);
                            let curr = self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                            pack_index_ptr(&mut index_buf, curr, 0);
                            self.write_block(&index_buf,index_ptr as usize,0)?;
                        } else {
                            self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                        }
                        index_count += 1;
                    } else {
                        let curr = self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                        pack_index_ptr(&mut index_buf,curr,index_count as usize);
                        self.write_block(&index_buf,index_ptr as usize,0)?;
                        index_count += 1;
                    }
                },
                StorageType::Tree => {
                    if index_count > 255 {
                        master_count += 1;
                        index_ptr = 0;
                        index_count = 0;
                        index_buf = vec![0;512];
                    }
                    if index_ptr==0 && buf_maybe!=None {
                        index_ptr = self.get_available_block().expect("unreachable").unwrap();
                        self.allocate_block(index_ptr as usize)?;
                        entry.delta_blocks(1);
                    }
                    let curr = self.write_data_block_or_not(count,fimg.end(),&mut entry,buf_maybe)?;
                    pack_index_ptr(&mut index_buf, curr, index_count as usize);
                    if index_ptr > 0 {
                        self.write_block(&index_buf,index_ptr as usize,0)?;
                    }
                    pack_index_ptr(&mut master_buf, index_ptr, master_count as usize);
                    self.write_block(&master_buf,master_ptr as usize,0)?;
                    index_count += 1;
                },
                _ => panic!("unexpected storage type during write")
            }
        }
        // update the entry, do last to capture all the changes
        let eof = super::FileImage::usize_from_truncated_le_bytes(&fimg.eof);
        if eof>0 {
            entry.set_eof(eof);
        }
        entry.set_all_access(fimg.access[0]);
        self.write_entry(&loc,&entry)?;
        return Ok(eof);
    }
    /// modify a file entry, optionally lock, unlock, rename, retype; attempt to change already locked file will fail.
    fn modify(&mut self,loc: &EntryLocation,maybe_lock: Option<bool>,maybe_new_name: Option<&str>,
        maybe_new_type: Option<&str>,maybe_new_aux: Option<u16>) -> STDRESULT {  
        let dir = self.get_directory(loc.block as usize)?;
        let mut entry = dir.get_entry(&loc);
        if !entry.get_access(Access::Rename) && maybe_new_name!=None {
            return Err(Box::new(Error::WriteProtected));
        }
        if let Some(lock) = maybe_lock {
            if lock {
                entry.set_access(Access::Destroy,false);
                entry.set_access(Access::Rename,false);
                entry.set_access(Access::Write,false);
            } else {
                entry.set_access(Access::Read,true);
                entry.set_access(Access::Destroy,true);
                entry.set_access(Access::Rename,true);
                entry.set_access(Access::Write,true);
            }
        }
        if let Some(new_name) = maybe_new_name {
            entry.rename(new_name);
        }
        if let Some(new_type) = maybe_new_type {
            match FileType::from_str(new_type) {
                Ok(typ) => entry.set_ftype(typ as u8),
                Err(e) => return Err(Box::new(e))
            }
        }
        if let Some(new_aux) = maybe_new_aux {
            entry.set_aux(new_aux);
        }
        self.write_entry(loc, &entry)?;
        return Ok(());
    }
    /// Output ProDOS directory as a JSON object, calls itself recursively
    fn tree_node(&mut self,dir_block: u16,include_meta: bool) -> Result<json::JsonValue,DYNERR> {
        let mut files = json::JsonValue::new_object();
        let mut curr = dir_block;
        while curr>0 {
            let dir = self.get_directory(curr as usize)?;
            for loc in dir.entry_locations(curr) {
                let entry = dir.get_entry(&loc);
                if entry.is_active() {
                    let key = entry.name();
                    files[&key] = json::JsonValue::new_object();
                    if entry.storage_type()==StorageType::SubDirEntry {
                        trace!("descend into directory {}",key);
                        files[&key]["files"] = self.tree_node(entry.get_ptr(),include_meta)?;
                    }
                    if include_meta {
                        files[&key]["meta"] = entry.meta_to_json();
                    }
                }
                curr = dir.next();
            }
        }
        Ok(files)
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self,chunk_len: usize) -> super::FileImage {
        Disk::new_fimg(chunk_len)
    }
    fn stat(&mut self) -> Result<super::Stat,DYNERR> {
        let vheader = self.get_vol_header()?;
        Ok(super::Stat {
            fs_name: FS_NAME.to_string(),
            label: vheader.name(),
            users: Vec::new(),
            block_size: BLOCK_SIZE,
            block_beg: 0,
            block_end: self.total_blocks,
            free_blocks: self.num_free_blocks()? as usize,
            raw: "".to_string()
        })
    }
    fn catalog_to_stdout(&mut self, path: &str) -> STDRESULT {
        let b = self.find_dir_key_block(path)?;
        let mut dir = self.get_directory(b as usize)?;
        println!();
        if b==2 {
            println!("{}{}","/".bright_blue().bold(),dir.name().bright_blue().bold());
        } else {
            println!("{}",dir.name().bright_blue().bold());
        }
        println!();
        println!(" {:15} {:4} {:6} {:16} {:16} {:7} {:7}",
            "NAME".bold(),"TYPE".bold(),"BLOCKS".bold(),
            "MODIFIED".bold(),"CREATED".bold(),"ENDFILE".bold(),"SUBTYPE".bold());
        println!();
        let mut curr = b;
        while curr>0 {
            dir = self.get_directory(curr as usize)?;
            for loc in dir.entry_locations(curr) {
                let entry = dir.get_entry(&loc);
                if entry.is_active() {
                    println!("{}",entry);
                }
            }
            curr = dir.next();
        }
        println!();
        let free = self.num_free_blocks()? as usize;
        let used = self.total_blocks-free;
        println!("BLOCKS FREE: {}  BLOCKS USED: {}  TOTAL BLOCKS: {}",free,used,self.total_blocks);
        println!();
        Ok(())
    }
    fn catalog_to_vec(&mut self, path: &str) -> Result<Vec<String>,DYNERR> {
        let mut ans = Vec::new();
        let mut curr = self.find_dir_key_block(path)?;
        while curr>0 {
            let dir = self.get_directory(curr as usize)?;
            for loc in dir.entry_locations(curr) {
                let entry = dir.get_entry(&loc);
                if entry.is_active() {
                    ans.push(entry.universal_row());
                }
            }
            curr = dir.next();
        }
        Ok(ans)
    }
    fn tree(&mut self,include_meta: bool) -> Result<String,DYNERR> {
        let vhdr = self.get_vol_header()?;
        let dir_block = self.find_dir_key_block("/")?;
        let mut tree = json::JsonValue::new_object();
        tree["file_system"] = json::JsonValue::String(FS_NAME.to_string());
        tree["files"] = self.tree_node(dir_block,include_meta)?;
        tree["label"] = json::JsonValue::new_object();
        tree["label"]["name"] = json::JsonValue::String(vhdr.name());
        if vhdr.total_blocks() <= 1600 && !include_meta {
            Ok(json::stringify_pretty(tree,2))
        } else if vhdr.total_blocks() <= 1600 && include_meta {
            Ok(json::stringify_pretty(tree,1))
        } else {
            Ok(json::stringify(tree))
        }
    }
    fn create(&mut self,path: &str) -> STDRESULT {
        match self.prepare_to_write(path) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize)?;
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0)?;
                // write the entry into the parent directory (may not be key block)
                let mut entry = Entry::create_subdir(&name,new_block,key_block,None);
                entry.delta_blocks(1);
                entry.set_eof(512);
                self.write_entry(&loc,&entry)?;
                // write the new directory's key block
                let mut subdir = KeyBlock::<SubDirHeader>::new();
                subdir.header.create(&name,loc.block,loc.idx as u8,None);
                self.write_block(&subdir.to_bytes(),new_block as usize,0)?;
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    fn delete(&mut self,path: &str) -> STDRESULT {
        if let Ok(loc) = self.find_file(path) {
            let entry = self.read_entry(&loc)?;
            if !entry.get_access(Access::Destroy) {
                return Err(Box::new(Error::WriteProtected));
            }
            self.deallocate_file_blocks(&entry)?;
            let mut dir = self.get_directory(loc.block as usize)?;
            dir.delete_entry(&loc);
            self.write_block(&dir.to_bytes(),loc.block as usize,0)?;
            let (key_ptr,mut key_dir) = self.get_key_directory(loc.block)?;
            key_dir.dec_file_count();
            self.write_block(&key_dir.to_bytes(),key_ptr as usize,0)?;
            return Ok(());
        }
        if let Ok(ptr) = self.find_dir_key_block(path) {
            let mut dir = self.get_directory(ptr as usize)?;
            if let Some(parent_loc) = dir.parent_entry_loc() {
                let mut parent_dir = self.get_directory(parent_loc.block as usize)?;
                if dir.file_count()>0 {
                    return Err(Box::new(Error::WriteProtected));
                }
                dir.delete();
                self.write_block(&dir.to_bytes(),ptr as usize,0)?;
                let mut next = ptr;
                for _try in 0..100 {
                    self.deallocate_block(next as usize)?;
                    next = dir.next();
                    if next==0 {
                        parent_dir.delete_entry(&parent_loc);
                        self.write_block(&parent_dir.to_bytes(),parent_loc.block as usize,0)?;
                        let (key_ptr,mut key_dir) = self.get_key_directory(parent_loc.block)?;
                        key_dir.dec_file_count();
                        self.write_block(&key_dir.to_bytes(),key_ptr as usize,0)?;
                        return Ok(());
                    }
                }
                error!("directory block count not plausible, aborting");
                return Err(Box::new(Error::EndOfData));
            } else {
                return Err(Box::new(Error::WriteProtected));
            }
        }
        return Err(Box::new(Error::PathNotFound));
    }
    fn protect(&mut self,_path: &str,_password: &str,_read: bool,_write: bool,_delete: bool) -> STDRESULT {
        error!("ProDOS does not support operation");
        Err(Box::new(Error::Syntax))
    }
    fn unprotect(&mut self,_path: &str) -> STDRESULT {
        error!("ProDOS does not support operation");
        Err(Box::new(Error::Syntax))
    }
    fn lock(&mut self,path: &str) -> STDRESULT {
        match self.find_file(path) {
            Ok(loc) => {
                self.modify(&loc,Some(true),None,None,None)
            },
            Err(e) => Err(e)
        }
    }
    fn unlock(&mut self,path: &str) -> STDRESULT {
        match self.find_file(path) {
            Ok(loc) => {
                self.modify(&loc,Some(false),None,None,None)
            },
            Err(e) => Err(e)
        }
    }
    fn rename(&mut self,path: &str,name: &str) -> STDRESULT {
        self.ok_to_rename(path, name)?;
        if let Ok(loc) = self.find_file(path) {
            return self.modify(&loc,None,Some(name),None,None);
        }
        if let Ok(ptr) = self.find_dir_key_block(path) {
            let dir = self.get_directory(ptr as usize)?;
            if let Some(parent_loc) = dir.parent_entry_loc() {
                return self.modify(&parent_loc,None,Some(name),None,None);
            }
        }
        return Err(Box::new(Error::PathNotFound));
    }
    fn retype(&mut self,path: &str,new_type: &str,sub_type: &str) -> STDRESULT {
        match u16::from_str(sub_type) {
            Ok(aux) => match self.find_file(path) {
                Ok(loc) => {
                    self.modify(&loc, None, None,Some(new_type),Some(aux))
                },
                Err(e) => Err(e)
            }
            Err(e) => Err(Box::new(e))
        }
    }
    fn bload(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(path,true)
    }
    fn bsave(&mut self,path: &str, dat: &[u8],start_addr: u16,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let padded = match trailing {
            Some(v) => [dat.to_vec(),v.to_vec()].concat(),
            None => dat.to_vec()
        };
        let mut fimg = Disk::new_fimg(BLOCK_SIZE);
        fimg.desequence(&padded);
        fimg.fs_type = vec![FileType::Binary as u8];
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
        fimg.aux = u16::to_le_bytes(start_addr).to_vec();
        return self.write_any(path,&fimg);
    }
    fn load(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(path,true)
    }
    fn save(&mut self,path: &str, dat: &[u8], typ: ItemType, _trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let mut fimg = Disk::new_fimg(BLOCK_SIZE);
        fimg.desequence(dat);
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
        match typ {
            ItemType::ApplesoftTokens => {
                let addr = applesoft::deduce_address(dat);
                fimg.fs_type = vec![FileType::ApplesoftCode as u8];
                fimg.aux = u16::to_le_bytes(addr).to_vec();
                debug!("Applesoft metadata {:?}, {:?}",fimg.fs_type,fimg.aux);
            },
            ItemType::IntegerTokens => {
                fimg.fs_type = vec![FileType::IntegerCode as u8];
            }
            _ => return Err(Box::new(Error::FileTypeMismatch))
        }
        return self.write_any(path,&fimg);
    }
    fn fimg_load_address(&self,fimg: &super::FileImage) -> u16 {
        super::FileImage::usize_from_truncated_le_bytes(&fimg.aux) as u16
    }
    fn fimg_file_data(&self,fimg: &super::FileImage) -> Result<Vec<u8>,DYNERR> {
        if &fimg.file_system != FS_NAME {
            return Err(Box::new(Error::FileTypeMismatch));
        }
        let eof = super::FileImage::usize_from_truncated_le_bytes(&fimg.eof);
        Ok(fimg.sequence_limited(eof))
    }
    fn read_raw(&mut self,path: &str,trunc: bool) -> Result<(u16,Vec<u8>),DYNERR> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc)?;
                let fimg = self.read_file(&entry)?;
                if trunc {
                    let eof = super::FileImage::usize_from_truncated_le_bytes(&fimg.eof);
                    Ok((entry.aux(),fimg.sequence_limited(eof)))
                } else {
                    Ok((entry.aux(),fimg.sequence()))
                }
            },
            Err(e) => Err(e)
        }
    }
    fn write_raw(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        let mut fimg = Disk::new_fimg(BLOCK_SIZE);
        fimg.desequence(dat);
        fimg.fs_type = vec![FileType::Text as u8];
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
        return self.write_any(path,&fimg);
    }
    fn read_text(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(path,true)
    }
    fn write_text(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        self.write_raw(path,dat)
    }
    fn read_records(&mut self,path: &str,_record_length: usize) -> Result<super::Records,DYNERR> {
        let encoder = Encoder::new(vec![0x0d]);
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc)?;
                let fimg = self.read_file(&entry)?;
                match super::Records::from_fimg(&fimg,entry.aux() as usize,encoder) {
                    Ok(ans) => Ok(ans),
                    Err(e) => Err(e)
                }
            },
            Err(e) => return Err(e)
        }
    }
    fn write_records(&mut self,path: &str, records: &super::Records) -> Result<usize,DYNERR> {
        let encoder = Encoder::new(vec![0x0d]);
        let mut fimg = self.new_fimg(BLOCK_SIZE);
        fimg.fs_type = vec![FileType::Text as u8];
        fimg.aux = super::FileImage::fix_le_vec(records.record_len,2);
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
        match records.update_fimg(&mut fimg, true, encoder) {
            Ok(_) => self.write_any(path,&fimg),
            Err(e) => Err(e)
        }
    }
    fn read_block(&mut self,num: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                let mut buf: Vec<u8> = vec![0;512];
                if block>=self.total_blocks {
                    return Err(Box::new(Error::Range));
                }
                self.read_block(&mut buf,block,0)?;
                Ok((0,buf))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > 512 || block>=self.total_blocks {
                    return Err(Box::new(Error::Range));
                }
                self.zap_block(dat,block,0)?;
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&mut self,path: &str) -> Result<super::FileImage,DYNERR> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc)?;
                return Ok(self.read_file(&entry)?);
            },
            Err(e) => return Err(e)
        }
    }
    fn write_any(&mut self,path: &str,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!=FS_NAME {
            error!("cannot write {} file image to prodos",fimg.file_system);
            return Err(Box::new(Error::IOError));
        }
        if fimg.chunk_len!=512 {
            error!("chunk length {} is incompatible with ProDOS",fimg.chunk_len);
            return Err(Box::new(Error::Range));
        }
        match self.prepare_to_write(path) {
            Ok((name,dir_key_block,loc,new_key_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(dir_key_block as usize)?;
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),dir_key_block as usize,0)?;
                // create the entry
                match Entry::create_file(&name,fimg,new_key_block,dir_key_block,None) {
                    Ok(entry) => self.write_entry(&loc,&entry)?,
                    Err(e) => return Err(e)
                }
                // write blocks
                match self.write_file(loc,fimg) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(e)
        }
    }
    fn decode_text(&self,dat: &[u8]) -> Result<String,DYNERR> {
        let file = types::SequentialText::from_bytes(&dat.to_vec());
        Ok(file.to_string())
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,DYNERR> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(_) => {
                error!("Cannot encode, perhaps use raw type");
                Err(Box::new(Error::FileTypeMismatch))
            }
        }
    }
    fn standardize(&mut self,ref_con: u16) -> HashMap<Block,Vec<usize>> {
        let mut ans: HashMap<Block,Vec<usize>> = HashMap::new();
        let mut curr = ref_con;
        while curr>0 {
            let mut dir = self.get_directory(curr as usize).expect("disk error");
            let locs = dir.entry_locations(curr);
            super::add_ignorable_offsets(&mut ans,Block::PO(curr as usize),dir.standardize(0));
            for loc in locs {
                let mut entry = dir.get_entry(&loc);
                let offset = 4 + (loc.idx-1)*0x27;
                super::add_ignorable_offsets(&mut ans,Block::PO(curr as usize),entry.standardize(offset));
                if entry.storage_type()==StorageType::SubDirEntry {
                    // recursively call to get the things in the subdirectory entries we want to ignore
                    let sub_map = self.standardize(entry.get_ptr());
                    super::combine_ignorable_offsets(&mut ans, sub_map);
                }
            }
            curr = dir.next();
        }
        return ans;
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        self.writeback_bitmap_buffer().expect("disk error");
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        let vhdr = self.get_vol_header().expect("disk error");
        for block in 0..vhdr.total_blocks() {
            let addr = Block::PO(block as usize);
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
        self.writeback_bitmap_buffer().expect("could not write back bitmap buffer");
        &mut self.img
    }
}

#[test]
fn test_path_normalize() {
    let img = Box::new(crate::img::dsk_po::PO::create(280));
    let mut disk = Disk::from_img(img);
    disk.format(&String::from("NEW.DISK"),true,None).expect("disk error");
    match disk.normalize_path("NEW.DISK","DIR1") {
        Ok(res) => assert_eq!(res,["NEW.DISK","DIR1"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("NEW.DISK","dir1/") {
        Ok(res) => assert_eq!(res,["NEW.DISK","DIR1",""]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("NEW.DISK","dir1/sub2") {
        Ok(res) => assert_eq!(res,["NEW.DISK","DIR1","SUB2"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("NEW.DISK","/new.disk/dir1/sub2") {
        Ok(res) => assert_eq!(res,["NEW.DISK","DIR1","SUB2"]),
        Err(e) => panic!("{}",e)
    }
    match disk.normalize_path("NEW.DISK","abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno/abcdefghijklmno") {
        Ok(_res) => panic!("normalize_path should have failed with path too long"),
        Err(e) => assert_eq!(e.to_string(),"RANGE ERROR")
    }
}

