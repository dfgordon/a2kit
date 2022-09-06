//! # ProDOS disk image library
//! This manipulates disk images containing one ProDOS volume.
//! 
//! * Image types: ProDOS ordered images (.PO)
//! * Single volume images only

use a2kit_macro::DiskStruct;
mod boot;
mod types;
mod directory;
use types::*;
use directory::*;


pub struct Disk {
    blocks: Vec<[u8;512]>,
    volume_dir: KeyBlock<VolDirHeader>,
}

impl Disk {
    /// Create an empty disk, all blocks are zero.
    pub fn new(num_blocks: u16) -> Self {
        let mut empty_blocks = Vec::new();
        for _i in 0..num_blocks {
            empty_blocks.push([0;512]);
        }
        Self {
            blocks: empty_blocks,
            volume_dir: KeyBlock::new()
        }
    }
    fn allocate_block(&mut self,iblock: usize) {
        let boff = iblock / 4096; // how many blocks into the map
        let byte = (iblock - 4096*boff) / 8;
        let bit = 7 - (iblock - 4096*boff) % 8;
        let bptr = u16::from_le_bytes(self.volume_dir.header.bitmap_ptr) as usize + boff;
        let mut map = self.blocks[bptr][byte];
        map &= (1 << bit as u8) ^ u8::MAX;
        self.blocks[bptr][byte] = map;
    }
    fn deallocate_block(&mut self,iblock: usize) {
        let boff = iblock / 4096; // how many blocks into the map
        let byte = (iblock - 4096*boff) / 8;
        let bit = 7 - (iblock - 4096*boff) % 8;
        let bptr = u16::from_le_bytes(self.volume_dir.header.bitmap_ptr) as usize + boff;
        let mut map = self.blocks[bptr][byte];
        map |= 1 << bit as u8;
        self.blocks[bptr][byte] = map;
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

    pub fn format(&mut self, vol_name: &String, floppy: bool, time: Option<chrono::NaiveDateTime>) {
        
        // calculate volume parameters and setup volume directory
        let bitmap_blocks = 1 + self.blocks.len() / 4096;
        self.volume_dir.set_links(0, 3);
        self.volume_dir.header.format(self.blocks.len() as u16,vol_name,time);
        let first = u16::from_le_bytes(self.volume_dir.header.bitmap_ptr) as usize;

        // mark all blocks as free
        for b in 0..self.blocks.len() {
            self.deallocate_block(b);
        }

        // mark the bitmap blocks as used
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

        // volume key block

        self.write_block(&self.volume_dir.to_bytes(),2,0);

        // next 3 volume directory blocks

        for b in 3..6 {
            let mut this = EntryBlock::new();
            if b==5 {
                this.set_links(b-1, 0);
            } else {
                this.set_links(b-1, b+1);
            }
            self.write_block(&this.to_bytes(),b as usize,0);
        }
    }
    fn search_entries<T: Header + HasName>(&self,stype: &Vec<StorageType>,name: &String,key: &KeyBlock<T>) -> Option<Entry> {
        let mut buf: Vec<u8> = vec![0;512];
        let file_count = key.header.fileCount();
        let mut next = key.next();
        let mut num_found = 0;
        let mut idx = 0;
        let mut entries = key.entries();
        while num_found < file_count {
            if idx<entries.len() {
                let entry = &entries[idx];
                if entry.is_active() {
                    num_found += 1;
                    if is_file_match::<Entry>(stype,name,entry) {
                        return Some(entries[idx]);
                    }
                }
            }
            if num_found < file_count {
                if idx==entries.len() {
                    if next==0 {
                        return None;
                    }
                    self.read_block(&mut buf, next as usize, 0);
                    let dir = EntryBlock::from_bytes(&buf);
                    idx = 0;
                    next = dir.next();
                    entries = dir.entries();
                } else {
                    idx += 1;
                }
            }
        }
        return None;
    }
    fn search_volume(&self,file_types: &Vec<StorageType>,path: &String) -> Result<Entry,Error> {
        let path_nodes: Vec<&str> = path.split("/").collect();
        let n = path_nodes.len();
        if path_nodes[0]!="" || path_nodes[n-1]=="" {
            return Err(Error::PathNotFound);
        }
        let file_name = path_nodes[n-1].to_string();
        let mut subdirs = Vec::<String>::new();
        for i in 1..n-1 {
            subdirs.push(path_nodes[i].to_string());
        }
        // root and file only
        if n==2 {
            if let Some(entry) = self.search_entries(&file_types, &file_name, &self.volume_dir) {
                return Ok(entry);
            }
        }
        // subdirectory search
        else if let Some(entry) = self.search_entries(&vec![StorageType::SubDirEntry], &subdirs[0], &self.volume_dir) {
            let mut next = u16::from_le_bytes(entry.get_ptr());
            let mut buf: Vec<u8> = vec![0;512];
            for subdir in subdirs[1..].to_vec() {
                self.read_block(&mut buf, next as usize, 0);
                let key = KeyBlock::<SubDirHeader>::from_bytes(&buf);
                if let Some(entry) = self.search_entries(&vec![StorageType::SubDirEntry], &subdir, &key) {
                    next = u16::from_le_bytes(entry.get_ptr());
                } else {
                    return Err(Error::PathNotFound);
                }
            }
            self.read_block(&mut buf, next as usize, 0);
            let key = KeyBlock::<SubDirHeader>::from_bytes(&buf);
            if let Some(entry) = self.search_entries(&file_types, &file_name, &key) {
                return Ok(entry);
            }
        }
        return Err(Error::PathNotFound);
    }
    fn find_file(&self,path: &String) -> Result<Entry,Error> {
        return self.search_volume(&vec![StorageType::Seedling,StorageType::Sapling,StorageType::Tree],path);
    }
    fn get_dir(&self,path: &String) -> Result<Box<dyn Directory>,Error> {
        if path=="/" || path=="" {
            return Ok(Box::new(self.volume_dir));
        }
        if let Ok(entry) = self.search_volume(&vec![StorageType::SubDirEntry], path) {
            let b = u16::from_le_bytes(entry.get_ptr());
            let mut buf: Vec<u8> = vec![0;512];
            self.read_block(&mut buf, b as usize, 0);
            return Ok(Box::new(KeyBlock::<SubDirHeader>::from_bytes(&buf)));
        }
        return Err(Error::PathNotFound);
    }
    /// List directory to standard output, mirrors `CATALOG`
    pub fn catalog_to_stdout(&self,path: &String) -> Result<(),Error> {
        match self.get_dir(path) {
            Ok(dir) => {
                println!("{}",dir.name());
                println!();
                println!(" {:15} {:4} {:6} {:16} {:16} {:7} {:7}","NAME","TYPE","BLOCKS","MODIFIED","CREATED","ENDFILE","SUBTYPE");
                println!();
                for entry in dir.entries() {
                    if entry.is_active() {
                        println!("{}",entry);
                    }
                }
                let mut next = dir.next();
                let mut buf: Vec<u8> = vec![0;512];
                while next>0 {
                    self.read_block(&mut buf,next as usize,0);
                    let next_dir = EntryBlock::from_bytes(&buf);
                    for entry in next_dir.entries() {
                        if entry.is_active() {
                            println!("{}",entry);
                        }
                    }
                    next = next_dir.next();
                }
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    pub fn from_po_img(po_img: &Vec<u8>) -> Result<Self,Error> {
        let block_count = po_img.len()/512;
        if po_img.len()%512 != 0 || block_count > 65535 {
            return Err(Error::EndOfData);
        }
        let mut disk = Self::new(block_count as u16);

        for block in 0..block_count {
            for byte in 0..512 {
                disk.blocks[block][byte] = po_img[byte+block*512];
            }
        }
        disk.volume_dir = KeyBlock::<VolDirHeader>::from_bytes(&disk.blocks[2].to_vec());
        if disk.blocks[0]==boot::FLOPPY_BLOCK0 || disk.blocks[0]==boot::HD_BLOCK0 {
            return Ok(disk);
        }
        return Err(Error::NoDeviceConnected);
    }
    pub fn to_po_img(&self) -> Vec<u8> {
        let mut result : Vec<u8> = Vec::new();
        for block in &self.blocks {
            for byte in 0..512 {
                result.push(block[byte]);
            }
        }
        return result;
    }
}