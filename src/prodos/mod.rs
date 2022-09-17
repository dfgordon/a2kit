//! # ProDOS disk image library
//! This manipulates disk images containing one ProDOS volume.
//! 
//! * Image types: ProDOS ordered images, DOS ordered images (.DO,.PO,.DSK)
//! * Single volume images only

use a2kit_macro::DiskStruct;
use std::str::FromStr;
use colored::*;
mod boot;
mod disk525;
pub mod types;
mod directory;
use types::*;
use directory::*;
use crate::disk_base;
use crate::applesoft;

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
    fn write_block(&mut self,data: &Vec<u8>, iblock: usize, offset: usize) {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write block"),
            x if x<=bytes => x,
            _ => bytes
        };
        for i in 0..actual_len as usize {
            self.blocks[iblock][i] = data[offset + i];
        }
        // update the volume directory's bitmap
        self.allocate_block(iblock);
    }
    fn get_available_block(&self) -> Option<u16> {
        for block in 0..self.blocks.len() {
            if self.is_block_free(block) {
                return Some(block as u16);
            }
        }
        return None;
    }

    pub fn format(&mut self, vol_name: &String, floppy: bool, time: Option<chrono::NaiveDateTime>) {
        
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
    /// Given an entry location get the entry from disk
    fn read_entry(&self,loc: &EntryLocation) -> Entry {
        let dir = self.get_directory(loc.block as usize);
        return dir.entry_copies()[loc.idx0];
    }
    /// Given a modified entry and location, write the change to disk.
    /// Any other unsaved changes in the block are lost.  Maybe this should go away.
    fn write_entry(&mut self,loc: &EntryLocation,entry: &Entry) {
        let mut dir = self.get_directory(loc.block as usize);
        dir.set_entry(loc.idx0,*entry);
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
                    return Ok(EntryLocation { block: avail, idx0: 0, idxv: 1});
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
        let key = self.get_directory(key_block as usize);
        let mut buf: Vec<u8> = vec![0;512];
        let mut curr: EntryLocation = EntryLocation {block: key_block, idx0: 0, idxv: 2};
        let mut next = key.next();
        let mut entries = key.entry_copies();
        for _try in 0..100 {
            for entry in &entries {
                if !entry.is_active() {
                    return Ok(curr);
                }
                curr.idx0 += 1;
                curr.idxv += 1;
            }
            if next==0 {
                if let Some(mut parent_loc) = key.parent_entry_loc() {
                    let parent_dir = self.get_directory(parent_loc.block as usize);
                    parent_loc.idx0 = parent_loc.idxv - parent_dir.idx_offset();
                    return match self.expand_directory(&parent_loc) {
                        Ok(loc) => Ok(loc),
                        Err(e) => Err(e)
                    }
                } else {
                    // this is the volume directory which we cannot expand
                    return Err(Error::DirectoryFull);
                }
            } else {
                self.read_block(&mut buf, next as usize, 0);
                let dir = EntryBlock::from_bytes(&buf);
                curr.block = next;
                curr.idx0 = 0;
                curr.idxv = 1;
                next = dir.next();
                entries = dir.entry_copies();
            }
        }
        panic!("too many blocks for this directory, disk likely damaged");
    }
    // Find specific entry in directory with the given key block
    fn search_entries(&self,stype: &Vec<StorageType>,name: &String,key_block: u16) -> Option<EntryLocation> {
        let key = self.get_directory(key_block as usize);
        let file_count = key.file_count();
        let mut curr = EntryLocation {block: key_block,idx0: 0,idxv: 2};
        let mut next = key.next();
        let mut num_found = 0;
        let mut entries = key.entry_copies();
        while num_found < file_count {
            for idx in 0..entries.len() {
                let entry = &entries[idx];
                if entry.is_active() {
                    num_found += 1;
                    if is_file_match::<Entry>(stype,name,entry) {
                        return Some(curr);
                    }
                }
                curr.idx0 += 1;
                curr.idxv += 1;
            }
            if next==0 {
                return None;
            }
            let dir = self.get_directory(next as usize);
            curr.block = next;
            curr.idx0 = 0;
            curr.idxv = 1;
            next = dir.next();
            entries = dir.entry_copies();
        }
        return None;
    }
    /// put path as [volume,subdir,subdir,...,last] where last could be an empty string,
    /// which indicates this is a directory.  If last is not empty, it could be either directory or file.
    fn normalize_path(&self,path: &String) -> Vec<String> {
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
    fn search_volume(&self,file_types: &Vec<StorageType>,path: &String) -> Result<EntryLocation,Error> {
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
    fn find_file(&self,path: &String) -> Result<EntryLocation,Error> {
        return self.search_volume(&vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree],path);
    }
    /// Get the directory as a tuple (block ptr,Directory trait object)
    fn get_dir_key_block(&self,path: &String) -> Result<u16,Error> {
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
    fn process_index_block(&self,entry: &Entry,index_ptr: u16,buf: &mut Vec<u8>,dat: &mut SparseFileData,progress: &mut usize) {
        self.read_block(buf,index_ptr as usize,0);
        let index_block = buf.clone();
        for idx in 0..256 {
            let ptr = u16::from_le_bytes([index_block[idx],index_block[idx+256]]);
            let mut bytes = 512;
            if *progress + bytes > entry.eof() {
                bytes = entry.eof() - *progress;
            }
            dat.index.push(ptr);
            if ptr>0 {
                self.read_block(buf,ptr as usize,0);
                dat.map.insert(ptr,buf[0..bytes].to_vec());
            }
            *progress += bytes;
        }
    }
    /// Read any file into the sparse file format.  Use `SparseFileData.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&self,entry: &Entry) -> types::SparseFileData {
        let mut dat = SparseFileData::new();
        let mut buf: Vec<u8> = vec![0;512];
        let master_ptr = entry.get_ptr();
        let mut progress: usize = 0;
        match entry.storage_type() {
            StorageType::Seedling => {
                self.read_block(&mut buf, master_ptr as usize, 0);
                dat.index.push(master_ptr);
                dat.map.insert(master_ptr, buf[0..entry.eof()].to_vec());
                return dat;
            },
            StorageType::Sapling => {
                self.process_index_block(&entry, master_ptr, &mut buf, &mut dat, &mut progress);
                return dat;
            },
            StorageType::Tree => {
                // TODO: find out how the master block is packed
                self.read_block(&mut buf,master_ptr as usize,0);
                let master_block = buf.clone();
                for idx in 0..256 {
                    let ptr = u16::from_le_bytes([master_block[idx],master_block[idx+256]]);
                    if ptr>0 {
                        self.process_index_block(entry, ptr, &mut buf, &mut dat, &mut progress);
                    }
                }
                return dat;
            }
            _ => panic!("cannot read file of this type")
        }
    }
    /// Prepare a directory for a new file or subdirectory.  This will modify the disk only if the directory needs to grow.
    fn prepare_to_write(&mut self,path: &String,types: &Vec<StorageType>) -> Result<(String,u16,EntryLocation,u16),Error> {
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
        if let Ok(key_block) = self.get_dir_key_block(&subdir_path) {
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
    /// Write any sparse or sequential file.  Use `SparseFileData::desequence` to put sequential data
    /// into the sparse file format, with no loss of generality.
    /// The entry must already exist and point to the next available block.
    fn write_file(&mut self,loc: EntryLocation,dat: &SparseFileData) -> Result<usize,Error> {
        let mut storage = StorageType::Seedling;
        let mut master_buf: Vec<u8> = vec![0;512];
        let mut master_ptr: u16 = 0;
        let mut master_count: u16 = 0;
        let mut index_buf: Vec<u8> = vec![0;512];
        let mut index_ptr: u16 = 0;
        let mut index_count: u16 = 0;
        let mut eof: usize = 0;
        let dir = self.get_directory(loc.block as usize);
        let mut entry = dir.entry_copies()[loc.idx0];

        for count in 0..dat.index.len() {

            let shadow_block = dat.index[count];
            let buf_maybe = dat.map.get(&shadow_block);

            if master_count > 127 {
                return Err(Error::DiskFull);
            }

            // Check that enough free blocks are available to proceed with this stage
            let mut blocks_needed = match storage {
                StorageType::Seedling => {
                    match count {
                        i if i==0 => 1,
                        _ => 2
                    }
                },
                StorageType::Sapling => {
                    match index_count {
                        i if i<256 => 1,
                        _ => 3
                    }
                },
                StorageType::Tree => {
                    match index_count {
                        i if i<256 => 1,
                        _ => 2
                    }
                }
                _ => panic!("unexpected storage type during write")
            };
            if buf_maybe==None {
                blocks_needed -= 1;
            }
            if blocks_needed > self.num_free_blocks() {
                return Err(Error::DiskFull);
            }
            
            // promote the storage type if necessary
            match storage {
                StorageType::Seedling => {
                    if count>0 {
                        index_ptr = self.get_available_block().unwrap();
                        self.allocate_block(index_ptr as usize);
                        pack_index_ptr(&mut index_buf,entry.get_ptr(),0);
                        index_count = 1;
                        storage = StorageType::Sapling;
                        entry.change_storage_type(storage);
                        entry.set_ptr(index_ptr);
                        entry.delta_blocks(1);
                    }
                },
                StorageType::Sapling => {
                    if index_count > 255 {
                        master_ptr = self.get_available_block().unwrap();
                        self.allocate_block(master_ptr as usize);
                        pack_index_ptr(&mut master_buf,index_ptr,0);
                        master_count = 1;
                        index_ptr = self.get_available_block().unwrap();
                        self.allocate_block(index_ptr as usize);
                        index_count = 0;
                        index_buf = vec![0;512];
                        storage = StorageType::Tree;
                        entry.change_storage_type(storage);
                        entry.set_ptr(master_ptr);
                        entry.delta_blocks(2);
                    }
                },
                StorageType::Tree => {
                    if index_count > 255 {
                        master_count += 1;
                        index_ptr = self.get_available_block().unwrap();
                        self.allocate_block(index_ptr as usize);
                        index_count = 0;
                        index_buf = vec![0;512];
                        entry.delta_blocks(1);
                    }
                },
                _ => panic!("unexpected storage type during write")
            }

            // write the data block
            let curr = self.get_available_block().unwrap();
            let mut curr_or_none = 0;
            // It is up to the creator of SparseFileData to decide if the first block is treated as empty
            if let Some(buf) = buf_maybe {
                self.write_block(&buf,curr as usize,0);
                eof += buf.len();
                entry.delta_blocks(1);
                curr_or_none = curr;
            } else {
                eof += 512;
            }
            entry.set_eof(eof);

            // write the index data
            match storage {
                StorageType::Seedling => {
                    // no index data
                }
                StorageType::Sapling => {
                    pack_index_ptr(&mut index_buf, curr_or_none, index_count as usize);
                    self.write_block(&index_buf, index_ptr as usize, 0);
                    index_count += 1;
                },
                StorageType::Tree => {
                    pack_index_ptr(&mut master_buf, index_ptr, master_count as usize);
                    pack_index_ptr(&mut index_buf, curr_or_none, index_count as usize);
                    self.write_block(&master_buf,master_ptr as usize,0);
                    self.write_block(&index_buf,index_ptr as usize,0);
                    index_count += 1;
                },
                _ => panic!("unexpected storage type during write")
            }

            // update the entry, do last to capture storage type changes
            self.write_entry(&loc,&entry);
        }
        return Ok(eof);
    }
    /// write a general sparse file
    pub fn write_sparse(&mut self,path: &String, dat: &SparseFileData, ftype: FileType, aux: u16) -> Result<usize,Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                self.write_entry(&loc,&Entry::create_file(&name, ftype, aux, new_block, key_block, None));
                // write blocks
                match self.write_file(loc,dat) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(Box::new(e))
                }
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    /// Return a disk object if the image data verifies as DOS ordered,
    /// otherwise return IOError.  N.b. `.dsk` images are often DOS
    /// ordered even if the image contains a ProDOS volume.
    /// Only 280 block (5.25 inch floppy) images are accepted.
    pub fn from_do_img(dimg: &Vec<u8>) -> Option<Self> {
        let block_count = dimg.len()/512;
        if dimg.len()%512 != 0 || block_count != 280 {
            return None;
        }
        let mut disk = Self::new(block_count as u16);

        for block in 0..block_count {
            let ([t1,s1],[t2,s2]) = disk525::ts_from_block(block as u16);
            for byte in 0..256 {
                disk.blocks[block][byte] = dimg[t1 as usize*4096 + s1 as usize*256 + byte];
                disk.blocks[block][byte+256] = dimg[t2 as usize*4096 + s2 as usize*256 + byte];
            }
        }
        if disk.blocks[0]==boot::FLOPPY_BLOCK0 || disk.blocks[0]==boot::HD_BLOCK0 {
            return Some(disk);
        }
        return None;
    }
    /// Return a disk object if the image data verifies as ProDOS ordered,
    /// otherwise return IOError.  The image is allowed to be any integral
    /// number of blocks up to the maximum of 65535.
    pub fn from_po_img(dimg: &Vec<u8>) -> Option<Self> {
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
        if disk.blocks[0]==boot::FLOPPY_BLOCK0 || disk.blocks[0]==boot::HD_BLOCK0 {
            return Some(disk);
        }
        return None;
    }
}

impl disk_base::A2Disk for Disk {
    fn catalog_to_stdout(&self, path: &String) {
        match self.get_dir_key_block(path) {
            Ok(b) => {
                let dir = self.get_directory(b as usize);
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
                for entry in dir.entry_copies() {
                    if entry.is_active() {
                        println!("{}",entry);
                    }
                }
                let mut next = dir.next();
                let mut buf: Vec<u8> = vec![0;512];
                while next>0 {
                    self.read_block(&mut buf,next as usize,0);
                    let next_dir = EntryBlock::from_bytes(&buf);
                    for entry in next_dir.entry_copies() {
                        if entry.is_active() {
                            println!("{}",entry);
                        }
                    }
                    next = next_dir.next();
                }
                println!();
                let total = self.blocks.len();
                let mut free = 0;
                for i in 0..total {
                    if self.is_block_free(i) {
                        free += 1;
                    }
                }
                let used = total-free;
                println!("BLOCKS FREE: {}  BLOCKS USED: {}  TOTAL BLOCKS: {}",free,used,total);
                println!();
            }
            Err(e) => panic!("{}",e)
        }
    }
    fn create(&mut self,path: &String,time: Option<chrono::NaiveDateTime>) -> Result<(),Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::SubDirEntry]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                let mut entry = Entry::create_subdir(&name,new_block,key_block,time);
                entry.delta_blocks(1);
                entry.set_eof(512);
                self.write_entry(&loc,&entry);
                // write the new directory's key block
                let mut subdir = KeyBlock::<SubDirHeader>::new();
                subdir.header.create(&name,loc.block,loc.idxv as u8,time);
                self.write_block(&subdir.to_bytes(),new_block as usize,0);
                Ok(())
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn bload(&self,path: &String) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                Ok((entry.aux(),ans.sequence()))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn bsave(&mut self,path: &String, dat: &Vec<u8>,start_addr: u16) -> Result<usize,Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                self.write_entry(&loc,&Entry::create_file(&name, FileType::Binary,start_addr,new_block, key_block, None));
                // write blocks
                match self.write_file(loc,&SparseFileData::desequence(dat)) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(Box::new(e))
                }
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn load(&self,path: &String) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                return Ok((0,ans.sequence()));
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn save(&mut self,path: &String, dat: &Vec<u8>, typ: disk_base::ItemType) -> Result<usize,Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                match typ {
                    disk_base::ItemType::ApplesoftTokens => {
                        let addr = applesoft::deduce_address(dat);
                        self.write_entry(&loc,&Entry::create_file(&name, FileType::ApplesoftCode,addr,new_block, key_block, None));
                    },
                    disk_base::ItemType::IntegerTokens => {
                        self.write_entry(&loc,&Entry::create_file(&name, FileType::IntegerCode,0,new_block, key_block, None));
                    }
                    _ => panic!("cannot write this type of program file")
                }
                // write blocks
                match self.write_file(loc,&SparseFileData::desequence(dat)) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(Box::new(e))
                }
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn read_text(&self,path: &String) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.find_file(path) {
            Ok(loc) => {
                let entry = self.read_entry(&loc);
                let ans = self.read_file(&entry);
                return Ok((0,ans.sequence()));
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_text(&mut self,path: &String, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        match self.prepare_to_write(path, &vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree]) {
            Ok((name,key_block,loc,new_block)) => {
                // update the file count in the parent key block
                let mut dir = self.get_directory(key_block as usize);
                dir.inc_file_count();
                self.write_block(&dir.to_bytes(),key_block as usize,0);
                // write the entry into the parent directory (may not be key block)
                self.write_entry(&loc,&Entry::create_file(&name, FileType::Text,0,new_block, key_block, None));
                // write blocks
                match self.write_file(loc,&SparseFileData::desequence(dat)) {
                    Ok(len) => Ok(len),
                    Err(e) => Err(Box::new(e))
                }
            },
            Err(e) => return Err(Box::new(e))
        }
    }
    fn decode_text(&self,dat: &Vec<u8>) -> String {
        let file = types::SequentialText::pack(&dat);
        return file.to_string();
    }
    fn encode_text(&self,s: &String) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&mut self,ref_con: u16) {
        let mut curr = ref_con;
        while curr>0 {
            let mut dir = self.get_directory(curr as usize);
            let mut entries = dir.entry_copies();
            dir.standardize();
            for i in 0..entries.len() {
                entries[i].standardize();
                if entries[i].storage_type()==StorageType::SubDirEntry {
                    self.standardize(entries[i].get_ptr());
                }
                dir.set_entry(i, entries[i]);
            }
            self.write_block(&dir.to_bytes(),curr as usize,0);
            curr = dir.next();
        }
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
    assert_eq!(disk.normalize_path(&"DIR1".to_string()),["NEW.DISK","DIR1"]);
    assert_eq!(disk.normalize_path(&"dir1/".to_string()),["NEW.DISK","DIR1",""]);
    assert_eq!(disk.normalize_path(&"dir1/sub2".to_string()),["NEW.DISK","DIR1","SUB2"]);
    assert_eq!(disk.normalize_path(&"/new.disk/dir1/sub2".to_string()),["NEW.DISK","DIR1","SUB2"]);
}

