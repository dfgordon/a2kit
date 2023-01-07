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
//! This module also contains the `Chunk` enumeration, which specifies and locates allocation units.
//! The enumeration names the file system's allocation system, and its value is a specific chunk.
//! The value can take any form, e.g., DOS chunks are 2-element lists with [track,sector], whereas
//! CPM chunks are 3-tuples with (block,BSH,OFF).
//! 
//! Sector skews are not handled here.  Transformation of a `Chunk` to a physical disk address is
//! handled within the `img` module.  Transformations that go between a file system and a disk,
//! such as sector skews, are kept in the `bios` module.

pub mod dos3x;
pub mod prodos;
pub mod pascal;
pub mod cpm;

use std::fmt;
use std::str::FromStr;
use std::collections::HashMap;
use log::{warn,error};
use crate::img;
use crate::commands::ItemType;

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
/// The `Chunk` implementation includes a simple mapping from blocks to sectors; disk images can use this or not as appropriate.
/// Disk images can also decide whether to immediately return an error given certain chunk types; e.g., a PO image might refuse
/// to locate a DO chunk type.  However, do not get confused, e.g., a DO image should usually be prepared to process a
/// PO chunk, since there are many ProDOS DSK images that are DOS ordered.
#[derive(PartialEq,Eq,Clone,Copy,Hash)]
pub enum Chunk {
    /// value is [track,sector]
    D13([usize;2]),
    /// value is [track,sector]
    DO([usize;2]),
    /// value is block number
    PO(usize),
    /// value is (absolute block number, BSH, OFF); see cpm::types
    CPM((usize,u8,u16))
}

impl Chunk {
    /// At this level we can only take sectors per track, and return a track-sector list,
    /// where a simple monotonically increasing relationship is assumed between chunks and sectors.
    /// Any further skewing must be handled by the caller.  CP/M offset is accounted for.
    pub fn get_lsecs(&self,secs_per_track: usize) -> Vec<[usize;2]> {
        match self {
            Self::D13([t,s]) => vec![[*t,*s]],
            Self::DO([t,s]) => vec![[*t,*s]],
            Self::PO(block) => panic!("function `get_lsecs` not appropriate for ProDOS"),
            Self::CPM((block,bsh,off)) => {
                let mut ans: Vec<[usize;2]> = Vec::new();
                let lsecs_per_block = 1 << bsh;
                for sec_count in block*lsecs_per_block..(block+1)*lsecs_per_block {
                    ans.push([*off as usize + sec_count/secs_per_track , sec_count%secs_per_track]);
                }
                ans
            }
        }
    }
}
impl fmt::Display for Chunk {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13([t,s]) => write!(f,"D13 track {} sector {}",t,s),
            Self::DO([t,s]) => write!(f,"DOS track {} sector {}",t,s),
            Self::PO(b) => write!(f,"ProDOS block {}",b),
            Self::CPM((b,s,o)) => write!(f,"CPM block {} shift {} offset {}",b,s,o)
        }
    }
}

/// Testing aid, adds offsets to the existing key, or create a new key if needed
pub fn add_ignorable_offsets(map: &mut HashMap<Chunk,Vec<usize>>,key: Chunk, offsets: Vec<usize>) {
    if let Some(val) = map.get(&key) {
        map.insert(key,[val.clone(),offsets].concat());
    } else {
        map.insert(key,offsets);
    }
}

/// Testing aid, combines offsets from two maps (used to fold in subdirectory offsets)
pub fn combine_ignorable_offsets(map: &mut HashMap<Chunk,Vec<usize>>,other: HashMap<Chunk,Vec<usize>>) {
    for (k,v) in other.iter() {
        add_ignorable_offsets(map, *k, v.clone());
    }
}

/// This converts between UTF8+LF/CRLF and the encoding used by the file system
pub trait TextEncoder {
    fn new(line_terminator: Vec<u8>) -> Self where Self: Sized;
    fn encode(&self,txt: &str) -> Option<Vec<u8>>;
    fn decode(&self,raw: &Vec<u8>) -> Option<String>;
    fn is_terminated(bytes: &Vec<u8>,term: &Vec<u8>) -> bool {
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
/// Sequential files are a special case.
/// Supports importing/exporting to an even more general JSON format.
/// In the JSON format, all data is represented by hex strings directly taken from disk.
/// Internally, `FileImage` encodes all metadata fields in a `u32` formed from the first four
/// little-endian hex-bytes in the JSON. `DiskFS` is responsible for further interpretation.
/// The data itself is in quantized chunks of `u8`, all of the same length.
/// A chunk could be a sector or block, depending on the file system.
pub struct FileImage {
    /// UTF8 string naming the file system
    pub file_system: String,
    /// length of a chunk
    pub chunk_len: u32,
    /// length of the file were it serialized
    pub eof: u32,
    /// file type, encoding varies by file system
    pub fs_type: u32,
    /// auxiliary file information, encoding varies by file system
    pub aux: u32,
    /// The access control bits, encoding varies by file system
    pub access: Vec<u8>,
    /// The creation time, encoding varies by file system
    pub created: u32,
    /// The modified time, encoding varies by file system
    pub modified: u32,
    /// Some version
    pub version: u32,
    /// Some minimum version
    pub min_version: u32,
    /// The key is an ordered chunk number starting at 0, no relation to any disk location.
    /// Contraints on the length of the data are undefined at this level.
    pub chunks: HashMap<usize,Vec<u8>>
}

impl FileImage {
    pub fn new(chunk_len: usize) -> Self {
        Self {
            file_system: String::from(""),
            chunk_len: chunk_len as u32,
            fs_type: 0,
            aux: 0,
            eof: 0,
            access: vec![0],
            created: 0,
            modified: 0,
            version: 0,
            min_version: 0,
            chunks: HashMap::new()
        }
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
    /// put any byte stream into a sparse data format
    pub fn desequence(chunk_len: usize, dat: &Vec<u8>) -> Self {
        let mut mark = 0;
        let mut idx = 0;
        let mut ans = Self::new(chunk_len);
        if dat.len()==0 {
            ans.eof = 0;
            return ans;
        }
        loop {
            let mut end = mark + chunk_len;
            if end > dat.len() {
                end = dat.len();
            }
            ans.chunks.insert(idx,dat[mark..end].to_vec());
            mark = end;
            if mark == dat.len() {
                ans.eof = dat.len() as u32;
                return ans;
            }
            idx += 1;
        }
    }
    pub fn parse_hex_to_vec(key: &str,parsed: &json::JsonValue) -> Option<Vec<u8>> {
        if let Some(s) = parsed[key].as_str() {
            if let Ok(bytes) = hex::decode(s) {
                return Some(bytes);
            }
        }
        return None;
    }
    pub fn parse_hex_to_u32(key: &str,parsed: &json::JsonValue) -> Option<u32> {
        if let Some(s) = parsed[key].as_str() {
            if let Ok(bytes) = hex::decode(s) {
                if bytes.len()<5 {
                    let mut ans: u32 = 0;
                    for i in 0..bytes.len() {
                        ans += bytes[i] as u32 * 256_u32.pow(i as u32);
                    }
                    return Some(ans);
                }
            }
        }
        return None;
    }
    /// Get chunks from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<FileImage,Box<dyn std::error::Error>> {
        match json::parse(json_str) {
            Ok(parsed) => {
                let maybe_fs = parsed["file_system"].as_str();
                let maybe_len = FileImage::parse_hex_to_u32("chunk_len",&parsed);
                let maybe_fs_type = FileImage::parse_hex_to_u32("fs_type",&parsed);
                let maybe_aux = FileImage::parse_hex_to_u32("aux",&parsed);
                let maybe_eof = FileImage::parse_hex_to_u32("eof",&parsed);
                let maybe_access = FileImage::parse_hex_to_vec("access",&parsed);
                let maybe_created = FileImage::parse_hex_to_u32("created",&parsed);
                let maybe_modified = FileImage::parse_hex_to_u32("modified",&parsed);
                let maybe_vers = FileImage::parse_hex_to_u32("version",&parsed);
                let maybe_min_version = FileImage::parse_hex_to_u32("min_version",&parsed);
                if let (
                    Some(fs),
                    Some(chunk_len),
                    Some(fs_type),
                    Some(aux),
                    Some(eof),
                    Some(access),
                    Some(created),
                    Some(modified),
                    Some(version),
                    Some(min_version)
                ) = (
                    maybe_fs,
                    maybe_len,
                    maybe_fs_type,
                    maybe_aux,
                    maybe_eof,
                    maybe_access,
                    maybe_created,
                    maybe_modified,
                    maybe_vers,
                    maybe_min_version) {
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
                error!("json records missing metadata");
                Err(Box::new(Error::FileImageFormat))
            },
            Err(_e) => Err(Box::new(Error::FileImageFormat))
        } 
    }
    /// Put chunks into the JSON string representation, if indent=0 use unpretty form
    pub fn to_json(&self,indent: u16) -> String {
        let mut json_map = json::JsonValue::new_object();
        for (c,v) in &self.chunks {
            json_map[c.to_string()] = json::JsonValue::String(hex::encode_upper(v));
        }
        let ans = json::object! {
            file_system: self.file_system.clone(),
            chunk_len: hex::encode_upper(u32::to_le_bytes(self.chunk_len)),
            eof: hex::encode_upper(u32::to_le_bytes(self.eof)),
            fs_type: hex::encode_upper(u32::to_le_bytes(self.fs_type)),
            aux: hex::encode_upper(u32::to_le_bytes(self.aux)),
            access: hex::encode_upper(self.access.clone()),
            created: hex::encode_upper(u32::to_le_bytes(self.created)),
            modified: hex::encode_upper(u32::to_le_bytes(self.modified)),
            version: hex::encode_upper(u32::to_le_bytes(self.version)),
            min_version: hex::encode_upper(u32::to_le_bytes(self.min_version)),
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
    pub fn from_fimg(dat: &FileImage,record_length: usize,encoder: impl TextEncoder) -> Result<Records,Box<dyn std::error::Error>> {
        if record_length==0 {
            return Err(Box::new(Error::FileFormat));
        }
        let mut ans = Records::new(record_length);
        let mut list: Vec<usize> = Vec::new();
        // add record index for each starting record boundary that falls within a chunk
        let chunk_len = dat.chunk_len as usize;
        for c in dat.chunks.keys() {
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
                match dat.chunks.get(&chunk_num) {
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
    /// create file image from the records, this is usually done before writing to a disk image
    pub fn to_fimg(&self,chunk_len: usize,fs_type: u32,require_first: bool,encoder: impl TextEncoder) -> Result<FileImage,Box<dyn std::error::Error>> {
        let mut ans = FileImage::new(chunk_len);
        ans.fs_type = fs_type;
        ans.aux = self.record_len as u32;
        ans.eof = 0;
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
                        ans.eof = u32::max((lb*512 + buf.len()) as u32,ans.eof);
                        ans.chunks.insert(lb as usize,buf);
                    }
                },
                None => return Err(Box::new(std::fmt::Error))
            }
        }
        return Ok(ans);
    }
    /// Get records from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<Records,Box<dyn std::error::Error>> {
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
/// Provides BASIC-like high level commands, chunk operations, and file image operations.
pub trait DiskFS {
    /// List all the files on disk to standard output, mirrors `CATALOG`
    fn catalog_to_stdout(&self, path: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// Create a new directory
    fn create(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// Delete a file or directory
    fn delete(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// Rename a file or directory
    fn rename(&mut self,path: &str,name: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// write protect a file
    fn lock(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>>;
    // remove write protection from a file
    fn unlock(&mut self,path: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// Change the type and subtype of a file, strings may contain numbers as appropriate.
    fn retype(&mut self,path: &str,new_type: &str,sub_type: &str) -> Result<(),Box<dyn std::error::Error>>;
    /// Read a binary file from the disk, mirrors `BLOAD`.  Returns (aux,data), aux = starting address.
    fn bload(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
    /// Write a binary file to the disk, mirrors `BSAVE`
    fn bsave(&mut self,path: &str, dat: &Vec<u8>,start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>>;
    /// Read a BASIC program file from the disk, mirrors `LOAD`, program is in tokenized form.
    /// Detokenization is handled in a different module.  Returns (aux,data), aux = 0
    fn load(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
    /// Write a BASIC program to the disk, mirrors `SAVE`, program must already be tokenized.
    /// Tokenization is handled in a different module.
    fn save(&mut self,path: &str, dat: &Vec<u8>, typ: ItemType,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>>;
    /// Read sequential text file from the disk, mirrors `READ`, text remains in raw A2 format.
    /// Use `decode_text` to get a UTF8 string.  Returns (aux,data), aux = 0.
    fn read_text(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
    /// Write sequential text file to the disk, mirrors `WRITE`, text must already be in A2 format.
    /// Use `encode_text` to generate data from a UTF8 string.
    fn write_text(&mut self,path: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>>;
    /// Read records from a random access text file.  This finds all possible records, some may be spurious.
    /// The `record_length` can be set to 0 on file systems where this is stored with the file.
    fn read_records(&self,path: &str,record_length: usize) -> Result<Records,Box<dyn std::error::Error>>;
    /// Write records to a random access text file
    fn write_records(&mut self,path: &str, records: &Records) -> Result<usize,Box<dyn std::error::Error>>;
    /// Read a file into a generalized representation
    fn read_any(&self,path: &str) -> Result<FileImage,Box<dyn std::error::Error>>;
    /// Write a file from a generalized representation
    fn write_any(&mut self,path: &str,dat: &FileImage) -> Result<usize,Box<dyn std::error::Error>>;
    /// Get a chunk (block or sector) appropriate for this file system
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>>;
    /// Put a chunk (block or sector) appropriate for this file system.
    /// N.b. this simply zaps the chunk and can break the file system.
    fn write_chunk(&mut self, num: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>>;
    /// Convert file system text to a UTF8 string
    fn decode_text(&self,dat: &Vec<u8>) -> String;
    /// Convert UTF8 string to file system text
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,Box<dyn std::error::Error>>;
    /// Standardize for comparison with other sources of disk images.
    /// Returns a map from chunks to offsets within the chunk that are to be zeroed or ignored.
    /// Typically it is important to call this before deletions happen.
    /// May be recursive, ref_con can be used to initialize each recursion.
    fn standardize(&self,ref_con: u16) -> HashMap<Chunk,Vec<usize>>;
    /// Compare this disk with a reference disk for testing purposes.  Panics if comparison fails.
    fn compare(&self,path: &std::path::Path,ignore: &HashMap<Chunk,Vec<usize>>);
    /// Mutably borrow the underlying disk image
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage>;
}