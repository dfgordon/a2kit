use std::str::FromStr;
use std::fmt;
use a2kit_macro::{DiskStructError,DiskStruct};
use crate::fs::TextConversion;

/// Status byte for a deleted file, also fill value for unused blocks.
pub const DELETED: u8 = 0xe5;
/// Status byte for a label
pub const LABEL: u8 = 0x20;
/// Status byte for a timestamp
pub const TIMESTAMP: u8 = 0x21;
/// Largest possible user number plus one
pub const USER_END: u8 = 0x10;
/// Unit of data transfer in bytes as seen by the CP/M BDOS.
/// This was the sector size on the original 8 inch disks.
pub const RECORD_SIZE: usize = 128;
/// Size of the directory entry in bytes, always 32
pub const DIR_ENTRY_SIZE: usize = 32;
/// Maximum number of logical extents in a file, array is indexed by major version number
pub const MAX_LOGICAL_EXTENTS: [usize;4] = [32,512,2048,2048];
/// There is a subdivision of an extent, sometimes called a logical extent,
/// which has a fixed size. See the EXM field in the disk parameter block.
pub const LOGICAL_EXTENT_SIZE: usize = 16384;
/// Characters forbidden from file names
pub const INVALID_CHARS: &str = " <>.,;:=?*[]";

/// Enumerates CP/M errors.  The `Display` trait will print the long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("bad disk format")]
    BadSector,
    #[error("bad data format")]
    BadFormat,
    #[error("file is read only")]
    FileReadOnly,
    #[error("disk is read only")]
    DiskReadOnly,
    #[error("drive not found")]
    Select,
    #[error("directory full")]
    DirectoryFull,
    #[error("disk full")]
    DiskFull,
    #[error("cannot read")]
    ReadError,
    #[error("cannot write")]
    WriteError,
    #[error("file exists")]
    FileExists,
    #[error("file not found")]
    FileNotFound
}

#[derive(PartialEq)]
pub enum ExtentType {
    File,
    Label,
    Password,
    Timestamp,
    Deleted,
    Unknown
}

/// Pointers to the various levels of CP/M disk structures.
/// This is supposed to help us distinguish "extent as directory entry" from "extent as data."
/// We should probably abolish all conflating of "extent" with "entry" (TODO).
#[derive(PartialEq,Eq,Copy,Clone)]
pub enum Ptr {
    /// Index to the extent's metadata, ordering its appearance in the directory
    ExtentEntry(usize),
    /// Index stored with the extent, ordering the logical extents within the file
    ExtentData(usize),
    /// Global pointer to one of the data blocks associated with a file extent
    Block(usize)
}

impl Ptr {
    /// These pointers are just counts, extract the integer.
    pub fn unwrap(&self) -> usize {
        match self {
            Self::Block(i) => *i,
            Self::ExtentData(i) => *i,
            Self::ExtentEntry(i) => *i
        }
    }
}

impl PartialOrd for Ptr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.unwrap().partial_cmp(&other.unwrap())
    }
    fn ge(&self, other: &Self) -> bool {
        self.unwrap().ge(&other.unwrap())
    }
    fn gt(&self, other: &Self) -> bool {
        self.unwrap().gt(&other.unwrap())
    }
    fn le(&self, other: &Self) -> bool {
        self.unwrap().le(&other.unwrap())
    }
    fn lt(&self, other: &Self) -> bool {
        self.unwrap().lt(&other.unwrap())
    }
}

impl Ord for Ptr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.unwrap().cmp(&other.unwrap())
    }
}

/// Transforms between UTF8 and CP/M text.
/// CP/M text is +ASCII with CRLF line separators, and 0x1A overall terminator.
/// non-ASCII found in the CP/M text is put as ASCII null.
/// non-ASCII found in a UTF8 string to convert is refused.
pub struct TextConverter {
    line_terminator: Vec<u8>
}

impl TextConversion for TextConverter {
    fn new(line_terminator: Vec<u8>) -> Self {
        Self {
            line_terminator
        }
    }
    fn from_utf8(&self,txt: &str) -> Option<Vec<u8>> {
        let src: Vec<u8> = txt.as_bytes().to_vec();
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if i+1<src.len() && src[i]==0x0d && src[i+1]==0x0a {
                continue;
            }
            if src[i]==0x0a || src[i]==0x0d {
                ans.push(0x0d);
                ans.push(0x0a);
            } else if src[i]<128 {
                ans.push(src[i]);
            } else {
                return None;
            }
        }
        if !Self::is_terminated(&ans, &self.line_terminator) {
            ans.append(&mut self.line_terminator.clone());
        }
        return Some(ans);
    }
    fn to_utf8(&self,src: &[u8]) -> Option<String> {
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if src[i]==0x0d {
                continue;
            } else if src[i]>127 {
                ans.push(0);
            } else if src[i]==0x1a {
                break;
            } else {
                ans.push(src[i]);
            }
        }
        let res = String::from_utf8(ans);
        match res {
            Ok(s) => Some(s),
            Err(_) => None
        }
    }
}

/// Structured representation of sequential text files on disk.
/// CP/M terminates with 0x1a to the next 128-byte record boundary.
/// For random access text use `fs::Records` instead.
pub struct SequentialText {
    pub text: Vec<u8>,
    terminator: u8
}

/// Allows the structure to be created from string slices using `from_str`.
/// This replaces LF/CR with CRLF. Negative ASCII is an error.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let encoder = TextConverter::new(vec![]);
        if let Some(dat) = encoder.from_utf8(s) {
            return Ok(Self {
                text: dat.clone(),
                terminator: 0x1a
            });
        }
        Err(std::fmt::Error)
    }
}

/// Allows the text to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
/// This disposes of CR, nulls negative ASCII, and terminates on 0x1a.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoder = TextConverter::new(vec![]);
        if let Some(ans) = encoder.to_utf8(&self.text) {
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
            terminator: 0x1a
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &[u8]) -> Result<Self,DiskStructError> {
        Ok(Self {
            text: match dat.split(|x| *x==0x1a).next() {
                Some(v) => v.to_vec(),
                _ => dat.to_vec()
            },
            terminator: 0x1a
        })
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.text.clone());
        ans.push(self.terminator);
        while ans.len()%128>0 {
            ans.push(self.terminator);
        }
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &[u8]) -> Result<(),DiskStructError> {
        let temp = SequentialText::from_bytes(&dat)?;
        self.text = temp.text.clone();
        self.terminator = 0x1a;
        Ok(())
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        let unpadded = self.text.len() + 1;
        match unpadded%128 {
            0 => unpadded,
            remainder => unpadded + 128 - remainder
        }
    }
}