
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use thiserror::Error;
use std::str::FromStr;
use std::fmt;
use a2kit_macro::DiskStruct;
use crate::disk_base::TextEncoder;

pub const BLOCK_SIZE: usize = 512;
pub const VOL_KEY_BLOCK: u16 = 2;
pub const STD_ACCESS: u8 = 1+2+32+64+128;

#[derive(Error,Debug)]
pub enum Error {
    #[error("RANGE ERROR")]
    Range = 2,
    #[error("NO DEVICE CONNECTED")]
    NoDeviceConnected = 3,
    #[error("WRITE PROTECTED")]
    WriteProtected = 4,
    #[error("END OF DATA")]
    EndOfData = 5,
    #[error("PATH NOT FOUND")]
    PathNotFound = 6,
    #[error("I/O ERROR")]
    IOError = 8,
    #[error("DISK FULL")]
    DiskFull = 9,
    #[error("FILE LOCKED")]
    FileLocked = 10,
    #[error("INVALID OPTION")]
    InvalidOption = 11,
    #[error("NO BUFFERS AVAILABLE")]
    NoBuffersAvailable = 12,
    #[error("FILE TYPE MISMATCH")]
    FileTypeMismatch = 13,
    #[error("PROGRAM TOO LARGE")]
    ProgramTooLarge = 14,
    #[error("NOT DIRECT COMMAND")]
    NotDirectCommand = 15,
    #[error("SYNTAX ERROR")]
    Syntax = 16,
    #[error("DIRECTORY FULL")]
    DirectoryFull = 17,
    #[error("FILE NOT OPEN")]
    FileNotOpen = 18,
    #[error("DUPLICATE FILENAME")]
    DuplicateFilename = 19,
    #[error("FILE BUSY")]
    FileBusy = 20,
    #[error("FILE(S) STILL OPEN")]
    FilesStillOpen = 21
}

/// Map file type codes to strings for display
pub const TYPE_MAP_DISP: [(u8,&str);39] = [
    (0x00, "???"),
    (0x01, "BAD"),
    (0x02, "PCD"), // Pascal code
    (0x03, "PTX"), // Pascal text
    (0x04, "TXT"),
    (0x05, "PDA"), // Pascal data
    (0x06, "BIN"),
    (0x07, "FON"), // SOS
    (0x08, "FOT"), // Photo
    (0x09, "BAS"), // SOS
    (0x0a, "DAT"), // SOS
    (0x0b, "WRD"), // SOS
    (0x0c, "SYS"), // SOS
    (0x0f, "DIR"),
    (0x10, "RPD"), // SOS
    (0x11, "RPX"), // SOS
    (0x12, "AFD"), // SOS
    (0x13, "AFM"), // SOS
    (0x14, "AFR"), // SOS
    (0x15, "SLB"), // SOS
    (0x19, "AWD"), // AppleWorks Data Base
    (0x1a, "AWW"), // AppleWorks Word Processor
    (0x1b, "AWS"), // AppleWorks Spreadsheet
    (0xef, "PSA"), // Pascal area
    (0xf0, "CMD"),
    (0xf1, "USR"),
    (0xf2, "USR"),
    (0xf3, "USR"),
    (0xf4, "USR"),
    (0xf5, "USR"),
    (0xf6, "USR"),
    (0xf7, "USR"),
    (0xf8, "USR"),
    (0xfa, "INT"),
    (0xfb, "IVR"),
    (0xfc, "BAS"),
    (0xfd, "VAR"),
    (0xfe, "REL"),
    (0xff, "SYS")
];

/// Enumerates a subset of ProDOS file types, available conversions are:
/// * Type to u8: `as u8`
/// * u8 to Type: `FromPrimitive::from_u8`
/// * &str to Type: `Type::from_str`, str can be a number or mnemonic
#[derive(FromPrimitive)]
pub enum FileType {
    None = 0x00,
    Text = 0x04,
    Binary = 0x06,
    Directory = 0x0f,
    IntegerCode = 0xfa,
    IntegerVars = 0xfb,
    ApplesoftCode = 0xfc,
    ApplesoftVars = 0xfd,
    RelocatableCode = 0xfe,
    System = 0xff
}

impl FromStr for FileType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        // string can be the number itself
        if let Ok(num) = u8::from_str(s) {
            return match FromPrimitive::from_u8(num) {
                Some(typ) => Ok(typ),
                _ => Err(Error::FileTypeMismatch)
            };
        }
        // or a mnemonic
        match s {
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "atok" => Ok(Self::ApplesoftCode),
            "itok" => Ok(Self::IntegerCode),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
            "rel" => Ok(Self::RelocatableCode),
            "sys" => Ok(Self::System),
            _ => Err(Error::FileTypeMismatch)
        }
    }
}

#[derive(Clone,Copy,FromPrimitive,PartialEq)]
pub enum StorageType {
    Inactive = 0x00,
    Seedling = 0x01,
    Sapling = 0x02,
    Tree = 0x03,
    Pascal = 0x04,
    SubDirEntry = 0x0d,
    SubDirHeader = 0x0e,
    VolDirHeader = 0x0f
}

#[derive(Clone,Copy,FromPrimitive)]
pub enum Access {
    Read = 0x01,
    Write = 0x02,
    Backup = 0x20,
    Rename = 0x40,
    Destroy = 0x80
}

/// Convenience for locating an entry in a directory.
/// `idx` mirrors the internal indexing, which starts at 2 in a key block, and 1 in an entry block
pub struct EntryLocation {
    pub block: u16,
    pub idx: usize
}

pub struct Encoder {
    line_terminator: Option<u8>
}

impl TextEncoder for Encoder {
    fn new(line_terminator: Option<u8>) -> Self {
        Self {
            line_terminator
        }
    }
    fn encode(&self,txt: &str) -> Option<Vec<u8>> {
        let src: Vec<u8> = txt.as_bytes().to_vec();
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if i+1<src.len() && src[i]==0x0d && src[i+1]==0x0a {
                continue;
            }
            if src[i]==0x0a || src[i]==0x0d {
                ans.push(0x0d);
            } else if src[i]<128 {
                ans.push(src[i]);
            } else {
                return None;
            }
        }
        if let Some(terminator) = self.line_terminator {
            if ans[ans.len()-1] != terminator {
                ans.push(terminator);
            }
        }
        return Some(ans);
    }
    fn decode(&self,src: &Vec<u8>) -> Option<String> {
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if src[i]==0x0d {
                ans.push(0x0a);
            } else if src[i]<128 {
                ans.push(src[i]);
            } else {
                ans.push(0);
            }
        }
        let res = String::from_utf8(ans);
        match res {
            Ok(s) => Some(s),
            Err(_) => None
        }
    }
}

/// Structured representation of sequential text files on disk.  Will not work for random access files.
pub struct SequentialText {
    pub text: Vec<u8>,
    terminator: u8
}

/// Allows the structure to be created from string slices using `from_str`.
/// This will convert LF/CRLF to CR.  Negative ASCII is an error.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let encoder = Encoder::new(None);
        if let Some(dat) = encoder.encode(s) {
            return Ok(Self {
                text: dat.clone(),
                terminator: 0
            });
        }
        Err(std::fmt::Error)
    }
}

/// Allows the text to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
/// This changes CR to LF, and nulls out negative ASCII.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoder = Encoder::new(None);
        if let Some(ans) = encoder.decode(&self.text) {
            return write!(f,"{}",ans);
        }
        write!(f,"err")
    }
}

impl DiskStruct for SequentialText {
    /// Create an empty structure
    fn new() -> Self {
        Self {
            text: Vec::new(),
            terminator: 0
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        Self {
            text: match dat.split(|x| *x==0).next() {
                Some(v) => v.to_vec(),
                _ => dat.clone()
            },
            terminator: 0
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.text.clone());
        ans.push(0);
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = SequentialText::from_bytes(&dat);
        self.text = temp.text.clone();
        self.terminator = 0;
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return self.text.len() + 1;
    }
}
