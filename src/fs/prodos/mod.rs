//! # ProDOS file system module
//! This manipulates disk images containing one ProDOS volume.
//! 
//! * Single volume images only

mod boot;
pub mod types;
mod directory;

use a2kit_macro::DiskStruct;
use std::str::FromStr;
use std::fmt::Write;
use colored::*;
use log::info;
use types::*;
use directory::*;
use crate::disk_base;
use crate::disk_base::TextEncoder;
use crate::lang::applesoft;
use crate::create_fs_from_file;

/// The primary interface for disk operations.
pub struct Disk {
    blocks: Vec<[u8;512]>,
}

/// put a u16 into an index block in the prescribed fashion
fn pack_index_ptr(buf: &mut Vec<u8>,ptr: u16,idx: usize) {
    let bytes = u16::to_le_bytes(ptr);
    buf[idx] = bytes[0];
    buf[idx+256] = bytes[1];
}

impl Disk {
    /// Create an empty disk, all blocks are zero.
    pub fn new(num_blocks: u16) -> Self {
        let mut empty_blocks = Vec::new();
        for _i in 0..num_blocks {
            empty_blocks.push([0;512]);
        }
        Self {
            blocks: empty_blocks
        }
    }
    fn allocate_block(&mut self,iblock: usize) {
        let bitmap_ptr = self.get_vol_header().bitmap_ptr;
        let boff = iblock / 4096; // how many blocks into the map
        let byte = (iblock - 4096*boff) / 8;
        let bit = 7 - (iblock - 4096*boff) % 8;
        let bptr = u16::from_le_bytes(bitmap_ptr) as usize + boff;
        let mut map = self.blocks[bptr][byte];
        map &= (1 << bit as u8) ^ u8::MAX;
        self.blocks[bptr][byte] = map;
    }
    fn deallocate_block(&mut self,iblock: usize) {
        let bitmap_ptr = self.get_vol_header().bitmap_ptr;
        let boff = iblock / 4096; // how many blocks into the map
        let byte = (iblock - 4096*boff) / 8;
        let bit = 7 - (iblock - 4096*boff) % 8;
        let bptr = u16::from_le_bytes(bitmap_ptr) as usize + boff;
        let mut map = self.blocks[bptr][byte];
        map |= 1 << bit as u8;
        self.blocks[bptr][byte] = map;
    }
    fn is_block_free(&self,iblock: usize) -> bool {
        let bitmap_ptr = self.get_vol_header().bitmap_ptr;
        let boff = iblock / 4096; // how many blocks into the map
        let byte = (iblock - 4096*boff) / 8;
        let bit = 7 - (iblock - 4096*boff) % 8;
        let bptr = u16::from_le_bytes(bitmap_ptr) as usize + boff;
        let map = self.blocks[bptr][byte];
        return (map & (1 << bit as u8)) > 0;
    }
    fn num_free_blocks(&self) -> u16 {
        let mut free: u16 = 0;
        for i in 0..self.blocks.len() {
            if self.is_block_free(i) {
                free += 1;
            }
        }
        free
    }
    fn read_block(&self,data: &mut Vec<u8>, iblock: usize, offset: usize) {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x,
            _ => bytes
        };
        for i in 0..actual_len as usize {
            data[offset + i] = self.blocks[iblock][i];
        }
    }
    /// Write and allocate the block in one step.
    fn write_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        self.zap_block(data,iblock,offset);
        self.allocate_block(iblock);
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    fn zap_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x,
            _ => bytes
        };
        for i in 0..actual_len as usize {
            self.blocks[iblock][i] = data[offset + i];
        }
    }
    fn get_available_block(&self) -> Option<u16> {
        for block in 0..self.blocks.len() {
            if self.is_block_free(block) {
                return Some(block as u16);
            }
        }
        return None;
    }

    pub fn format(&mut self, vol_name: &str, floppy: bool, time: Option<chrono::NaiveDateTime>) {
        // make sure we start with all 0
        for iblock in 0..self.blocks.len() {
            self.zap_block(&[0;512].to_vec(),iblock,0);
        }
        // calculate volume parameters and setup volume directory
        let mut volume_dir = KeyBlock::<VolDirHeader>::new();
        let bitmap_blocks = 1 + self.blocks.len() / 4096;
        volume_dir.set_links(Some(0), Some(VOL_KEY_BLOCK+1));
        volume_dir.header.format(self.blocks.len() as u16,vol_name,time);
        let first = u16::from_le_bytes(volume_dir.header.bitmap_ptr) as usize;

        // volume key block
        self.write_block(&volume_dir.to_bytes(),VOL_KEY_BLOCK as usize,0);

        // mark all blocks as free
        for b in 0..self.blocks.len() {
            self.deallocate_block(b);
        }

        // mark volume key and bitmap blocks as used
        self.allocate_block(VOL_KEY_BLOCK as usize);
        for b in first..first + bitmap_blocks {
            self.allocate_block(b);
        }
        
        // boot loader blocks
        if floppy {
            self.write_block(&boot::FLOPPY_BLOCK0.to_vec(),0,0);
        }
        else {
            self.write_block(&boot::HD_BLOCK0.to_vec(), 0, 0)
        }
        self.write_block(&vec![0;512],1,0);

        // next 3 volume directory blocks
        for b in 3..6 {
            let mut this = EntryBlock::new();
            if b==5 {
                this.set_links(Some(b-1), Some(0));
            } else {
                this.set_links(Some(b-1), Some(b+1));
            }
            self.write_block(&this.to_bytes(),b as usize,0);
        }
    }
    fn get_vol_header(&self) -> VolDirHeader {
        let mut buf: Vec<u8> = vec![0;512];
        self.read_block(&mut buf,VOL_KEY_BLOCK as usize,0);
        let volume_dir = KeyBlock::<VolDirHeader>::from_bytes(&buf);
        return volume_dir.header;
    }
    /// Return the correct trait object assuming this block is a directory block.
    /// May return a key block or an entry block.
    fn get_directory(&self,iblock: usize) -> Box<dyn Directory> {
        let mut buf: Vec<u8> = vec![0;512];
        self.read_block(&mut buf,iblock,0);
        match (iblock==VOL_KEY_BLOCK as usize,buf[0]==0 && buf[1]==0) {
            (true,true) => Box::new(KeyBlock::<VolDirHeader>::from_bytes(&buf)),
            (true,false) => Box::new(KeyBlock::<VolDirHeader>::from_bytes(&buf)),
            (false,true) => Box::new(KeyBlock::<SubDirHeader>::from_bytes(&buf)),
            (false,false) => Box::new(EntryBlock::from_bytes(&buf))
        }
    }
    /// Find the key block assuming this block is a directory block, and return the
    /// block pointer and corresponding trait object in a tuple.
    fn get_key_directory(&self,ptr: u16) -> (u16,Box<dyn Directory>) {
        let mut curr = ptr;
        for _try in 0..100 {
            let test_dir = self.get_directory(curr as usize);
            if test_dir.prev()==0 {
                return (curr,test_dir);
            }
            curr = test_dir.prev();
        }
        panic!("too many blocks for this directory, disk likely damaged");
    }
    /// Given an entry location get the entry from disk
    fn read_entry(&self,loc: &EntryLocation) -> Entry {
        let dir = self.get_directory(loc.block as usize);
        return dir.get_entry(loc);
    }
    /// Given a modified entry and location, write the change to disk.
    /// Any other unsaved changes in the block are lost.  Maybe this should go away.
    fn write_entry(&mut self,loc: &EntryLocation,entry: &Entry) {
        let mut dir = self.get_directory(loc.block as usize);
        dir.set_entry(loc,*entry);
        let buf = dir.to_bytes();
        self.write_block(&buf,loc.block as usize,0);
    }
    /// Try to add another entry block to the directory with the given parent entry.
    /// If successful return the location of the first entry in the new block.
    /// This is called when the directory runs out of entries.
    fn expand_directory(&mut self, parent_loc: &EntryLocation) -> Result<EntryLocation,Error> {
        let mut entry = self.read_entry(&parent_loc);
        if entry.storage_type()!=StorageType::SubDirEntry {
            return Err(Error::FileTypeMismatch);
        }
        let mut curr = entry.get_ptr();
        for _try in 0..100 {
            let mut dir = self.get_directory(curr as usize);
            if dir.next()==0 {
                if let Some(avail) = self.get_available_block() {
                    // update the parent entry
                    entry.set_eof(entry.eof()+512);
                    entry.delta_blocks(1);
                    self.write_entry(parent_loc,&entry);
                    // link to new block
                    dir.set_links(None, Some(avail));
                    self.write_block(&dir.to_bytes(),curr as usize,0);
                    // fill new block
                    dir = Box::new(EntryBlock::new());
                    dir.set_links(Some(curr),Some(0));
                    self.write_block(&dir.to_bytes(),avail as usize,0);
                    return Ok(EntryLocation { block: avail, idx: 1});
                } else {
                    return Err(Error::DiskFull);
                }
            }
            curr = dir.next();
        }
        panic!("too many blocks for this directory, disk likely damaged");
    }
    /// Get the next available entry location.
    /// Will try to expand the directory if necessary.
    fn get_available_entry(&mut self, key_block: u16) -> Result<EntryLocation,Error> {
        let mut curr = key_block;
        for _try in 0..100 {
            let mut dir = self.get_directory(curr as usize);
            let locs = dir.entry_locations(curr);
            for loc in locs {
                if !dir.get_entry(&loc).is_active() {
                    return Ok(loc);
                }
            }
            curr = dir.next();
            if curr==0 {
                dir = self.get_directory(key_block as usize);
                if let Some(parent_loc) = dir.parent_entry_loc() {
                    return match self.expand_directory(&parent_loc) {
                        Ok(loc) => Ok(loc),
                        Err(e) => Err(e)
                    }
                } else {
                    // this is the volume directory which we cannot expand
                    return Err(Error::DirectoryFull);
                }
            }
        }
        panic!("too many blocks for this directory, disk likely damaged");
    }
    // Find specific entry in directory with the given key block
    fn search_entries(&self,stype: &Vec<StorageType>,name: &String,key_block: u16) -> Option<EntryLocation> {
        let mut curr = key_block;
        for _try in 0..100 {
            let dir = self.get_directory(curr as usize);
            let locs = dir.entry_locations(curr);
            for loc in locs {
                let entry = dir.get_entry(&loc);
                if entry.is_active() && is_file_match::<Entry>(stype,name,&entry) {
                    return Some(loc);
                }
            }
            curr = dir.next();
            if curr==0 {
                return None;
            }
        }
        panic!("too many blocks for this directory, disk likely damaged");
    }
    /// put path as [volume,subdir,subdir,...,last] where last could be an empty string,
    /// which indicates this is a directory.  If last is not empty, it could be either directory or file.
    fn normalize_path(&self,path: &str) -> Vec<String> {
        let volume_dir = self.get_directory(VOL_KEY_BLOCK as usize);
        let prefix = volume_dir.name();
        let mut path_nodes: Vec<String> = path.split("/").map(|s| s.to_string().to_uppercase()).collect();
        if &path[0..1]!="/" {
            path_nodes.insert(0,prefix);
        } else {
            path_nodes = path_nodes[1..].to_vec();
        }
        return path_nodes;
    }
    fn search_volume(&self,file_types: &Vec<StorageType>,path: &str) -> Result<EntryLocation,Error> {
        let volume_dir = self.get_directory(VOL_KEY_BLOCK as usize);
        let path_nodes = self.normalize_path(path);
        if &path_nodes[0]!=&volume_dir.name() {
            return Err(Error::PathNotFound);
        }
        // path_nodes = [volume,dir,dir,...,dir|file|empty]
        let n = path_nodes.len();
        // There is no entry for the volume itself, so if that is the search, return an error
        if n<3 && path_nodes[n-1]=="" {
            return Err(Error::PathNotFound);
        }
        // walk the tree
        let mut curr: u16 = VOL_KEY_BLOCK;
        for level in 1..n {
            let subdir = path_nodes[level].clone();
            let file_types_now = match level {
                l if l==n-1 => file_types.clone(),
                _ => vec![StorageType::SubDirEntry]
            };
            if let Some(loc) = self.search_entries(&file_types_now, &subdir, curr) {
                // success conditions:
                // 1. this is the terminus
                // 2. this is the last subdirectory, terminus is empty, directory was requested
                if level==n-1 || level==n-2 && path_nodes[n-1]=="" && file_types.contains(&types::StorageType::SubDirEntry) {
                    return Ok(loc);
                }
                let entry = self.read_entry(&loc);
                curr = entry.get_ptr();
            } else {
                return Err(Error::PathNotFound);
            }
        }
        return Err(Error::PathNotFound);
    }
    fn find_file(&self,path: &str) -> Result<EntryLocation,Error> {
        return self.search_volume(&vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree],path);
    }
    /// Find the directory and return the key block pointer
    fn find_dir_key_block(&self,path: &str) -> Result<u16,Error> {
        let volume_dir = self.get_directory(VOL_KEY_BLOCK as usize);
        if path=="/" || path=="" || path==&("/".to_string()+&volume_dir.name()) {
            return Ok(VOL_KEY_BLOCK);
        }
        if let Ok(loc) = self.search_volume(&vec![StorageType::SubDirEntry], path) {
            let entry = self.read_entry(&loc);
            return Ok(entry.get_ptr());
        }
        return Err(Error::PathNotFound);
    }
    /// Read the data referenced by a single index block
    fn read_index_block(&self,entry: &Entry,index_ptr: u16,buf: &mut Vec<u8>,fimg: &mut disk_base::FileImage,count: &mut usize,eof: &mut usize) {
        self.read_block(buf,index_ptr as usize,0);
        let index_block = buf.clone();
        for idx in 0..256 {
            let ptr = u16::from_le_bytes([index_block[idx],index_block[idx+256]]);
            let mut bytes = 512;
            if *eof + bytes > entry.eof() {
                bytes = entry.eof() - *eof;
            }
            if ptr>0 {
                self.read_block(buf,ptr as usize,0);
                fimg.chunks.insert(*count,buf[0..bytes].to_vec());
            }
            *count += 1;
            *eof += bytes;
        }
    }
    /// Deallocate the index block and all data blocks referenced by it
    fn deallocate_index_block(&mut self,index_ptr: u16,buf: &mut Vec<u8>) {
        self.read_block(buf,index_ptr as usize,0);
        let index_block = buf.clone();
        for idx in 0..256 {
            let ptr = u16::from_le_bytes([index_block[idx],index_block[idx+256]]);
            if ptr>0 {
                self.deallocate_block(ptr as usize);
            }
        }
        // ProDOS evidently swaps the index block halves upon deletion (why?)
        let swapped = [index_block[256..512].to_vec(),index_block[0..256].to_vec()].concat();
        self.write_block(&swapped,index_ptr as usize,0);
        self.deallocate_block(index_ptr as usize);
    }
    /// Deallocate all the blocks associated with any entry
    fn deallocate_file_blocks(&mut self,entry: &Entry) {
        let mut buf: Vec<u8> = vec![0;512];
        let master_ptr = entry.get_ptr();
        match entry.storage_type() {
            StorageType::Seedling => {
                self.deallocate_block(master_ptr as usize);
            },
            StorageType::Sapling => {
                self.deallocate_index_block(master_ptr, &mut buf);
            },
            StorageType::Tree => {
                self.read_block(&mut buf,master_ptr as usize,0);
                let master_block = buf.clone();
                for idx in 0..256 {
                    let ptr = u16::from_le_bytes([master_block[idx],master_block[idx+256]]);
                    if ptr>0 {
                        self.deallocate_index_block(ptr, &mut buf);
                    }
                }
                // ProDOS evidently swaps the master index block halves upon deletion (why?)
                let swapped = [master_block[256..512].to_vec(),master_block[0..256].to_vec()].concat();
                self.write_block(&swapped,master_ptr as usize,0);
                self.deallocate_block(master_ptr as usize);
            }
            _ => panic!("cannot read file of this type")
        }
    }
    /// Read any file into the sparse file format.  Use `FileImage.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&self,entry: &Entry) -> disk_base::FileImage {
        let mut fimg = disk_base::FileImage::new(512);
        entry.metadata_to_fimg(&mut fimg);
        let mut buf: Vec<u8> = vec![0;512];
        let master_ptr = entry.get_ptr();
        let mut eof: usize = 0;
        let mut count: usize = 0;
        match entry.storage_type() {
            StorageType::Seedling => {
                self.read_block(&mut buf, master_ptr as usize, 0);
                fimg.chunks.insert(0, buf[0..entry.eof()].to_vec());
                return fimg;
            },
            StorageType::Sapling => {
                self.read_index_block(&entry, master_ptr, &mut buf, &mut fimg, &mut count, &mut eof);
                return fimg;
            },
            StorageType::Tree => {
                self.read_block(&mut buf,master_ptr as usize,0);
                let master_block = buf.clone();
                for idx in 0..256 {
                    let ptr = u16::from_le_bytes([master_block[idx],master_block[idx+256]]);
                    if ptr>0 {
                        self.read_index_block(entry, ptr, &mut buf, &mut fimg, &mut count, &mut eof);
                    } else {
                        count += 256;
                        eof += 256*512;
                    }
                }
                return fimg;
            }
            _ => panic!("cannot read file of this type")
        }
    }
    /// Prepare a directory for a new file or subdirectory.  This will modify the disk only if the directory needs to grow.
    fn prepare_to_write(&mut self,path: &str,types: &Vec<StorageType>) -> Result<(String,u16,EntryLocation,u16),Error> {
        // following builds the subdirectory path
        let mut path_nodes = self.normalize_path(path);
        if path_nodes[path_nodes.len()-1].len()==0 {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let name = path_nodes[path_nodes.len()-1].clone();
        if path_nodes.len()<2 {
            return Err(Error::PathNotFound);
        } else {
            path_nodes = path_nodes[0..path_nodes.len()-1].to_vec();
        }
        let subdir_path: String = path_nodes.iter().map(|s| "/".to_string() + s).collect::<Vec<String>>().concat();
        // find the parent key block, entry location, and new data block (or new key block if directory)
        if let Ok(key_block) = self.find_dir_key_block(&subdir_path) {
            if let Some(_loc) = self.search_entries(types, &name, key_block) {
                return Err(Error::DuplicateFilename);
            }
            match self.get_available_entry(key_block) {
                Ok(loc) => {
                    if let Some(new_block) = self.get_available_block() {
                        return Ok((name,key_block,loc,new_block));
                    } else {
                        return Err(Error::DiskFull);
                    }
                },
                Err(e) => return Err(e)
            }
        } else {
            return Err(Error::PathNotFound);
        }
    }
    /// Write any sparse or sequential file.  Use `FileImage::desequence` to put sequential data
    /// into the sparse file format, with no loss of generality.
    /// The entry must already exist and point to the next available block.
    /// The creator of `FileImage` must ensure that the first block is allocated.
    /// This writes blocks more often than necessary, would be inadequate for an actual disk.
    fn write_file(&mut self,loc: EntryLocation,fimg: &disk_base::FileImage) -> Result<usize,Error> {
        let mut storage = StorageType::Seedling;
        let mut master_buf: Vec<u8> = vec![0;512];
        let mut master_ptr: u16 = 0;
        let mut master_count: u16 = 0;
        let mut index_buf: Vec<u8> = vec![0;512];
        let mut index_ptr: u16 = 0;
        let mut index_count: u16 = 0;
        let mut eof: usize = 0;
        let dir = self.get_directory(loc.block as usize);
        let mut entry = dir.get_entry(&loc);
        let mut blocks_available = self.num_free_blocks();

        for count in 0..fimg.end() {

            let buf_maybe = fimg.chunks.get(&count);

            if master_count > 127 {
                return Err(Error::DiskFull);
            }

            // Check that enough free blocks are available to proceed with this stage.
            // If this isn't done right we can get a panic later.
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
            // calling num_free_blocks() here slows down tests enormously
            if blocks_needed > blocks_available {
                return Err(Error::DiskFull);
            } else {
                blocks_available -= blocks_needed;
            }

            // Closure to write the data block.
            // It is up to the creator of SparseFileData to ensure that the first block is not empty. 
            let mut write_data_block_or_not = |disk: &mut Disk,ent: &mut Entry| {
                let mut data_block: u16 = 0;
                if let Some(buf) = buf_maybe {
                    data_block = disk.get_available_block().unwrap();
                    disk.write_block(&buf,data_block as usize,0);
                    eof += match count {
                        c if c+1 < fimg.end() => 512,
                        _ => buf.len()
                    };
                    ent.delta_blocks(1);
                } else {
                    eof += 512;
                }
                ent.set_eof(eof);
                return data_block;
            };
            
            match storage {
                StorageType::Seedling => {
                    if count>0 {
                        storage = StorageType::Sapling;
                        entry.change_storage_type(storage);
                        index_ptr = self.get_available_block().unwrap();
                        self.allocate_block(index_ptr as usize);
                        entry.delta_blocks(1);
                        pack_index_ptr(&mut index_buf,entry.get_ptr(),0);
                        entry.set_ptr(index_ptr);
                        index_count += 1;
                        let curr = write_data_block_or_not(self,&mut entry);
                        pack_index_ptr(&mut index_buf, curr, index_count as usize);
                        self.write_block(&index_buf, index_ptr as usize, 0);
                        index_count += 1;
                    } else {
                        write_data_block_or_not(self,&mut entry);
                        // index does not exist yet
                    }
                },
                StorageType::Sapling => {
                    if index_count > 255 {
                        storage = StorageType::Tree;
                        entry.change_storage_type(storage);
                        master_ptr = self.get_available_block().unwrap();
                        self.allocate_block(master_ptr as usize);
                        entry.set_ptr(master_ptr);
                        entry.delta_blocks(1);
                        pack_index_ptr(&mut master_buf,index_ptr,0);
                        master_count += 1;
                        index_ptr = 0;
                        index_count = 0;
                        index_buf = vec![0;512];
                        if buf_maybe!=None {
                            index_ptr = self.get_available_block().unwrap();
                            self.allocate_block(index_ptr as usize);
                            entry.delta_blocks(1);
                            let curr = write_data_block_or_not(self,&mut entry);
                            pack_index_ptr(&mut index_buf, curr, 0);
                            self.write_block(&index_buf,index_ptr as usize,0);
                        } else {
                            write_data_block_or_not(self,&mut entry);
                        }
                        index_count += 1;
                    } else {
                        let curr = write_data_block_or_not(self,&mut entry);
                        pack_index_ptr(&mut index_buf,curr,index_count as usize);
                        self.write_block(&index_buf,index_ptr as usize,0);
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
                        index_ptr = self.get_available_block().unwrap();
                        self.allocate_block(index_ptr as usize);
                        entry.delta_blocks(1);
                    }
                    let curr = write_data_block_or_not(self,&mut entry);
                    pack_index_ptr(&mut index_buf, curr, index_count as usize);
                    if index_ptr > 0 {
                        self.write_block(&index_buf,index_ptr as usize,0);
                    }
                    pack_index_ptr(&mut master_buf, index_ptr, master_count as usize);
                    self.write_block(&master_buf,master_ptr as usize,0);
                    index_count += 1;
                },
                _ => panic!("unexpected storage type during write")
            }

            // update the entry, do last to capture all the changes
            entry.set_eof(usize::max(entry.eof(),fimg.eof));
            self.write_entry(&loc,&entry);
        }
        return Ok(eof);
    }
    /// modify a file entry, optionally lock, unlock, rename, retype; attempt to change already locked file will fail.
    fn modify(&mut self,loc: &EntryLocation,maybe_lock: Option<bool>,maybe_new_name: Option<&str>,
        maybe_new_type: Option<&str>,maybe_new_aux: Option<u16>) -> Result<(),Box<dyn std::error::Error>> {
        let dir = self.get_directory(loc.block as usize);
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
        self.write_entry(loc, &entry);
        return Ok(());
    }
    /// Return a disk object if the image data verifies as ProDOS,
    /// otherwise return None.  The image is allowed to be any integral
    /// number of blocks up to the maximum of 65535.  Various fields in the
    /// volume directory header are checked for consistency.
    pub fn from_img(dimg: &Vec<u8>) -> Option<Self> {
        let block_count = dimg.len()/512;
        if dimg.len()%512 != 0 || block_count > 65535 {
            return None;
        }
        let mut disk = Self::new(block_count as u16);

        for block in 0..block_count {
            for byte in 0..512 {
                disk.blocks[block][byte] = dimg[byte+block*512];
            }
        }
        // test the volume directory header to see if this is ProDOS
        let first_char_patt = "ABCDEFGHIJKLMNOPQRSTUVWXYZ.";
        let char_patt = [first_char_patt,"0123456789"].concat();
        let vol_key: KeyBlock<VolDirHeader> = KeyBlock::from_bytes(&disk.blocks[2].to_vec());
        let (nibs,name) = vol_key.header.fname();
        let total_blocks = u16::from_le_bytes([disk.blocks[2][0x29],disk.blocks[2][0x2A]]);
        if disk.blocks[2][0x23]!=0x27 || disk.blocks[2][0x24]!=0x0D {
            info!("unexpected header bytes {}, {}",disk.blocks[2][0x23],disk.blocks[2][0x24]);
            return None;
        }
        if total_blocks as usize!=block_count {
            info!("unexpected total blocks {}",total_blocks);
            return None;
        }
        if vol_key.prev()!=0 || vol_key.next()!=3 || (nibs >> 4)!=15 {
            info!("unexpected volume name length or links");
            return None;
        }
        if !first_char_patt.contains(name[0] as char) {
            info!("volume name unexpected character");
            return None;
        }
        for i in 1..(nibs & 0x0F) {
            if !char_patt.contains(name[i as usize] as char) {
                info!("volume name unexpected character");
                return None;
            }
        }
        return Some(disk);
    }
}

impl disk_base::DiskFS for Disk {
    fn catalog_to_stdout(&self, path: &str) -> Result<(),Box<dyn std::error::Error>> {
        match self.find_dir_key_block(path) {
            Ok(b) => {
                let mut dir = self.get_directory(b as usize);
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
                    dir = self.get_directory(curr as usize);
                    for loc in dir.entry_locations(curr) {
                        let entry = dir.get_entry(&loc);
                        if entry.is_active() {
                            println!("{}",entry);
                        }
                    }
                    curr = dir.next();
                }
                println!();
                let total = self.blocks.len();
                let free = self.num_free_blocks() as usize;
                let used = total-free;
                println!("BLOCKS FREE: {}  BLOCKS USED: {}  TOTAL BLOCKS: {}",free,used,total);
                println!();
                Ok(())
            }
            Err(e) => Err(Box::new(e))
        }
    }
    fn create(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::SubDirEntry]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                let mut entry = Entry::create_subdir(&name,new_block,key_block,None);
                entry.delta_blocks(1);
                entry.set_eof(512);
                self.write_entry(&loc,&entry);
                // write the new directory's key block
                let mut subdir = KeyBlock::<SubDirHeader>::new();
                subdir.header.create(&name,loc.block,loc.idx as u8,None);
                self.write_block(&subdir.to_bytes(),new_block as usize,0);
                Ok(())
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn delete(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>> {
        if let Ok(loc) = self.find_file(path) {
            let entry = self.read_entry(&loc);
            if !entry.get_access(Access::Destroy) {
                return Err(Box::new(Error::WriteProtected));
            }
            self.deallocate_file_blocks(&entry);
            let mut dir = self.get_directory(loc.block as usize);
            dir.delete_entry(&loc);
            self.write_block(&dir.to_bytes(),loc.block as usize,0);
            let (key_ptr,mut key_dir) = self.get_key_directory(loc.block);
            key_dir.dec_file_count();
            self.write_block(&key_dir.to_bytes(),key_ptr as usize,0);
            return Ok(());
        }
        if let Ok(ptr) = self.find_dir_key_block(path) {
            let mut dir = self.get_directory(ptr as usize);
            if let Some(parent_loc) = dir.parent_entry_loc() {
                let mut parent_dir = self.get_directory(parent_loc.block as usize);
                if dir.file_count()>0 {
                    return Err(Box::new(Error::WriteProtected));
                }
                dir.delete();
                self.write_block(&dir.to_bytes(),ptr as usize,0);
                let mut next = ptr;
                for _try in 0..100 {
                    self.deallocate_block(next as usize);
                    next = dir.next();
                    if next==0 {
                        parent_dir.delete_entry(&parent_loc);
                        self.write_block(&parent_dir.to_bytes(),parent_loc.block as usize,0);
                        let (key_ptr,mut key_dir) = self.get_key_directory(parent_loc.block);
                        key_dir.dec_file_count();
                        self.write_block(&key_dir.to_bytes(),key_ptr as usize,0);
                        return Ok(());
                    }
                }
                panic!("too many blocks for this directory, disk likely damaged");
            } else {
                return Err(Box::new(Error::WriteProtected));
            }
        }
        return Err(Box::new(Error::PathNotFound));
    }
    fn lock(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                self.modify(&loc,Some(true),None,None,None)
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn unlock(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                self.modify(&loc,Some(false),None,None,None)
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn rename(&mut self,path: &str,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        if let Ok(loc) = self.find_file(path) {
            return self.modify(&loc,None,Some(name),None,None);
        }
        if let Ok(ptr) = self.find_dir_key_block(path) {
            let dir = self.get_directory(ptr as usize);
            if let Some(parent_loc) = dir.parent_entry_loc() {
                return self.modify(&parent_loc,None,Some(name),None,None);
            }
        }
        return Err(Box::new(Error::PathNotFound));
    }
    fn retype(&mut self,path: &str,new_type: &str,sub_type: &str) -> Result<(),Box<dyn std::error::Error>> {
        match u16::from_str(sub_type) {
            Ok(aux) => match self.find_file(path) {
                Ok(loc) => {
                    self.modify(&loc, None, None,Some(new_type),Some(aux))
                },
                Err(e) => Err(Box::new(e))
            }
            Err(e) => Err(Box::new(e))
        }
    }
    fn bload(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                Ok((entry.aux(),ans.sequence()))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn bsave(&mut self,path: &str, dat: &Vec<u8>,start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let padded = match trailing {
            Some(v) => [dat.clone(),v.clone()].concat(),
            None => dat.clone()
        };
        let mut fimg = disk_base::FileImage::desequence(BLOCK_SIZE, &padded);
        fimg.fs_type = (FileType::Binary as u8).to_string();
        fimg.aux = start_addr.to_string();
        return self.write_any(path,&fimg);
    }
    fn load(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                return Ok((0,ans.sequence()));
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn save(&mut self,path: &str, dat: &Vec<u8>, typ: disk_base::ItemType, _trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let mut fimg = disk_base::FileImage::desequence(BLOCK_SIZE, dat);
        match typ {
            disk_base::ItemType::ApplesoftTokens => {
                let addr = applesoft::deduce_address(dat);
                fimg.fs_type = (FileType::ApplesoftCode as u8).to_string();
                fimg.aux = addr.to_string();
                info!("Applesoft metadata {}, {}",fimg.fs_type,fimg.aux);
            },
            disk_base::ItemType::IntegerTokens => {
                fimg.fs_type = (FileType::IntegerCode as u8).to_string();
            }
            _ => panic!("cannot write this type of program file")
        }
        return self.write_any(path,&fimg);
    }
    fn read_text(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                return Ok((0,ans.sequence()));
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_text(&mut self,path: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        let mut fimg = disk_base::FileImage::desequence(BLOCK_SIZE, dat);
        fimg.fs_type = (FileType::Text as u8).to_string();
        return self.write_any(path,&fimg);
    }
    fn read_records(&self,path: &str,_record_length: usize) -> Result<disk_base::Records,Box<dyn std::error::Error>> {
        let encoder = Encoder::new(Some(0x0d));
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let fimg = self.read_file(&entry);
                match disk_base::Records::from_fimg(&fimg,entry.aux() as usize,encoder) {
                    Ok(ans) => Ok(ans),
                    Err(e) => Err(e)
                }
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn write_records(&mut self,path: &str, records: &disk_base::Records) -> Result<usize,Box<dyn std::error::Error>> {
        let encoder = Encoder::new(Some(0x0d));
        if let Ok(fimg) = records.to_fimg(BLOCK_SIZE,true,encoder) {
            return self.write_any(path,&fimg);
        } else {
            Err(Box::new(Error::Syntax))
        }
    }
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(block) => {
                let mut buf: Vec<u8> = vec![0;512];
                if block>=self.blocks.len() {
                    return Err(Box::new(Error::Range));
                }
                self.read_block(&mut buf,block,0);
                Ok((0,buf))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_chunk(&mut self, num: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > 512 || block>=self.blocks.len() {
                    return Err(Box::new(Error::Range));
                }
                self.zap_block(dat,block,0);
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&self,path: &str) -> Result<disk_base::FileImage,Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                return Ok(self.read_file(&entry));
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn write_any(&mut self,path: &str,fimg: &disk_base::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if fimg.chunk_len!=512 {
            eprintln!("chunk length {} is incompatible with ProDOS",fimg.chunk_len);
            return Err(Box::new(Error::Range));
        }
        match self.prepare_to_write(path, &vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree]) {
            Ok((name,dir_key_block,loc,new_key_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(dir_key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),dir_key_block as usize,0);
                // create the entry
                match Entry::create_file(&name,fimg,new_key_block,dir_key_block,None) {
                    Ok(entry) => self.write_entry(&loc,&entry),
                    Err(e) => return Err(Box::new(e))
                }
                // write blocks
                match self.write_file(loc,fimg) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(Box::new(e))
                }
            },
            Err(e) => return Err(Box::new(e))
        }
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
    fn standardize(&self,ref_con: u16) -> Vec<usize> {
        let mut ans: Vec<usize> = Vec::new();
        let mut curr = ref_con;
        while curr>0 {
            let mut dir = self.get_directory(curr as usize);
            let locs = dir.entry_locations(curr);
            ans = [ans,dir.standardize(curr as usize*512)].concat();
            for loc in locs {
                let mut entry = dir.get_entry(&loc);
                let offset = loc.block as usize*512 + 4 + (loc.idx-1)*0x27;
                ans = [ans,entry.standardize(offset)].concat();
                if entry.storage_type()==StorageType::SubDirEntry {
                    ans = [ans,self.standardize(entry.get_ptr())].concat();
                }
            }
            curr = dir.next();
        }
        return ans;
    }
    fn compare(&self,path: &std::path::Path,ignore: &Vec<usize>) {
        let emulator_disk = create_fs_from_file(&path.to_str()
            .expect("could not unwrap path"))
            .expect("could not interpret file system");
        let mut expected = emulator_disk.to_img();
        let mut actual = self.to_img();
        for ignorable in ignore {
            expected[*ignorable] = 0;
            actual[*ignorable] = 0;
        }
        for block in 0..self.blocks.len() {
            for row in 0..16 {
                let mut fmt_actual = String::new();
                let mut fmt_expected = String::new();
                let offset = block*512 + row*32;
                write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
                write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
                assert_eq!(fmt_actual,fmt_expected," at block {}, row {}",block,row)
            }
        }
    }
    fn get_ordering(&self) -> disk_base::DiskImageType {
        return disk_base::DiskImageType::PO;
    }
    fn to_img(&self) -> Vec<u8> {
        let mut result : Vec<u8> = Vec::new();
        for block in &self.blocks {
            for byte in 0..512 {
                result.push(block[byte]);
            }
        }
        return result;
    }
}

#[test]
fn test_path_normalize() {
    let mut disk = Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);
    assert_eq!(disk.normalize_path("DIR1"),["NEW.DISK","DIR1"]);
    assert_eq!(disk.normalize_path("dir1/"),["NEW.DISK","DIR1",""]);
    assert_eq!(disk.normalize_path("dir1/sub2"),["NEW.DISK","DIR1","SUB2"]);
    assert_eq!(disk.normalize_path("/new.disk/dir1/sub2"),["NEW.DISK","DIR1","SUB2"]);
}

