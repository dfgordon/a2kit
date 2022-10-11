//! # Base Layer for File System Operations
//! This module defines types and traits for use with any supported disk image.
//! Ideally this should encompass any file system.
//! The structure is geared toward DOS and ProDOS at present.
//! Note that the `DiskStruct` trait, which abstracts directory components in a file system,
//! uses procedural macros, and therefore is required to be in a separate crate.

use std::error::Error;
use thiserror;
use std::str::FromStr;
use std::collections::HashMap;
use std::fmt;
use json;
use hex;

#[derive(thiserror::Error,Debug)]
pub enum CommandError {
    #[error("Item type is not yet supported")]
    UnsupportedItemType,
    #[error("Item type is unknown")]
    UnknownItemType,
    #[error("Command could not be interpreted")]
    InvalidCommand,
    #[error("One of the parameters was out of range")]
    OutOfRange,
    #[error("Input source could not be interpreted")]
    InputFormatBad
}

#[derive(PartialEq)]
pub enum DiskKind {
    A2_525_13,
    A2_525_16,
    A2_35
}

#[derive(PartialEq)]
pub enum DiskImageType {
    DO,
    PO,
    WOZ
}

/// Types of files that may be distinguished by the file system.
/// This will have to be mapped to a similar enumeration at lower levels
/// in order to obtain the binary type code.
#[derive(PartialEq)]
pub enum ItemType {
    Raw,
    Binary,
    Text,
    Records,
    SparseData,
    ApplesoftText,
    IntegerText,
    ApplesoftTokens,
    IntegerTokens,
    ApplesoftVars,
    IntegerVars,
    Chunk,
    System
}

impl FromStr for DiskImageType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "do" => Ok(Self::DO),
            "po" => Ok(Self::PO),
            "woz" => Ok(Self::WOZ),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

impl FromStr for ItemType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "raw" => Ok(Self::Raw),
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "rec" => Ok(Self::Records),
            "any" => Ok(Self::SparseData),
            "atxt" => Ok(Self::ApplesoftText),
            "itxt" => Ok(Self::IntegerText),
            "atok" => Ok(Self::ApplesoftTokens),
            "itok" => Ok(Self::IntegerTokens),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
            "chunk" => Ok(Self::Chunk),
            "sys" => Ok(Self::System),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

/// This converts between UTF8+LF/CRLF and the encoding used by the file system
pub trait TextEncoder {
    fn new(terminator: Option<u8>) -> Self where Self: Sized;
    fn encode(&self,txt: &str) -> Option<Vec<u8>>;
    fn decode(&self,raw: &Vec<u8>) -> Option<String>;
}

/// This is an abstraction of a sparse file, that also can encompass sequential files.
/// The data is in the form of quantized chunks,
/// all of the same length. A chunk could be a sector or block, depending on file system.
/// The chunks can be partially filled, e.g., `desequence` will not pad the last chunk.
/// This is essentially `Records`, but for raw bytes.  Text should already be
/// properly encoded by the time it gets put into the chunks.
pub struct SparseData {
    /// The length of a chunk
    pub chunk_len: usize,
    /// The file system type in some string representation
    pub fs_type: String,
    /// Auxiliary data in some string representation
    pub aux: String,
    /// The key is an ordered chunk number starting at 0, no relation to any disk location.
    /// Contraints on the length of the data are undefined at this level.
    pub chunks: HashMap<usize,Vec<u8>>
}

impl SparseData {
    pub fn new(chunk_len: usize) -> Self {
        Self {
            chunk_len,
            fs_type: String::from("bin"),
            aux: String::from("0"),
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
    /// put any byte stream into a sparse data format
    pub fn desequence(chunk_len: usize, dat: &Vec<u8>) -> Self {
        let mut mark = 0;
        let mut idx = 0;
        let mut ans = Self::new(chunk_len);
        loop {
            let mut end = mark + chunk_len;
            if end > dat.len() {
                end = dat.len();
            }
            ans.chunks.insert(idx,dat[mark..end].to_vec());
            mark = end;
            if mark == dat.len() {
                return ans;
            }
            idx += 1;
        }
    }
    pub fn new_type(&mut self,new_type: &str) -> &mut Self {
        self.fs_type = new_type.to_string();
        return self;
    }
    pub fn new_aux(&mut self,new_aux: &str) -> &mut Self {
        self.aux = new_aux.to_string();
        return self;
    }
    /// Get chunks from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<SparseData,Box<dyn Error>> {
        match json::parse(json_str) {
            Ok(parsed) => {
                let maybe_type = parsed["a2kit_type"].as_str();
                let maybe_len = parsed["chunk_length"].as_usize();
                let maybe_fs_type = parsed["fs_type"].as_str();
                let maybe_aux = parsed["aux"].as_str();
                if let (Some(typ),Some(len),Some(fs_type),Some(aux)) = (maybe_type,maybe_len,maybe_fs_type,maybe_aux) {
                    if typ=="any" {
                        let mut chunks: HashMap<usize,Vec<u8>> = HashMap::new();
                        let map_obj = &parsed["chunks"];
                        if map_obj.entries().len()==0 {
                            eprintln!("no object entries in json records");
                            return Err(Box::new(CommandError::InputFormatBad));
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
                                eprintln!("could not read hex string from chunk");
                                return Err(Box::new(CommandError::InputFormatBad));
                            }
                        }
                        return Ok(Self {
                            chunk_len: len,
                            fs_type: fs_type.to_string(),
                            aux: aux.to_string(),
                            chunks
                        });    
                    } else {
                        eprintln!("json metadata type mismatch");
                        return Err(Box::new(CommandError::InputFormatBad));
                    }
                }
                eprintln!("json records missing metadata");
                Err(Box::new(CommandError::InputFormatBad))
            },
            Err(_e) => Err(Box::new(CommandError::InputFormatBad))
        } 
    }
    /// Put chunks into the JSON string representation, if indent=0 use unpretty form
    pub fn to_json(&self,indent: u16) -> String {
        let mut json_map = json::JsonValue::new_object();
        for (c,v) in &self.chunks {
            json_map[c.to_string()] = json::JsonValue::String(hex::encode_upper(v));
        }
        let ans = json::object! {
            a2kit_type: "any",
            fs_type: self.fs_type.to_string(),
            aux: self.aux.to_string(),
            chunk_length: self.chunk_len,
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
/// This will usually be translated into `SparseData` for lower level handling.
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
    /// Derive records from sparse data, this should find any real record, but may also find spurious ones.
    /// This is due to fundamental non-invertibility of the A2 file system's random access storage pattern.
    /// This routine assumes ASCII null terminates any record.
    pub fn from_sparse_data(dat: &SparseData,record_length: usize,encoder: impl TextEncoder) -> Result<Records,Box<dyn Error>> {
        if record_length==0 {
            return Err(Box::new(CommandError::OutOfRange));
        }
        let mut ans = Records::new(record_length);
        let mut list: Vec<usize> = Vec::new();
        // add record index for each starting record boundary that falls within a chunk
        for c in dat.chunks.keys() {
            let start_rec = c*dat.chunk_len/record_length + match c*dat.chunk_len%record_length { x if x>0 => 1, _ => 0 };
            let end_rec = (c+1)*dat.chunk_len/record_length + match (c+1)*dat.chunk_len%record_length { x if x>0 => 1, _ => 0 };
            for r in start_rec..end_rec {
                list.push(r);
            }
        }
        // add only records with complete data
        for r in list {
            let start_chunk = r*record_length/dat.chunk_len;
            let end_chunk = 1 + (r+1)*record_length/dat.chunk_len;
            let start_offset = r*record_length%dat.chunk_len;
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
    /// create sparse data from the records, this is usually done before writing to a disk image
    pub fn to_sparse_data(&self,chunk_len: usize,require_first: bool,encoder: impl TextEncoder) -> Result<SparseData,Box<dyn Error>> {
        let mut ans = SparseData::new(chunk_len);
        ans.new_type("txt");
        ans.new_aux(&self.record_len.to_string());
        let mut total_end_logical_chunk = 0;
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
                        ans.chunks.insert(lb as usize,buf);
                    }
                    if end_logical_chunk > total_end_logical_chunk {
                        total_end_logical_chunk = end_logical_chunk;
                    }
                },
                None => return Err(Box::new(std::fmt::Error))
            }
        }
        return Ok(ans);
    }
    /// Get records from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<Records,Box<dyn Error>> {
        match json::parse(json_str) {
            Ok(parsed) => {
                let maybe_type = parsed["a2kit_type"].as_str();
                let maybe_len = parsed["record_length"].as_usize();
                if let (Some(typ),Some(len)) = (maybe_type,maybe_len) {
                    if typ=="rec" {
                        let mut records: HashMap<usize,String> = HashMap::new();
                        let map_obj = &parsed["records"];
                        if map_obj.entries().len()==0 {
                            eprintln!("no object entries in json records");
                            return Err(Box::new(CommandError::InputFormatBad));
                        }
                        for (key,lines) in map_obj.entries() {
                            if let Ok(num) = usize::from_str(key) {
                                let mut fields = String::new();
                                for maybe_field in lines.members() {
                                    if let Some(line) = maybe_field.as_str() {
                                        fields = fields + line + "\n";
                                    } else {
                                        eprintln!("record is not a string");
                                        return Err(Box::new(CommandError::InputFormatBad));
                                    }
                                }
                                records.insert(num,fields);
                            } else {
                                eprintln!("key is not a number");
                                return Err(Box::new(CommandError::InputFormatBad));
                            }
                        }
                        return Ok(Self {
                            record_len: len,
                            map: records
                        });    
                    } else {
                        eprintln!("json metadata type mismatch");
                        return Err(Box::new(CommandError::InputFormatBad));
                    }
                }
                eprintln!("json records missing metadata");
                Err(Box::new(CommandError::InputFormatBad))
            },
            Err(_e) => Err(Box::new(CommandError::InputFormatBad))
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
            a2kit_type: "rec",
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

pub trait DiskImage {
    fn update_from_dsk(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn Error>>;
    fn to_dsk(&self) -> Result<Vec<u8>,Box<dyn Error>>;
}

/// Abstract disk interface applicable to DOS or ProDOS.
/// Provides BASIC-like file commands, chunk operations, and `any` type operations.
pub trait A2Disk {
    /// List all the files on disk to standard output, mirrors `CATALOG`
    fn catalog_to_stdout(&self, path: &str) -> Result<(),Box<dyn Error>>;
    /// Create a new directory
    fn create(&mut self,path: &str) -> Result<(),Box<dyn Error>>;
    /// Delete a file or directory
    fn delete(&mut self,path: &str) -> Result<(),Box<dyn Error>>;
    /// Rename a file or directory
    fn rename(&mut self,path: &str,name: &str) -> Result<(),Box<dyn Error>>;
    /// write protect a file
    fn lock(&mut self,path: &str) -> Result<(),Box<dyn Error>>;
    // remove write protection from a file
    fn unlock(&mut self,path: &str) -> Result<(),Box<dyn Error>>;
    /// Read a binary file from the disk, mirrors `BLOAD`.  Returns (aux,data), aux = starting address.
    fn bload(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write a binary file to the disk, mirrors `BSAVE`
    fn bsave(&mut self,path: &str, dat: &Vec<u8>,start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn Error>>;
    /// Read a BASIC program file from the disk, mirrors `LOAD`, program is in tokenized form.
    /// Detokenization is handled in a different module.  Returns (aux,data), aux = 0
    fn load(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write a BASIC program to the disk, mirrors `SAVE`, program must already be tokenized.
    /// Tokenization is handled in a different module.
    fn save(&mut self,path: &str, dat: &Vec<u8>, typ: ItemType,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn Error>>;
    /// Read sequential text file from the disk, mirrors `READ`, text remains in raw A2 format.
    /// Use `decode_text` to get a UTF8 string.  Returns (aux,data), aux = 0.
    fn read_text(&self,path: &str) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write sequential text file to the disk, mirrors `WRITE`, text must already be in A2 format.
    /// Use `encode_text` to generate data from a UTF8 string.
    fn write_text(&mut self,path: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn Error>>;
    /// Read records from a random access text file.  This finds all possible records, some may be spurious.
    /// The `record_length` can be set to 0 on file systems where this is stored with the file.
    fn read_records(&self,path: &str,record_length: usize) -> Result<Records,Box<dyn Error>>;
    /// Write records to a random access text file
    fn write_records(&mut self,path: &str, records: &Records) -> Result<usize,Box<dyn Error>>;
    /// Read a file into a generalized representation
    fn read_any(&self,path: &str) -> Result<SparseData,Box<dyn Error>>;
    /// Write a file from a generalized representation
    fn write_any(&mut self,path: &str,dat: &SparseData) -> Result<usize,Box<dyn Error>>;
    /// Get a chunk (block or sector) appropriate for this disk
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Put a chunk (block or sector) appropriate for this disk, n.b. this simply zaps the disk image and can easily break it
    fn write_chunk(&mut self, num: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn Error>>;
    /// Create disk image bytestream appropriate for the file system on this disk.
    fn to_img(&self) -> Vec<u8>;
    /// Convert file system text to a UTF8 string
    fn decode_text(&self,dat: &Vec<u8>) -> String;
    /// Convert UTF8 string to file system text
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,Box<dyn Error>>;
    /// Standardize for comparison with other sources of disk images.
    /// Returns a vector of offsets into the image that are to be zeroed or ignored.
    /// Typically it is important to call this before deletions happen.
    /// May be recursive, ref_con can be used to initialize each recursion.
    fn standardize(&self,ref_con: u16) -> Vec<usize>;
    /// Compare this disk with a reference disk for testing purposes.  Panics if comparison fails.
    fn compare(&self,path: &std::path::Path,ignore: &Vec<usize>);
}