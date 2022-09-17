//! # Base Layer for Disk Operations
//! This module defines types and traits for use with any supported disk image.
//! Ideally this should encompass any file system.
//! The structure is geared toward DOS and ProDOS at present.
//! Note that the `DiskStruct` trait, which abstracts directory components in a file system,
//! uses procedural macros, and therefore is required to be in a separate crate.

use std::error::Error;
use thiserror;
use std::str::FromStr;

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
    /// Create disk image bytestream appropriate for the file system on this disk.
    fn to_img(&self) -> Vec<u8>;
    /// Convert file system text to a UTF8 string
    fn decode_text(&self,dat: &Vec<u8>) -> String;
    /// Convert UTF8 string to file system text
    fn encode_text(&self,s: &String) -> Result<Vec<u8>,Box<dyn Error>>;
    /// Standardize for comparison with other sources of disk images
    fn standardize(&mut self,ref_con: u16);
}