//! # File System Module
//! 
//! File system modules handle interactions with directories and files.  There is a sub-module for
//! each supported file system.
//! 
//! File systems are represented by the `DiskFS` trait.  The trait object takes ownership of
//! some disk image, which it uses as storage.  Files are represented by a `FileImage` trait
//! object.  This is a low level representation of the file that works for any of the supported
//! file systems.
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

use std::fmt;
use std::str::FromStr;
use std::collections::{BTreeMap,HashMap};
use log::{warn,error};
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
    #[error("high level file format is wrong")]
    FileFormat
}

/// Encapsulates the disk address and addressing mode used by a file system.
/// Disk addresses generally involve some transformation between logical (file system) and physical (disk fields) addresses.
/// The disk image layer has the final responsibility for making this transformation.
/// The `Block` implementation includes a simple mapping from blocks to sectors; disk images can use this or not as appropriate.
/// Disk images can also decide whether to immediately return an error given certain block types; e.g., a PO image might refuse
/// to locate a DO block type.  However, do not get confused, e.g., a DO image should usually be prepared to process a
/// PO block, since there are many ProDOS DSK images that are DOS ordered.
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
    /// At this level we can only take sectors per track, and return a track-sector list,
    /// where a simple monotonically increasing relationship is assumed between blocks and sectors.
    /// Any further skewing must be handled by the caller.  CP/M and FAT offsets are accounted for.
    /// CP/M logical sectors are numbered from 1.
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

/// This converts between UTF8+LF/CRLF and the encoding used by the file system
pub trait TextEncoder {
    fn new(line_terminator: Vec<u8>) -> Self where Self: Sized;
    fn encode(&self,txt: &str) -> Option<Vec<u8>>;
    fn decode(&self,raw: &[u8]) -> Option<String>;
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

/// This is an abstraction of a sparse file and its metadata.
/// Sequential files are a special case.  Metadata items are represented by a Vec<u8>
/// that contains the same byte ordering that is stored on disk.  In the JSON representation
/// these become hex strings.  The `DiskFS` is responsible for further interpretation.
/// The data itself is stored in a map with a numerical chunk id as the key, and a Vec<u8>
/// as the chunk data.  The JSON representation uses decimal strings for the key and hex
/// strings for the data.  *Beware of sorting routines that put "10" before "9"*.
/// 
/// Each `DiskFS` provides its own routine for creating an empty file image.
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
    /// The creation time, encoding varies by file system
    pub created: Vec<u8>,
    /// The modified time, encoding varies by file system
    pub modified: Vec<u8>,
    /// Some version
    pub version: Vec<u8>,
    /// Some minimum version
    pub min_version: Vec<u8>,
    /// The key is an ordered chunk number starting at 0, no relation to any disk location.
    /// Contraints on the length of the data are undefined at this level.
    pub chunks: HashMap<usize,Vec<u8>>
}

impl FileImage {
    pub fn fimg_version() -> String {
        "2.0.0".to_string()
    }
    pub fn ordered_indices(&self) -> Vec<usize> {
        let copy = self.chunks.clone();
        let mut idx_list = copy.into_keys().collect::<Vec<usize>>();
        idx_list.sort_unstable();
        return idx_list;
    }
    /// Find the logical number of chunks (assuming indexing from 0..end)
    pub fn end(&self) -> usize {
        match self.ordered_indices().pop() {
            Some(idx) => idx+1,
            None => 0
        }
    }
    /// pack the data sequentially, all structure is lost
    pub fn sequence(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        for chunk in self.ordered_indices() {
            match self.chunks.get(&chunk) {
                Some(v) => ans.append(&mut v.clone()),
                _ => panic!("unreachable")
            };
        }
        return ans;
    }
    /// pack the data sequentially, all structure is lost
    pub fn sequence_limited(&self,max_len: usize) -> Vec<u8> {
        let mut ans = self.sequence();
        if max_len < ans.len() {
            ans = ans[0..max_len].to_vec();
        }
        return ans;
    }
    /// use any byte stream as the file image data; internally this organizes the data into chunks
    pub fn desequence(&mut self, dat: &[u8]) {
        let mut mark = 0;
        let mut idx = 0;
        if dat.len()==0 {
            self.eof = vec![0;self.eof.len()];
            return;
        }
        loop {
            let mut end = mark + self.chunk_len;
            if end > dat.len() {
                end = dat.len();
            }
            self.chunks.insert(idx,dat[mark..end].to_vec());
            mark = end;
            if mark == dat.len() {
                self.eof = Self::fix_le_vec(dat.len(),self.eof.len());
                return;
            }
            idx += 1;
        }
    }
    /// throw out trailing zeros with minimum length constraint
    fn fix_le_vec(val: usize,min_len: usize) -> Vec<u8> {
        let mut ans = usize::to_le_bytes(val).to_vec();
        let mut count = 0;
        for byte in ans.iter().rev() {
            if *byte>0 {
                break;
            }
            count += 1;
        }
        for _i in 0..count {
            ans.pop();
        }
        for _i in ans.len()..min_len {
            ans.push(0);
        }
        ans
    }
    /// compute a usize assuming missing trailing bytes are 0
    fn usize_from_truncated_le_bytes(bytes: &[u8]) -> usize {
        let mut ans: usize = 0;
        for i in 0..bytes.len() {
            if i == usize::BITS as usize/8 {
                break;
            }
            ans += (bytes[i] as usize) << (i*8);
        }
        ans
    }
    pub fn parse_hex_to_vec(key: &str,parsed: &json::JsonValue) -> Result<Vec<u8>,DYNERR> {
        if let Some(s) = parsed[key].as_str() {
            if let Ok(bytes) = hex::decode(s) {
                return Ok(bytes);
            }
        }
        error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    pub fn parse_usize(key: &str,parsed: &json::JsonValue) -> Result<usize,DYNERR> {
        if let Some(val) = parsed[key].as_usize() {
            return Ok(val);
        }
        error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    pub fn parse_str(key: &str,parsed: &json::JsonValue) -> Result<String,DYNERR> {
        if let Some(s) = parsed[key].as_str() {
            return Ok(s.to_string());
        }
        error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    /// Get chunks from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<FileImage,DYNERR> {
        let parsed = json::parse(json_str)?;
        let fimg_version = FileImage::parse_str("fimg_version",&parsed)?;
        let fs = FileImage::parse_str("file_system",&parsed)?;
        let chunk_len = FileImage::parse_usize("chunk_len", &parsed)?;
        let fs_type = FileImage::parse_hex_to_vec("fs_type",&parsed)?;
        let aux = FileImage::parse_hex_to_vec("aux",&parsed)?;
        let eof = FileImage::parse_hex_to_vec("eof",&parsed)?;
        let access = FileImage::parse_hex_to_vec("access",&parsed)?;
        let created = FileImage::parse_hex_to_vec("created",&parsed)?;
        let modified = FileImage::parse_hex_to_vec("modified",&parsed)?;
        let version = FileImage::parse_hex_to_vec("version",&parsed)?;
        let min_version = FileImage::parse_hex_to_vec("min_version",&parsed)?;
        let mut chunks: HashMap<usize,Vec<u8>> = HashMap::new();
        let map_obj = &parsed["chunks"];
        if map_obj.entries().len()==0 {
            warn!("file image contains metadata, but no data");
        }
        for (key,hex) in map_obj.entries() {
            let prev_len = chunks.len();
            if let Ok(num) = usize::from_str(key) {
                if let Some(hex_str) = hex.as_str() {
                    if let Ok(dat) = hex::decode(hex_str) {
                        chunks.insert(num,dat);
                    }
                }
            }
            if chunks.len()==prev_len {
                error!("could not read hex string from chunk");
                return Err(Box::new(Error::FileImageFormat));
            }
        }
        return Ok(Self {
            fimg_version,
            file_system: fs.to_string(),
            chunk_len,
            eof,
            fs_type,
            aux,
            access,
            created,
            modified,
            version,
            min_version,
            chunks
        });
    }
    /// Put chunks into the JSON string representation, if indent=0 use unpretty form
    pub fn to_json(&self,indent: u16) -> String {
        let mut json_map = json::JsonValue::new_object();
        let mut sorted : BTreeMap<usize,Vec<u8>> = BTreeMap::new();
        for (c,v) in &self.chunks {
            sorted.insert(*c,v.clone());
        }
        for (c,v) in &sorted {
            json_map[c.to_string()] = json::JsonValue::String(hex::encode_upper(v));
        }
        let ans = json::object! {
            fimg_version: self.fimg_version.clone(),
            file_system: self.file_system.clone(),
            chunk_len: self.chunk_len,
            eof: hex::encode_upper(self.eof.clone()),
            fs_type: hex::encode_upper(self.fs_type.clone()),
            aux: hex::encode_upper(self.aux.clone()),
            access: hex::encode_upper(self.access.clone()),
            created: hex::encode_upper(self.created.clone()),
            modified: hex::encode_upper(self.modified.clone()),
            version: hex::encode_upper(self.version.clone()),
            min_version: hex::encode_upper(self.min_version.clone()),
            chunks: json_map
        };
        if indent > 0 {
            return json::stringify_pretty(ans, indent);
        } else {
            return json::stringify(ans);
        }
    }}


/// This is an abstraction used in handling random access text files.
/// Text encoding at this level is UTF8, it may be translated at lower levels.
/// This will usually be translated into `FileImage` for lower level handling.
pub struct Records {
    /// The fixed length of all records in this collection
    pub record_len: usize,
    /// key is an ordered record number starting at 0, no relation to any disk location
    pub map: HashMap<usize,String>
}

impl Records {
    pub fn new(record_len: usize) -> Self {
        Self {
            record_len,
            map: HashMap::new()
        }
    }
    /// add a string as record number `num`, fields should be separated by LF or CRLF.
    pub fn add_record(&mut self,num: usize,fields: &str) {
        self.map.insert(num,fields.to_string());
    }
    /// Derive records from file image, this should find any real record, but may also find spurious ones.
    /// This is due to fundamental non-invertibility of the A2 file system's random access storage pattern.
    /// This routine assumes ASCII null terminates any record.
    pub fn from_fimg(fimg: &FileImage,record_length: usize,encoder: impl TextEncoder) -> Result<Records,DYNERR> {
        if record_length==0 {
            return Err(Box::new(Error::FileFormat));
        }
        let mut ans = Records::new(record_length);
        let mut list: Vec<usize> = Vec::new();
        // add record index for each starting record boundary that falls within a chunk
        let chunk_len = fimg.chunk_len;
        for c in fimg.chunks.keys() {
            let start_rec = c*chunk_len/record_length + match c*chunk_len%record_length { x if x>0 => 1, _ => 0 };
            let end_rec = (c+1)*chunk_len/record_length + match (c+1)*chunk_len%record_length { x if x>0 => 1, _ => 0 };
            for r in start_rec..end_rec {
                list.push(r);
            }
        }
        // add only records with complete data
        for r in list {
            let start_chunk = r*record_length/chunk_len;
            let end_chunk = 1 + (r+1)*record_length/chunk_len;
            let start_offset = r*record_length%chunk_len;
            let mut bytes: Vec<u8> = Vec::new();
            let mut complete = true;
            for chunk_num in start_chunk..end_chunk {
                match fimg.chunks.get(&chunk_num) {
                    Some(chunk) => {
                       for i in chunk {
                            bytes.push(*i);
                        }
                    },
                    _ => complete = false
                }
            }
            if complete && start_offset < bytes.len() {
                let actual_end = usize::min(start_offset+record_length,bytes.len());
                if let Some(long_str) = encoder.decode(&bytes[start_offset..actual_end].to_vec()) {
                    if let Some(partial) = long_str.split("\u{0000}").next() {
                        if partial.len()>0 {
                            ans.map.insert(r,partial.to_string());
                        }
                    } else {
                        if long_str.len()>0 {
                            ans.map.insert(r,long_str);
                        }
                    }
                }
            }
        }
        return Ok(ans);
    }
    /// Update a file image's data using the records, this is usually done before writing to a disk image.
    /// This will set the file image's eof, but no other metadata.
    pub fn update_fimg(&self,ans: &mut FileImage,require_first: bool,encoder: impl TextEncoder) -> STDRESULT {
        let chunk_len = ans.chunk_len;
        let mut eof: usize = 0;
        // always need to have the first chunk referenced on ProDOS
        if require_first {
            ans.chunks.insert(0,vec![0;chunk_len]);
        }
        // now insert the actual records, first chunk can always be overwritten
        for (rec_num,fields) in &self.map {
            match encoder.encode(fields) {
                Some(data_bytes) => {
                    let logical_chunk = self.record_len * rec_num / chunk_len;
                    let end_logical_chunk = 1 + (self.record_len * (rec_num+1) - 1) / chunk_len;
                    let fwd_offset = self.record_len * rec_num % chunk_len;
                    for lb in logical_chunk..end_logical_chunk {
                        let start_byte = match lb {
                            l if l==logical_chunk => fwd_offset,
                            _ => 0
                        };
                        let end_byte = match lb {
                            l if l==end_logical_chunk-1 => fwd_offset + data_bytes.len() - chunk_len*(end_logical_chunk-logical_chunk-1),
                            _ => chunk_len
                        };
                        let mut buf = match ans.chunks.contains_key(&lb) {
                            true => ans.chunks.get(&lb).unwrap().clone(),
                            false => Vec::new()
                        };
                        // extend only to the end of data
                        for _i in buf.len()..end_byte as usize {
                            buf.push(0);
                        }
                        // load the part of the chunk with the data
                        for i in start_byte..end_byte {
                            buf[i as usize] = data_bytes[chunk_len*(lb-logical_chunk) + i - fwd_offset];
                        }
                        eof = usize::max(lb*512 + buf.len(),eof);
                        ans.chunks.insert(lb as usize,buf);
                    }
                },
                None => return Err(Box::new(std::fmt::Error))
            }
        }
        ans.eof = FileImage::fix_le_vec(eof,ans.eof.len());
        return Ok(());
    }
    /// Get records from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<Records,DYNERR> {
        match json::parse(json_str) {
            Ok(parsed) => {
                let maybe_type = parsed["fimg_type"].as_str();
                let maybe_len = parsed["record_length"].as_usize();
                if let (Some(typ),Some(len)) = (maybe_type,maybe_len) {
                    if typ=="rec" {
                        let mut records: HashMap<usize,String> = HashMap::new();
                        let map_obj = &parsed["records"];
                        if map_obj.entries().len()==0 {
                            error!("no object entries in json records");
                            return Err(Box::new(Error::FileImageFormat));
                        }
                        for (key,lines) in map_obj.entries() {
                            if let Ok(num) = usize::from_str(key) {
                                let mut fields = String::new();
                                for maybe_field in lines.members() {
                                    if let Some(line) = maybe_field.as_str() {
                                        fields = fields + line + "\n";
                                    } else {
                                        error!("record is not a string");
                                        return Err(Box::new(Error::FileImageFormat));
                                    }
                                }
                                records.insert(num,fields);
                            } else {
                                error!("key is not a number");
                                return Err(Box::new(Error::FileImageFormat));
                            }
                        }
                        return Ok(Self {
                            record_len: len,
                            map: records
                        });    
                    } else {
                        error!("json metadata type mismatch");
                        return Err(Box::new(Error::FileImageFormat));
                    }
                }
                error!("json records missing metadata");
                Err(Box::new(Error::FileImageFormat))
            },
            Err(_e) => Err(Box::new(Error::FileImageFormat))
        } 
    }
    /// Put records into the JSON string representation, if indent=0 use unpretty form
    pub fn to_json(&self,indent: u16) -> String {
        let mut json_map = json::JsonValue::new_object();
        for (r,l) in &self.map {
            let mut json_array = json::JsonValue::new_array();
            for line in l.lines() {
                json_array.push(line).expect("error while building JSON array");
            }
            json_map[r.to_string()] = json_array;
        }
        let ans = json::object! {
            fimg_type: "rec",
            record_length: self.record_len,
            records: json_map
        };
        if indent > 0 {
            return json::stringify_pretty(ans, indent);
        } else {
            return json::stringify(ans);
        }
    }
}

/// Allows the records to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
impl fmt::Display for Records {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx,fields) in &self.map {
            write!(f,"Record {}",idx).expect("format error");
            for field in fields.lines() {
                write!(f,"    {}",field).expect("format error");
            }
        }
        write!(f,"Record Count = {}",self.map.len())
    }
}

/// Abstract file system interface.  Presumed to own an underlying DiskImage.
/// Provides BASIC-like high level commands, block operations, and file image operations.
pub trait DiskFS {
    /// Create an empty file image appropriate for this file system
    fn new_fimg(&self,chunk_len: usize) -> FileImage;
    /// List all the files on disk to standard output, mirrors `CATALOG`
    fn catalog_to_stdout(&mut self, path: &str) -> STDRESULT;
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
    /// Read a binary file from the disk.  Returns (aux,data), aux = load address if applicable.
    fn bload(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR>;
    /// Write a binary file to the disk.
    fn bsave(&mut self,path: &str, dat: &[u8],start_addr: u16,trailing: Option<&[u8]>) -> Result<usize,DYNERR>;
    /// Read a BASIC program file from the disk, program is in tokenized form.
    /// Detokenization is handled in a different module.  Returns (aux,data), aux = load address if applicable.
    fn load(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR>;
    /// Write a BASIC program to the disk, program must already be tokenized.
    /// Tokenization is handled in a different module.
    fn save(&mut self,path: &str, dat: &[u8], typ: ItemType,trailing: Option<&[u8]>) -> Result<usize,DYNERR>;
    /// Read sequential data from the disk, Returns (aux,data), aux is implementation dependent.
    /// If `trunc=true` the data will be truncated at the EOF given by the file's metadata (if available),
    /// otherwise it extends to the block boundary.
    fn read_raw(&mut self,path: &str,trunc: bool) -> Result<(u16,Vec<u8>),DYNERR>;
    /// Write sequential data to the disk.
    fn write_raw(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR>;
    /// Usually same as `read_raw` with `trunc=true`. Use `decode_text` on the result to get a UTF8 string.
    fn read_text(&mut self,path: &str) -> Result<(u16,Vec<u8>),DYNERR>;
    /// Usually same as `write_raw`. Use `encode_text` to generate `dat` from a UTF8 string.
    fn write_text(&mut self,path: &str, dat: &[u8]) -> Result<usize,DYNERR>;
    /// Read records from a random access text file.  This finds all possible records, some may be spurious.
    /// The `record_length` can be set to 0 on file systems where this is stored with the file.
    fn read_records(&mut self,path: &str,record_length: usize) -> Result<Records,DYNERR>;
    /// Write records to a random access text file
    fn write_records(&mut self,path: &str, records: &Records) -> Result<usize,DYNERR>;
    /// Read a file into a generalized representation
    fn read_any(&mut self,path: &str) -> Result<FileImage,DYNERR>;
    /// Write a file from a generalized representation
    fn write_any(&mut self,path: &str,fimg: &FileImage) -> Result<usize,DYNERR>;
    /// Get a native file system allocation unit
    fn read_block(&mut self,num: &str) -> Result<(u16,Vec<u8>),DYNERR>;
    /// Put a native file system allocation unit
    /// N.b. this simply zaps the block and can break the file system.
    fn write_block(&mut self, num: &str, dat: &[u8]) -> Result<usize,DYNERR>;
    /// Convert file system text to a UTF8 string
    fn decode_text(&self,dat: &[u8]) -> Result<String,DYNERR>;
    /// Convert UTF8 string to file system text
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,DYNERR>;
    /// Standardize for comparison with other sources of disk images.
    /// Returns a map from blocks to offsets within the block that are to be zeroed or ignored.
    /// Typically it is important to call this before deletions happen.
    /// May be recursive, ref_con can be used to initialize each recursion.
    fn standardize(&mut self,ref_con: u16) -> HashMap<Block,Vec<usize>>;
    /// Compare this disk with a reference disk for testing purposes.  Panics if comparison fails.
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>);
    /// Mutably borrow the underlying disk image
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage>;
}