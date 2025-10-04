//! ## Pascal file system module
//! 
//! This module is *not* for the Pascal language, but rather the Pascal file system.
//! Tested only with UCSD Pascal version 1.2.

pub mod types;
mod boot;
mod directory;
mod pack;

use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;
use num_traits::FromPrimitive;
use types::*;
use pack::*;
use directory::*;
use super::Block;
use crate::img;
use crate::{STDRESULT,DYNERR};

pub const FS_NAME: &str = "a2 pascal";
const IMAGE_TYPES: [img::DiskImageType;5] = [
    img::DiskImageType::DO,
    img::DiskImageType::NIB,
    img::DiskImageType::WOZ1,
    img::DiskImageType::WOZ2,
    img::DiskImageType::DOT2MG
];

/// Load directory structure from a borrowed disk image.
/// This is used to test images, as well as being called during FS operations.
fn get_directory(img: &mut Box<dyn img::DiskImage>) -> Result<Directory,DYNERR> {
    let mut ans = Directory::new();
    let mut buf = img.read_block(Block::PO(VOL_HEADER_BLOCK))?;
    ans.header = VolDirHeader::from_bytes(&buf[0..ENTRY_SIZE])?;
    let beg0 = u16::from_le_bytes(ans.header.begin_block);
    let beg = VOL_HEADER_BLOCK as u16;
    let end = u16::from_le_bytes(ans.header.end_block);
    if beg0!=0 || end<=beg || (end as usize)>ans.total_blocks() {
        log::debug!("bad header: begin block {}, end block {}",beg,end);
        return Err(Box::new(Error::BadFormat));
    }
    // gather up all the directory blocks in a contiguous buffer; this is convenient
    // since the entries are allowed to span 2 blocks.
    buf = Vec::new();
    for iblock in beg..end {
        let mut temp = img.read_block(Block::PO(iblock as usize))?;
        buf.append(&mut temp);
    }
    // create all possible entries whether in use or not
    let max_num_entries = buf.len()/ENTRY_SIZE - 1;
    let mut offset = ENTRY_SIZE;
    for _i in 0..max_num_entries {
        ans.entries.push(DirectoryEntry::from_bytes(&buf[offset..offset+ENTRY_SIZE])?);
        offset += ENTRY_SIZE;
    }
    return Ok(ans);
}

pub fn new_fimg(chunk_len: usize,set_time: bool,name: &str) -> Result<super::FileImage,DYNERR> {
    if !is_name_valid(name, false) {
        return Err(Box::new(Error::BadFormat))
    }
    let modified = match set_time {
        true => pack::pack_date(None).to_vec(),
        false => vec![0;2]
    };
    Ok(super::FileImage {
        fimg_version: super::FileImage::fimg_version(),
        file_system: String::from(FS_NAME),
        fs_type: vec![0;2],
        aux: vec![],
        eof: vec![0;4],
        accessed: vec![],
        created: vec![],
        modified,
        access: vec![],
        version: vec![],
        min_version: vec![],
        chunk_len,
        full_path: name.to_string(),
        chunks: HashMap::new()
    })
}

pub struct Packer {
}

/// The primary interface for disk operations.
pub struct Disk
{
    img: Box<dyn img::DiskImage>
}

impl Disk
{
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    pub fn from_img(img: Box<dyn img::DiskImage>) -> Result<Self,DYNERR> {
        Ok(Self {
            img
        })
    }
    /// Test an image for the Pascal file system.  Changes method to Auto.
    /// Usually called before user parameters are applied.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>) -> bool {
        if !IMAGE_TYPES.contains(&img.what_am_i()) {
            return false;
        }
        log::info!("trying Pascal");
        img.change_method(img::tracks::Method::Auto);
        // test the volume directory header
         match get_directory(img) {
            Ok(directory) => {
                let beg0 = u16::from_le_bytes(directory.header.begin_block);
                let beg = VOL_HEADER_BLOCK as u16;
                let end = u16::from_le_bytes(directory.header.end_block);
                let tot = u16::from_le_bytes(directory.header.total_blocks);
                if beg0!=0 || end<=beg || end>20 {
                    log::debug!("header begin {} end {}",beg0,end);
                    return false;
                }
                // if (tot as usize) != block_count {
                //     log::debug!("header total blocks {}",tot);
                //     return false;
                // }
                if directory.header.name_len>7 || directory.header.name_len==0 {
                    log::debug!("header name length {}",directory.header.name_len);
                    return false;
                }
                if directory.header.file_type != [0,0] {
                    log::debug!("header type {}",u16::from_le_bytes(directory.header.file_type));
                    return false;
                }
                for i in 0..directory.header.name_len {
                    let c = directory.header.name[i as usize];
                    if c<32 || c>126 {
                        log::debug!("header name character {}",c);
                        return false;
                    }
                }
                // test every directory entry that is used
                for i in 0..u16::from_le_bytes(directory.header.num_files) {
                    let entry = directory.entries[i as usize];
                    let ebeg = u16::from_le_bytes(entry.begin_block);
                    let eend = u16::from_le_bytes(entry.end_block);
                    if ebeg>0 {
                        if ebeg<end || eend<=ebeg || (eend as u16) > tot {
                            log::debug!("entry {} begin {} end {}",i,ebeg,eend);
                            return false;
                        }
                        if entry.name_len>15 || entry.name_len==0 {
                            log::debug!("entry {} name length {}",i,entry.name_len);
                            return false;
                        }
                        for j in 0..entry.name_len {
                            let c = entry.name[j as usize];
                            if c<32 || c>126 {
                                log::debug!("entry {} name char {}",i,c);
                                return false;
                            }
                        }
                    }
                }
                return true;
            },
            Err(e) => {
                log::debug!("pascal directory was not readable: {}",e);
                return false;
            }
        }
    }
    fn get_directory(&mut self) -> Result<Directory,DYNERR> {
        get_directory(&mut self.img)
    }
    fn save_directory(&mut self,dir: &Directory) -> STDRESULT {
        let beg = VOL_HEADER_BLOCK as u16;
        let end = u16::from_le_bytes(dir.header.end_block);
        let buf = dir.to_bytes();
        for iblock in beg..end {
            self.write_block(&buf,iblock as usize,(iblock-beg) as usize * BLOCK_SIZE)?;
        }
        Ok(())
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
    fn num_free_blocks(&mut self) -> Result<(u16,u16),DYNERR> {
        let directory = self.get_directory()?;
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
        Ok((free,largest))
    }
    /// Read a block of data into buffer `data` starting at `offset` within the buffer.
    /// Will read as many bytes as will fit in the buffer starting at `offset`.
    fn read_block(&mut self,data: &mut [u8], iblock: usize, offset: usize) -> STDRESULT {
        let bytes = 512;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read block"),
            x if x<=bytes => x,
            _ => bytes
        };
        let buf = self.img.read_block(Block::PO(iblock))?;
        for i in 0..actual_len as usize {
            data[offset + i] = buf[i];
        }
        Ok(())
    }
    /// Writes a block of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the block, trailing bytes are unaffected.
    /// Same as zap since there is no track bitmap in Pascal file system.
    fn write_block(&mut self,data: &[u8], iblock: usize, offset: usize) -> STDRESULT {
        self.zap_block(data,iblock,offset)
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
        self.img.write_block(Block::PO(iblock), &data[offset..offset+actual_len])
    }
    /// Try to find `num` contiguous free blocks.  If found return the first block index.
    fn get_available_blocks(&mut self,num: u16) -> Result<Option<u16>,DYNERR> {
        let directory = self.get_directory()?;
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
                    return Ok(Some(start));
                }
            } else {
                count = 0;
                start = 0;
            }
        }
        return Ok(None);
    }
    /// Format disk for the Pascal file system
    pub fn format(&mut self, vol_name: &str, fill: u8, time: Option<chrono::NaiveDateTime>) -> STDRESULT {
        if !is_name_valid(vol_name, true) {
            log::error!("invalid pascal volume name");
            return Err(Box::new(Error::BadTitle));
        }
        if self.img.nominal_capacity().is_none() {
            return Err(Box::new(Error::BadFormat));
        }
        let num_blocks = self.img.nominal_capacity().unwrap()/512;
        // Zero boot and directory blocks
        for iblock in 0..6 {
            self.write_block(&[0;BLOCK_SIZE],iblock,0)?;
        }
        // Put `fill` value in all remaining blocks
        for iblock in 6..num_blocks {
            self.write_block(&[fill;BLOCK_SIZE],iblock,0)?;
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
        self.write_block(&dir.to_bytes(),VOL_HEADER_BLOCK,0)?;

        // boot loader blocks
        match self.img.kind() {
            img::names::A2_DOS33_KIND => {
                self.write_block(&boot::PASCAL_525_BLOCK0, 0, 0)?;
                self.write_block(&boot::PASCAL_525_BLOCK1, 1, 0)?;
            },
            _ => {
                log::error!("unsupported disk type");
                return Err(Box::new(Error::NoDev))
            }
        }
        return Ok(());
    }

    /// Scan the directory to find the named file and return (Option<entry index>, directory).
    /// N.b. Pascal FS always keeps files in contiguous blocks.
    fn get_file_entry(&mut self,name: &str) -> Result<(Option<usize>,Directory),DYNERR> {
        let directory = self.get_directory()?;
        for i in 0..u16::from_le_bytes(directory.header.num_files) {
            let entry = &directory.entries[i as usize];
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg>0 && end>beg && (end as usize)<directory.total_blocks() {
                if name.to_uppercase() == file_name_to_string(entry.name, entry.name_len) {
                    return Ok((Some(i as usize),directory));
                }
            }
        }
        return Ok((None,directory));
    }
    /// Read any file into the sparse file format.  The fact that the Pascal FS does not
    /// have sparse files presents no difficulty, since `FileImage` is quite general.
    /// As usual we can use `FileImage::sequence` to make the result sequential.
    fn read_file(&mut self,name: &str) -> Result<super::FileImage,DYNERR> {
        if !is_name_valid(name,false) {
            log::error!("invalid pascal filename");
            return Err(Box::new(Error::BadFormat));
        }
        match self.get_file_entry(name)? { (Some(idx),dir) => {
            let entry = &dir.entries[idx];
            let mut ans = new_fimg(BLOCK_SIZE,false,name)?;
            let mut buf = vec![0;BLOCK_SIZE];
            let mut count: usize = 0;
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            let ftype = u16::from_le_bytes(entry.file_type);
            for iblock in beg..end {
                self.read_block(&mut buf, iblock as usize, 0)?;
                ans.chunks.insert(count,buf.clone());
                count += 1;
            }
            ans.fs_type = u16::to_le_bytes(ftype).to_vec();
            ans.eof = u32::to_le_bytes(BLOCK_SIZE as u32*ans.chunks.len() as u32 - u16::from_le_bytes(entry.bytes_remaining) as u32).to_vec();
            ans.modified = entry.mod_date.to_vec();
            return Ok(ans);
        } _ => {
            return Err(Box::new(Error::NoFile));
        }}
    }
    /// Write any file using the sparse file format.  The caller must ensure that the
    /// chunks are sequential (Pascal only supports sequential data).  This is easy:
    /// use `FileImage::desequence` to put sequential data into the sparse file format.
    fn write_file(&mut self,name: &str, fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.chunks.len()==0 {
            log::error!("empty data is not allowed for Pascal file images");
            return Err(Box::new(Error::NoFile));
        }
        if !is_name_valid(name,false) {
            log::error!("invalid pascal filename");
            return Err(Box::new(Error::BadFormat));
        }
        let (maybe_idx,mut dir) = self.get_file_entry(name)?;
        if maybe_idx==None {
            // this is a new file
            // we do not write anything unless there is room
            let data_blocks = fimg.chunks.len();
            let fs_type_usize = fimg.get_ftype();
            let eof_usize = fimg.get_eof();
            if let Some(fs_type) = FileType::from_usize(fs_type_usize) {
                match self.get_available_blocks(data_blocks as u16)? { Some(beg) => {
                    let i = u16::from_le_bytes(dir.header.num_files) as usize;
                    if i < dir.entries.len() {
                        log::debug!("using entry {}",i);
                        dir.entries[i].begin_block = u16::to_le_bytes(beg);
                        dir.entries[i].end_block = u16::to_le_bytes(beg+data_blocks as u16);
                        dir.entries[i].file_type = u16::to_le_bytes(fs_type as u16);
                        dir.entries[i].name_len = name.len() as u8;
                        dir.entries[i].name = string_to_file_name(name);
                        dir.entries[i].bytes_remaining = u16::to_le_bytes((BLOCK_SIZE*data_blocks - eof_usize) as u16);
                        dir.entries[i].mod_date = pack_date(None); // None means use system clock
                        dir.header.num_files = u16::to_le_bytes(u16::from_le_bytes(dir.header.num_files)+1);
                        dir.header.last_access_date = pack_date(None);
                        self.save_directory(&dir)?;
                        for b in 0..data_blocks {
                            if fimg.chunks.contains_key(&b) {
                                self.write_block(&fimg.chunks[&b],beg as usize+b,0)?;
                            } else {
                                log::error!("pascal file image had a hole which is not allowed");
                                return Err(Box::new(Error::BadFormat));
                            }
                        }
                        return Ok(data_blocks);
                    }
                    log::error!("directory is full");
                    return Err(Box::new(Error::NoRoom));
                } _ => {
                    log::error!("not enough contiguous space");
                    return Err(Box::new(Error::NoRoom));
                }}
            } else {
                log::error!("unknown file type");
                return Err(Box::new(Error::BadMode));
            }
        } else {
            log::error!("overwriting is not allowed");
            return Err(Box::new(Error::DuplicateFilename));
        }
    }
    /// Verify that the new name does not already exist
    fn ok_to_rename(&mut self,new_name: &str) -> STDRESULT {
        if !is_name_valid(new_name,false) {
            return Err(Box::new(Error::BadFormat));
        }
        match self.get_file_entry(new_name) {
            Ok((None,_)) => Ok(()),
            Ok(_) => Err(Box::new(Error::DuplicateFilename)),
            Err(e) => Err(e)
        }
    }
    /// modify a file entry, optionally rename, retype.
    fn modify(&mut self,name: &str,maybe_new_name: Option<&str>,maybe_ftype: Option<&str>) -> STDRESULT {
        if !is_name_valid(name, false) {
            return Err(Box::new(Error::BadFormat));
        }
        match self.get_file_entry(name)? { (Some(idx),mut dir) => {
            let entry = &mut dir.entries[idx];
            if let Some(new_name) = maybe_new_name {
                if !is_name_valid(new_name,false) {
                    return Err(Box::new(Error::BadFormat));
                }
                entry.name = string_to_file_name(new_name);
                entry.name_len = new_name.len() as u8;
            }
            if let Some(ftype) = maybe_ftype {
                match FileType::from_str(ftype) {
                    Ok(typ) => entry.file_type = u16::to_le_bytes(typ as u16),
                    Err(e) => return Err(Box::new(e))
                }
            }
            self.save_directory(&dir)?;
            return Ok(());
        } _ => {
            return Err(Box::new(Error::NoFile));
        }}
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self, chunk_len: Option<usize>,set_time: bool,path: &str) -> Result<super::FileImage,DYNERR> {
        match chunk_len {
            Some(l) => new_fimg(l,set_time,path),
            None => new_fimg(BLOCK_SIZE,set_time,path)
        }
    }
    fn stat(&mut self) -> Result<super::Stat,DYNERR> {
        let dir = self.get_directory()?;
        let free_block_tuple = self.num_free_blocks()?;
        Ok(super::Stat {
            fs_name: FS_NAME.to_string(),
            label: vol_name_to_string(dir.header.name,dir.header.name_len),
            users: Vec::new(),
            block_size: BLOCK_SIZE,
            block_beg: 0,
            block_end: dir.total_blocks(),
            free_blocks: free_block_tuple.0 as usize,
            raw: "".to_string()
        })
    }
    fn catalog_to_stdout(&mut self, _path: &str) -> STDRESULT {
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let dir = self.get_directory()?;
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
        let (free,largest) = self.num_free_blocks()?;
        let used = total-free as usize;
        println!("{}/{} files<listed/in-dir>, {} blocks used, {} unused, {} in largest",file_count,expected_count,used,free,largest);
        println!();
        Ok(())
    }
    fn catalog_to_vec(&mut self, path: &str) -> Result<Vec<String>,DYNERR> {
        if path!="/" && path!="" {
            return Err(Box::new(Error::NoFile));
        }
        let mut ans = Vec::new();
        let typ_map: HashMap<u8,&str> = HashMap::from(TYPE_MAP_DISP);
        let dir = self.get_directory()?;
        let total = dir.total_blocks();
        for entry in dir.entries {
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg!=0 && end>beg && (end as usize)<total {
                let name = file_name_to_string(entry.name,entry.name_len);
                let blocks = end - beg;
                let type_as_hex = "$".to_string()+ &hex::encode_upper(vec![entry.file_type[0]]);
                let typ = match typ_map.get(&entry.file_type[0]) {
                    Some(s) => s,
                    None => type_as_hex.as_str()
                };
                ans.push(super::universal_row(typ,blocks as usize,&name));
            }
        }
        Ok(ans)
    }
    fn glob(&mut self,pattern: &str,case_sensitive: bool) -> Result<Vec<String>,DYNERR> {
        let mut ans = Vec::new();
        let glob = match case_sensitive {
            true => globset::Glob::new(pattern)?.compile_matcher(),
            false => globset::Glob::new(&pattern.to_uppercase())?.compile_matcher()
        };
        let dir = self.get_directory()?;
        let total = dir.total_blocks();
        for entry in dir.entries {
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg!=0 && end>beg && (end as usize)<total {
                let name = match case_sensitive {
                    true => file_name_to_string(entry.name, entry.name_len),
                    false => file_name_to_string(entry.name, entry.name_len).to_uppercase()
                };
                if glob.is_match(&name) {
                    ans.push(name);
                }
            }
        }
        Ok(ans)
    }
    fn tree(&mut self,include_meta: bool,indent: Option<u16>) -> Result<String,DYNERR> {
        let dir = self.get_directory()?;
        let total = dir.total_blocks();
        const TIME_FMT: &str = "%Y/%m/%d %H:%M";
        let mut tree = json::JsonValue::new_object();
        tree["file_system"] = json::JsonValue::String(FS_NAME.to_string());
        tree["files"] = json::JsonValue::new_object();
        tree["label"] = json::JsonValue::new_object();
        tree["label"]["name"] = json::JsonValue::String(vol_name_to_string(dir.header.name, dir.header.name_len));
        tree["label"]["time_created"] = json::JsonValue::String(unpack_date(dir.header.last_set_date).format(TIME_FMT).to_string());
        tree["label"]["time_modified"] = json::JsonValue::String(unpack_date(dir.header.last_set_date).format(TIME_FMT).to_string());
        for entry in dir.entries {
            let beg = u16::from_le_bytes(entry.begin_block);
            let end = u16::from_le_bytes(entry.end_block);
            if beg!=0 && end>beg && (end as usize)<total {
                let key = file_name_to_string(entry.name, entry.name_len);
                tree["files"][&key] = json::JsonValue::new_object();
                // file nodes must have no files object at all
                if include_meta {
                    let blocks = (end - beg) as usize;
                    let bytes = blocks*BLOCK_SIZE + u16::from_le_bytes(entry.bytes_remaining) as usize - BLOCK_SIZE;
                        tree["files"][&key]["meta"] = json::JsonValue::new_object();
                    let meta = &mut tree["files"][&key]["meta"];
                    meta["type"] = json::JsonValue::String(hex::encode_upper(entry.file_type.to_vec()));
                    meta["eof"] = json::JsonValue::Number(bytes.into());
                    if entry.mod_date!=[0,0] {
                        meta["time_modified"] = json::JsonValue::String(unpack_date(entry.mod_date).format(TIME_FMT).to_string());
                    }
                    meta["blocks"] = json::JsonValue::Number(blocks.into());
                }
            }
        }
        if let Some(spaces) = indent {
            Ok(json::stringify_pretty(tree, spaces))
        } else {
            Ok(json::stringify(tree))
        }
    }
    fn create(&mut self,_path: &str) -> STDRESULT {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn delete(&mut self,name: &str) -> STDRESULT {
        match self.get_file_entry(name)? { (Some(idx),mut dir) => {
            for i in idx..dir.entries.len() {
                if i+1 < dir.entries.len() {
                    dir.entries[i] = dir.entries[i+1];
                } else {
                    // probably unnecessary
                    dir.entries[i].begin_block = [0,0];
                    dir.entries[i].end_block = [0,0];
                }
            }
            dir.header.num_files = u16::to_le_bytes(u16::from_le_bytes(dir.header.num_files)-1);
            self.save_directory(&dir)?;
            return Ok(());
        } _ => {
            return Err(Box::new(Error::NoFile));
        }}
    }
    fn set_attrib(&mut self,_path: &str,_permissions: crate::fs::Attributes,_password: Option<&str>) -> STDRESULT {
        log::error!("pascal does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn rename(&mut self,old_name: &str,new_name: &str) -> STDRESULT {
        self.ok_to_rename(new_name)?;
        self.modify(old_name,Some(new_name),None)
    }
    fn retype(&mut self,name: &str,new_type: &str,_sub_type: &str) -> STDRESULT {
        self.modify(name, None, Some(new_type))
    }
    fn get(&mut self,name: &str) -> Result<super::FileImage,DYNERR> {
        self.read_file(name)
    }
    fn put(&mut self,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!=FS_NAME {
            log::error!("cannot write {} file image to a2 pascal",fimg.file_system);
            return Err(Box::new(Error::DevErr));
        }
        if fimg.chunk_len!=BLOCK_SIZE {
            log::error!("chunk length {} is incompatible with Pascal",fimg.chunk_len);
            return Err(Box::new(Error::DevErr));
        }
        self.write_file(&fimg.full_path,fimg)
    }
    fn read_block(&mut self,num: &str) -> Result<Vec<u8>,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => self.img.read_block(Block::PO(block)),
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        match usize::from_str(num) {
            Ok(block) => {
                if dat.len() > BLOCK_SIZE {
                    return Err(Box::new(Error::DevErr));
                }
                match self.img.write_block(Block::PO(block), dat) {
                    Ok(()) => Ok(dat.len()),
                    Err(e) => Err(e)
                }
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&mut self,_ref_con: u16) -> HashMap<Block,Vec<usize>> {
        // want to ignore dates, these occur at offest 18 and 20 in the header,
        // and at offset 24 in every entry.  Also ignore unused name bytes.
        // Also ignore unused directory entries entirely.
        let mut ans: HashMap<Block,Vec<usize>> = HashMap::new();
        let dir = self.get_directory().expect("could not get directory");
        let beg = VOL_HEADER_BLOCK; // begin_block points to boot block
        super::add_ignorable_offsets(&mut ans,Block::PO(beg),vec![18,19,20,21]);
        let vol_name_len = dir.header.name_len as usize;
        super::add_ignorable_offsets(&mut ans,Block::PO(beg),(7+vol_name_len..14).collect());
        // header is done, now do entries
        let mut aeoffset = dir.header.len();
        let num_files = u16::from_le_bytes(dir.header.num_files) as usize;
        for idx in 0..dir.entries.len() {
            // form vector of absolute offsets into the directory
            let aoffsets = match idx {
                _i if _i >= num_files => (aeoffset..aeoffset+ENTRY_SIZE).collect(),
                _ => {
                    let datestamp = vec![aeoffset+24, aeoffset+25];
                    let mut nlen = dir.entries[idx].name_len as usize;
                    if nlen > 15 {
                        nlen = 15;
                    }
                    let namebytes = (aeoffset+7+nlen..aeoffset+22).collect();
                    [datestamp,namebytes].concat()
                }
            };
            // work out the block and relative offset for each byte
            for aoff in aoffsets {
                let key = Block::PO(beg+aoff/BLOCK_SIZE);
                let val = aoff%BLOCK_SIZE;
                super::add_ignorable_offsets(&mut ans, key, vec![val]);
            }
            aeoffset += ENTRY_SIZE;
        }
        return ans;
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap(),None).expect("read error");
        let dir = self.get_directory().expect("could not get directory");
        for block in 0..dir.total_blocks() {
            let addr = Block::PO(block);
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