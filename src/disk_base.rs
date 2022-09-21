//! # Base Layer for Disk Operations
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

#[derive(thiserror::Error,Debug)]
pub enum CommandError {
    #[error("Item type is not yet supported")]
    UnsupportedItemType,
    #[error("Item type is unknown")]
    UnknownItemType,
    #[error("Command could not be interpreted")]
    InvalidCommand,
    #[error("One of the parameters was out of range")]
    OutOfRange
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
    ApplesoftText,
    IntegerText,
    ApplesoftTokens,
    IntegerTokens,
    ApplesoftVars,
    IntegerVars,
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
            "atxt" => Ok(Self::ApplesoftText),
            "itxt" => Ok(Self::IntegerText),
            "atok" => Ok(Self::ApplesoftTokens),
            "itok" => Ok(Self::IntegerTokens),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
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
/// This is essentially `Records`, but for raw bytes.  Text should already be
/// properly encoded by the time it gets put into the chunks.
pub struct SparseData {
    /// The length of a chunk
    pub chunk_len: usize,
    /// The key is an ordered chunk number starting at 0, no relation to any disk location.
    /// Contraints on the length of the data are undefined at this level.
    pub chunks: HashMap<usize,Vec<u8>>
}

impl SparseData {
    pub fn new(chunk_len: usize) -> Self {
        Self {
            chunk_len,
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
}


/// This is an abstraction used in handling random access text files.
/// Text encoding at this level is UTF8, it may be translated at lower levels.
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
    pub fn add_record(&mut self,num: usize,fields: &str) {
        self.map.insert(num,fields.to_string());
    }
    pub fn to_sparse_data(&self,chunk_len: usize,encoder: impl TextEncoder) -> Result<SparseData,Box<dyn Error>> {
        let mut ans = SparseData::new(chunk_len);
        let mut total_end_logical_chunk = 0;
        // always need to have the first chunk referenced
        ans.chunks.insert(0,vec![0;chunk_len]);
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

/// Abstract disk interface mirroring BASIC commands.
/// This provides a uniform interface applicable to DOS or ProDOS.
pub trait A2Disk {
    /// List all the files on disk to standard output, mirrors `CATALOG`
    fn catalog_to_stdout(&self, path: &String);
    /// Create a new directory
    fn create(&mut self,path: &String,time: Option<chrono::NaiveDateTime>) -> Result<(),Box<dyn std::error::Error>>;
    /// Read a binary file from the disk, mirrors `BLOAD`.  Returns (aux,data), aux = starting address.
    fn bload(&self,name: &String) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write a binary file to the disk, mirrors `BSAVE`
    fn bsave(&mut self,name: &String, dat: &Vec<u8>,start_addr: u16) -> Result<usize,Box<dyn Error>>;
    /// Read a BASIC program file from the disk, mirrors `LOAD`, program is in tokenized form.
    /// Detokenization is handled in a different module.  Returns (aux,data), aux = 0
    fn load(&self,name: &String) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write a BASIC program to the disk, mirrors `SAVE`, program must already be tokenized.
    /// Tokenization is handled in a different module.
    fn save(&mut self,name: &String, dat: &Vec<u8>, typ: ItemType) -> Result<usize,Box<dyn Error>>;
    /// Read sequential text file from the disk, mirrors `READ`, text remains in raw A2 format.
    /// Use `decode_text` to get a UTF8 string.  Returns (aux,data), aux = 0.
    fn read_text(&self,name: &String) -> Result<(u16,Vec<u8>),Box<dyn Error>>;
    /// Write sequential text file to the disk, mirrors `WRITE`, text must already be in A2 format.
    /// Use `encode_text` to generate data from a UTF8 string.
    fn write_text(&mut self,name: &String, dat: &Vec<u8>) -> Result<usize,Box<dyn Error>>;
    /// Write records to a random access text file
    fn write_records(&mut self,name: &String, records: &Records) -> Result<usize,Box<dyn Error>>;
    /// Create disk image bytestream appropriate for the file system on this disk.
    fn to_img(&self) -> Vec<u8>;
    /// Convert file system text to a UTF8 string
    fn decode_text(&self,dat: &Vec<u8>) -> String;
    /// Convert UTF8 string to file system text
    fn encode_text(&self,s: &String) -> Result<Vec<u8>,Box<dyn Error>>;
    /// Standardize for comparison with other sources of disk images
    fn standardize(&mut self,ref_con: u16);
}