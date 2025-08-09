//! # File System Module
//! 
//! File system modules handle interactions with directories and files.  There is a sub-module for
//! each supported file system.  File systems are represented by the `DiskFS` trait.  The trait object takes ownership of
//! some disk image, which it uses as storage.
//! 
//! Files are represented by a `FileImage`
//! object.  This is a low level representation of the file that works for any of the supported
//! file systems.  File image data is processed through the `Packing` trait.  There are
//! convenience functions in `DiskFS` such as `read_text`, which gets a file image and unpacks it
//! as text, or `write_text`, which packs text into a file image and puts it.
//! 
//! This module also contains the `Block` enumeration, which specifies and locates allocation units.
//! The enumeration names the file system's allocation system, and its value is a specific block.
//! The value can take any form, e.g., DOS blocks are 2-element lists with [track,sector], whereas
//! CPM blocks are 3-tuples with (block,BSH,OFF).
//! 
//! Sector skews are not handled here.  Transformation of a `Block` to a physical disk address is
//! handled within the `img` module.  Transformations that go between a file system and a disk,
//! such as sector skews, are kept in the `bios` module.

pub mod dos3x;
pub mod prodos;
pub mod pascal;
pub mod cpm;
pub mod fat;
mod fimg;
mod recs;

use std::fmt;
use std::collections::HashMap;
use crate::img;
use crate::commands::ItemType;
use crate::{STDRESULT,DYNERR};

/// Enumerates file system errors.  The `Display` trait will print equivalent long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("file system not compatible with request")]
    FileSystemMismatch,
    #[error("file image format is wrong")]
    FileImageFormat,
    #[error("incompatible or ill-formed version")]
    UnexpectedVersion,
    #[error("high level file format is wrong")]
    FileFormat
}

pub enum UnpackedData {
    Binary(Vec<u8>),
    Records(Records),
    Text(String)
}

/// Encapsulates the disk address and addressing mode used by a file system.
/// Disk addresses generally involve some transformation between logical (file system) and physical (disk fields) addresses.
/// Disk images are responsible for serving blocks in response to a file system request, see the 'img' docstring for more.
/// The `Block` implementation includes a simple mapping from blocks to sectors; disk images can use this or not as appropriate.
#[derive(PartialEq,Eq,Clone,Copy,Hash)]
pub enum Block {
    /// value is [track,sector]
    D13([usize;2]),
    /// value is [track,sector]
    DO([usize;2]),
    /// value is block number
    PO(usize),
    /// value is (absolute block number, BSH, OFF); see cpm::types
    CPM((usize,u8,u16)),
    /// value is (first logical sector,num sectors)
    FAT((u64,u8))
}

impl Block {
    /// Get a track-sector list for this block.
    /// At this level we can only assume a simple monotonically increasing relationship between blocks and sectors.
    /// Any further skewing must be handled by the caller.  CP/M and FAT offsets are accounted for.
    /// For CP/M be sure to use 128 byte logical sectors when computing `secs_per_track`.
    pub fn get_lsecs(&self,secs_per_track: usize) -> Vec<[usize;2]> {
        match self {
            Self::D13([t,s]) => vec![[*t,*s]],
            Self::DO([t,s]) => vec![[*t,*s]],
            Self::PO(_) => panic!("function `get_lsecs` not appropriate for ProDOS"),
            Self::CPM((block,bsh,off)) => {
                let mut ans: Vec<[usize;2]> = Vec::new();
                let lsecs_per_block = 1 << bsh;
                for sec_count in block*lsecs_per_block..(block+1)*lsecs_per_block {
                    ans.push([*off as usize + sec_count/secs_per_track , 1 + sec_count%secs_per_track]);
                }
                ans
            },
            Self::FAT((sec1,secs)) => {
                let mut ans: Vec<[usize;2]> = Vec::new();
                for sec in (*sec1 as usize)..(*sec1 as usize)+(*secs as usize) {
                    ans.push([sec/secs_per_track , sec%secs_per_track]);
                }
                ans
            }
        }
    }
}
impl fmt::Display for Block {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13([t,s]) => write!(f,"D13 track {} sector {}",t,s),
            Self::DO([t,s]) => write!(f,"DOS track {} sector {}",t,s),
            Self::PO(b) => write!(f,"ProDOS block {}",b),
            Self::CPM((b,s,o)) => write!(f,"CPM block {} shift {} offset {}",b,s,o),
            Self::FAT((s1,secs)) => write!(f,"FAT cluster sec1 {} secs {}",s1,secs)
        }
    }
}

/// Testing aid, adds offsets to the existing key, or create a new key if needed
pub fn add_ignorable_offsets(map: &mut HashMap<Block,Vec<usize>>,key: Block, offsets: Vec<usize>) {
    if let Some(val) = map.get(&key) {
        map.insert(key,[val.clone(),offsets].concat());
    } else {
        map.insert(key,offsets);
    }
}

/// Testing aid, combines offsets from two maps (used to fold in subdirectory offsets)
pub fn combine_ignorable_offsets(map: &mut HashMap<Block,Vec<usize>>,other: HashMap<Block,Vec<usize>>) {
    for (k,v) in other.iter() {
        add_ignorable_offsets(map, *k, v.clone());
    }
}

/// Unpacking data as text in a2kit almost always "succeeds", because unknown codes are simply
/// replaced with ASCII NULL.  This function judges the quality of the string by forming the
/// ratio of NULL occurrences to total length (0 is good, 1 is bad).
pub fn null_fraction(candidate: &str) -> f64 {
    let mut null_count = 0;
    for c in candidate.chars() {
        if c == '\u{0000}' {
            null_count += 1;
        }
    }
    if null_count > 0 {
        log::warn!("string had {} NULL (there may have been a lossy conversion)",null_count);
    }
    null_count as f64 / candidate.len() as f64
}

fn universal_row(typ: &str, blocks: usize, name: &str) -> String {
    format!("{:4} {:5}  {}",typ,blocks,name)
}

pub trait TextConversion {
    fn new(line_terminator: Vec<u8>) -> Self;
    /// Typical implementations will return Some(Vec) only if
    /// the string slice is pure ASCII.
    fn from_utf8(&self,txt: &str) -> Option<Vec<u8>>;
    /// Typical implementations will return Some(String) always.
    /// If `src` has something out of bounds, it
    /// will be replaced with ASCII NULL.  Consumers can then judge
    /// the result by calling `null_fraction`.
    fn to_utf8(&self,src: &[u8]) -> Option<String>;
    /// Does a given slice end with another slice
    fn is_terminated(bytes: &[u8],term: &[u8]) -> bool {
        if term.len()==0 {
            return true;
        }
        if bytes.len()==0 || bytes.len() < term.len() {
            return false;
        }
        for i in 0..term.len() {
            if bytes[i+bytes.len()-term.len()]!=term[i] {
                return false;
            }
        }
        true
    }
}

/// This is an abstraction of a file that must work for any supported file system.
/// In particular, it needs to capture all possible attibutes of a file, such
/// as sparse structure and metadata.  Metadata items are represented by a Vec<u8>
/// that contains the same byte ordering that is stored on disk.  In the JSON representation
/// these become hex strings.  The `DiskFS` is responsible for further interpretation.
/// The data itself is stored in a map with a numerical chunk id as the key, and a Vec<u8>
/// as the chunk data.  The JSON representation uses decimal strings for the key and hex
/// strings for the data.  If it is important to sort the chunk map, don't forget to convert
/// the decimal strings to number types first.
/// 
/// Each `DiskFS` trait object provides its own routine for creating an empty file image.
/// Buffer sizes should be set as appropriate for that FS.
/// Unused metadata can be represented by an empty vector.
pub struct FileImage {
    /// Version of the file image format, such as "2.0.0"
    pub fimg_version: String,
    /// UTF8 string naming the file system
    pub file_system: String,
    /// length of a chunk
    pub chunk_len: usize,
    /// length of the file were it serialized
    pub eof: Vec<u8>,
    /// file type, encoding varies by file system
    pub fs_type: Vec<u8>,
    /// auxiliary file information, encoding varies by file system
    pub aux: Vec<u8>,
    /// The access control bits, encoding varies by file system
    pub access: Vec<u8>,
    /// The time last accessed, encoding varies by file system
    pub accessed: Vec<u8>,
    /// The time created, encoding varies by file system
    pub created: Vec<u8>,
    /// The time last modified, encoding varies by file system
    pub modified: Vec<u8>,
    /// Some version
    pub version: Vec<u8>,
    /// Some minimum version
    pub min_version: Vec<u8>,
    /// full path, whether of the origin or intended destination, can be empty string
    pub full_path: String,
    /// The key is an ordered chunk number starting at 0, no relation to any disk location.
    /// Contraints on the length of the data are undefined at this level.
    pub chunks: HashMap<usize,Vec<u8>>
}

/// Trait implemented by the `Packer` delegates of `FileImage`.
/// Usually not called directly, because `FileImage` provides wrappers.
pub trait Packing {
    /// Check syntax and set the path for this file image
    fn set_path(&self,fimg: &mut FileImage,path: &str) -> STDRESULT;
    /// Get load address for this file image, if applicable.
    fn get_load_address(&self,fimg: &FileImage) -> usize;
    /// automatically select a packing strategy by analyzing the data
    fn pack(&self,_fimg: &mut FileImage, _dat: &[u8], _load_addr: Option<usize>) -> STDRESULT {
        log::error!("could not automatically pack");
        Err(Box::new(crate::fs::Error::FileFormat))
    }
    /// automatically select an unpacking strategy based on the file image metadata
    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR>;
    /// Pack raw byte stream into file image.
    /// Headers used by the file system are *not* automatically inserted.
    /// If the file system has explicit typing, the type is set to text.
    fn pack_raw(&self, fimg: &mut FileImage, dat: &[u8]) -> STDRESULT;
    /// Get the raw bytestream, including any header used by the file system.
    /// The byte stream will extend to end of block unless `trunc==true`.
    /// Setting `trunc==true` only works if the EOF is stored in the directory.
    fn unpack_raw(&self,fimg: &FileImage,trunc: bool) -> Result<Vec<u8>,DYNERR>;
    /// Pack bytes into file image, if file system uses a header it is added.
    /// The load address will be checked for validity, if not used by FS it must be None.
    fn pack_bin(&self,fimg: &mut FileImage,dat: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> STDRESULT;
    /// get bytes from file image, if file system uses a header it is stripped
    fn unpack_bin(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR>;
    /// Convert UTF8 with either LF or CRLF to the file system's text format.  This returns an error
    /// if the conversion would result in any loss of data.
    fn pack_txt(&self, fimg: &mut FileImage, txt: &str) -> STDRESULT;
    /// Convert the file system's text format to UTF8 with LF.  This always succeeds because the underlying
    /// text converters will replace unknown characters with ASCII NULL.
    fn unpack_txt(&self,fimg: &FileImage) -> Result<String,DYNERR>;
    /// pack language tokens into file image, if file system uses a header it is added
    fn pack_tok(&self,fimg: &mut FileImage,tok: &[u8],lang: ItemType,trailing: Option<&[u8]>) -> STDRESULT;
    /// get language tokens from file image, if file system uses a header it is stripped
    fn unpack_tok(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR>;
    /// turn JSON representation of random access text into a file image
    fn pack_rec_str(&self, fimg: &mut FileImage, json: &str) -> STDRESULT;
    /// turn the file image into JSON representation of random access text
    fn unpack_rec_str(&self,fimg: &FileImage,rec_len: Option<usize>,indent: Option<u16>) -> Result<String,DYNERR>;
    /// turn random access text records into a file image
    fn pack_rec(&self, fimg: &mut FileImage, recs: &Records) -> STDRESULT;
    /// turn the file image into random access text records
    fn unpack_rec(&self,fimg: &FileImage,rec_len: Option<usize>) -> Result<Records,DYNERR>;
    /// turn an AppleSingle file image into a native file image
    fn pack_apple_single(&self,_fimg: &mut FileImage, _dat: &[u8], _load_addr: Option<usize>) -> STDRESULT {
        log::error!("AppleSingle is not supported for this file system");
        Err(Box::new(Error::FileSystemMismatch))
    }
    /// turn the native file image into an AppleSingle file image
    fn unpack_apple_single(&self,_fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        log::error!("AppleSingle is not supported for this file system");
        Err(Box::new(Error::FileSystemMismatch))
    }
}

/// This is an abstraction used in handling random access text files.
/// Text encoding at this level is UTF8, it may be translated at lower levels.
/// This can be translated into `FileImage` for lower level handling,
/// or a JSON string for outward facing interactions.
pub struct Records {
    /// The fixed length of all records in this collection
    pub record_len: usize,
    /// key is an ordered record number starting at 0, no relation to any disk location
    pub map: HashMap<usize,String>
}


pub struct Stat {
    pub fs_name: String,
    pub label: String,
    pub users: Vec<String>,
    pub block_size: usize,
    pub block_beg: usize,
    pub block_end: usize,
    pub free_blocks: usize,
    /// raw params should be a JSON string or nothing
    pub raw: String
}

/// Abstract file system interface.  Presumed to own an underlying DiskImage.
/// Handles files, blocks, and directory structures.
/// Files are loaded or saved by passing file images.
/// File images are manipulated using the `Packing` trait.
pub trait DiskFS {
    /// Create an empty file image appropriate for this file system.
    /// To use the block size of this specific disk set `chunk_len` to `None`.
    fn new_fimg(&self, chunk_len: Option<usize>, set_time: bool, path: &str) -> Result<FileImage,DYNERR>;
    /// Stat the file system
    fn stat(&mut self) -> Result<Stat,DYNERR>;
    /// Directory listing to standard output in the file system's native style
    fn catalog_to_stdout(&mut self, path: &str) -> STDRESULT;
    /// Get directory listing as a Vec<String>.
    /// The rows are in an easily parsed fixed column format that is the same for all file systems.
    /// Columns 0..4 are the type/extension, 5..10 are the block count, 12.. is the basename.
    /// For flat file systems, the path must be "" or "/", or else an error is returned.
    /// For any file system, if the path resolves to a file, an error is returned.
    fn catalog_to_vec(&mut self, path: &str) -> Result<Vec<String>,DYNERR>;
    /// Return vector of paths based on the glob pattern
    fn glob(&mut self,pattern: &str,case_sensitive: bool) -> Result<Vec<String>,DYNERR>;
    /// Get the file system tree as a JSON string
    fn tree(&mut self,include_meta: bool,indent: Option<u16>) -> Result<String,DYNERR>;
    /// Create a new directory
    fn create(&mut self,path: &str) -> STDRESULT;
    /// Delete a file or directory
    fn delete(&mut self,path: &str) -> STDRESULT;
    /// Rename a file or directory
    fn rename(&mut self,path: &str,name: &str) -> STDRESULT;
    /// Change password protection for a file or disk.
    /// N.b. protection will only work in an emulation environment, and should not be considered secure.
    fn protect(&mut self,path: &str,password: &str,read: bool,write: bool,delete: bool) -> STDRESULT;
    /// Remove password protection for a file or disk.
    fn unprotect(&mut self,path: &str) -> STDRESULT;
    /// write protect a file
    fn lock(&mut self,path: &str) -> STDRESULT;
    // remove write protection from a file
    fn unlock(&mut self,path: &str) -> STDRESULT;
    /// Change the type and subtype of a file, strings may contain numbers as appropriate.
    fn retype(&mut self,path: &str,new_type: &str,sub_type: &str) -> STDRESULT;
    /// Get file image from the `path` within this disk image.
    fn get(&mut self,path: &str) -> Result<FileImage,DYNERR>;
    /// Write file image to this disk image at the path stored in `fimg`.
    fn put(&mut self,fimg: &FileImage) -> Result<usize,DYNERR>;
    /// Get a native file system allocation unit
    fn read_block(&mut self,num: &str) -> Result<Vec<u8>,DYNERR>;
    /// Put a native file system allocation unit
    /// N.b. this simply zaps the block and can break the file system.
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR>;
    /// Standardize for comparison with other sources of disk images.
    /// Returns a map from blocks to offsets within the block that are to be zeroed or ignored.
    /// Typically it is important to call this before deletions happen.
    /// May be recursive, ref_con can be used to initialize each recursion.
    fn standardize(&mut self,ref_con: u16) -> HashMap<Block,Vec<usize>>;
    /// Compare this disk with a reference disk for testing purposes.  Panics if comparison fails.
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>);
    /// Mutably borrow the underlying disk image
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage>;

    /// Convenience function to set path and put (default method)
    fn put_at(&mut self,path: &str,fimg: &mut FileImage) -> Result<usize,DYNERR> {
        fimg.set_path(path)?;
        self.put(fimg)
    }
    /// Convenience function to get (load_addr,binary_data) (default method)
    fn bload(&mut self,path: &str) -> Result<(usize,Vec<u8>),DYNERR> {
        let fimg = self.get(path)?;
        Ok((fimg.get_load_address() as usize, fimg.unpack_bin()?))
    }
    /// Convenience function to save binary file (default method)
    fn bsave(&mut self,path: &str,dat: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let mut fimg = self.new_fimg(None, true, path)?;
        fimg.pack_bin(dat,load_addr,trailing)?;
        self.put(&fimg)
    }
    /// Convenience function to get (load_addr,tokens) (default method)
    fn load(&mut self,path: &str) -> Result<(usize,Vec<u8>),DYNERR> {
        let fimg = self.get(path)?;
        Ok((fimg.get_load_address() as usize, fimg.unpack_tok()?))
    }
    /// Convenience function to save tokens (default method)
    fn save(&mut self,path: &str,dat: &[u8],lang: ItemType,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let mut fimg = self.new_fimg(None, true, path)?;
        fimg.pack_tok(dat,lang,trailing)?;
        self.put(&fimg)
    }
    /// Convenience function to load text (default method)
    fn read_text(&mut self,path: &str) -> Result<String,DYNERR> {
        self.get(path)?.unpack_txt()
    }
    /// Convenience function to save text (default method)
    fn write_text(&mut self,path: &str,txt: &str) -> Result<usize,DYNERR> {
        let mut fimg = self.new_fimg(None, true, path)?;
        fimg.pack_txt(txt)?;
        self.put(&fimg)
    }    
    /// Convenience function to load records (default method)
    fn read_records(&mut self,path: &str,rec_len: Option<usize>) -> Result<Records,DYNERR> {
        self.get(path)?.unpack_rec(rec_len)
    }
    /// Convenience function to save records (default method)
    fn write_records(&mut self,path: &str,recs: &Records) -> Result<usize,DYNERR> {
        let mut fimg = self.new_fimg(None, true, path)?;
        fimg.pack_rec(recs)?;
        self.put(&fimg)
    }    
}

impl Stat {
    pub fn to_json(&self,indent: Option<u16>) -> String {
        let mut ans = json::JsonValue::new_object();
        ans["fs_name"] = json::JsonValue::String(self.fs_name.clone());
        ans["label"] = json::JsonValue::String(self.label.clone());
        ans["users"] = json::JsonValue::Array(self.users.iter().map(|s| json::JsonValue::String(s.clone())).collect());
        ans["block_size"] = json::JsonValue::Number(self.block_size.into());
        ans["block_beg"] = json::JsonValue::Number(self.block_beg.into());
        ans["block_end"] = json::JsonValue::Number(self.block_end.into());
        ans["free_blocks"] = json::JsonValue::Number(self.free_blocks.into());
        if let Ok(obj) = json::parse(&self.raw) {
            ans["raw"] = obj;
        } else {
            ans["raw"] = json::JsonValue::Null;
        }
        if let Some(spaces) = indent {
            return json::stringify_pretty(ans, spaces);
        } else {
            return json::stringify(ans);
        }
    }
}