
use std::collections::HashMap;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use thiserror::Error;
use std::str::FromStr;
use std::fmt;
use a2kit_macro::DiskStruct;

pub const TYPE_MAP: [(u8,&str);39] = [
    (0x00, "???"),
    (0x01, "BAD"),
    (0x02, "PAC"), // Pascal code
    (0x03, "PAT"), // Pascal text
    (0x04, "TXT"),
    (0x05, "PAD"), // Pascal data
    (0x06, "BIN"),
    (0x07, "FON"), // SOS
    (0x08, "PIC"),
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

#[derive(FromPrimitive)]
pub enum FileType {
    None = 0x00,
    Text = 0x04,
    Binary = 0x06,
    Directory = 0x0f,
    IntegerCode = 0xfa,
    InteterVars = 0xfb,
    ApplesoftCode = 0xfc,
    ApplesoftVars = 0xfd,
    RelocatableCode = 0xfe,
    System = 0xff
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

/// Convenience for locating an entry in a directory.
/// `idx0` indexes the entry from 0
/// `idxv` mirrors the internal indexing, which starts at 2 in a key block, and 1 in an entry block
pub struct EntryLocation {
    pub block: u16,
    pub idx0: usize,
    pub idxv: usize
}

/// ProDOS files are in general sparse, i.e., index pointers are allowed to be null.
/// Sequential files can be viewed as sparse files with no null-pointers.
/// In this library, all files are based on a sparse file structure, with no loss in generality.
/// The `SparseFileData` struct does not mirror the actual disk structure, which is imposed elsewhere.
/// The index values and map keys are block pointers.  Before the file is written to disk, these may
/// be "shadow blocks", i.e., arbitrary except that 0 indicates an empty record.
pub struct SparseFileData {
    pub index: Vec<u16>,
    pub map: HashMap<u16,Vec<u8>>
}

impl SparseFileData {
    pub fn new() -> Self {
        Self {
            index: Vec::new(),
            map: HashMap::new()
        }
    }
    /// pack the data sequentially, all information about empty records is lost
    pub fn sequence(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        let temp = self.index.clone();
        for block in temp {
            if block!=0 {
                match self.map.get(&block) {
                    Some(v) => ans.append(&mut v.clone()),
                    _ => panic!("unmapped block in sparse file data")
                };
            }
        }
        return ans;
    }
    /// put any byte stream into a sparse data format
    pub fn desequence(dat: &Vec<u8>) -> Self {
        let mut mark = 0;
        let mut shadow_block = 1;
        let mut ans = Self::new();
        loop {
            let mut end = mark + 512;
            if end > dat.len() {
                end = dat.len();
            }
            ans.index.push(shadow_block);
            ans.map.insert(shadow_block,dat[mark..end].to_vec());
            mark = end;
            if mark == dat.len() {
                return ans;
            }
            shadow_block += 1;
        }
    }
}

/// Structured representation of sequential text files on disk.  Will not work for random access files.
pub struct SequentialText {
    pub text: Vec<u8>,
    terminator: u8
}

impl SequentialText {
    /// Take unstructured bytes representing the text only (sans terminator) and pack it into the structure.
    /// Use `FromStr` and `Display` below if you need to convert newlines for display purposes.
    pub fn pack(txt: &Vec<u8>) -> Self {
        Self {
            text: txt.clone(),
            terminator: 0
        }
    }
}

/// Allows the structure to be created from string slices using `from_str`.
/// This will convert LF/CRLF to CR.  Negative ASCII is an error.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let src: Vec<u8> = s.as_bytes().to_vec();
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if ans.len()>0 && ans[ans.len()-1]==0x0d && src[i]==0x0a {
                continue;
            }
            if src[i]==0x0a || src[i]==0x0d {
                ans.push(0x0d);
            } else if src[i]<128 {
                ans.push(src[i]);
            } else {
                return Err(std::fmt::Error);
            }
        }
        return Ok(Self {
            text: ans.clone(),
            terminator: 0
        });
    }
}

/// Allows the text to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
/// This changes CR to LF, and nulls out negative ASCII.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ans: Vec<u8> = Vec::new();
        let src: Vec<u8> = self.text.clone();
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
            Ok(s) => write!(f,"{}",s),
            Err(_) => write!(f,"err")
        }
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
        // find end of text
        let mut end_byte = dat.len();
        for i in 0..dat.len() {
            if dat[i]==0 {
                end_byte = i;
                break;
            }
        }
        Self {
            text: dat[0..end_byte as usize].to_vec(),
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
