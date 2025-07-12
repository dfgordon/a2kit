use std::str::FromStr;
use std::fmt;
use a2kit_macro::{DiskStructError,DiskStruct};
use super::super::TextConversion;

/// Enumerates MS-DOS 3.3 errors.  The `Display` trait will print the long message.
/// Some have been paraphrased, many omitted.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("general")]
    General,
    #[error("read fault")]
    ReadFault,
    #[error("sector not found")]
    SectorNotFound,
    #[error("write fault")]
    WriteFault,
    #[error("write protect")]
    WriteProtect,
    #[error("invalid command line parameter")]
    InvalidSwitch,
    #[error("File allocation table bad")]
    BadFAT,
    #[error("file not found")]
    FileNotFound,
    #[error("duplicate file name")]
    DuplicateFile,
    #[error("insufficient disk space")]
    DiskFull,
    #[error("no room in directory")]
    DirectoryFull,
    #[error("directory not empty")]
    DirectoryNotEmpty,
    #[error("syntax")]
    Syntax,
    #[error("first cluster invalid")]
    FirstClusterInvalid,
    #[error("incorrect DOS version")]
    IncorrectDOS
}

/// Pointers to the various levels of FAT disk structures.
/// This may help distinguish the various indices we have to work with.
#[derive(PartialEq,Eq,Copy,Clone)]
pub enum Ptr {
    /// Index ordering a file's appearance in the directory
    Entry(usize),
    /// Index ordering clusters in the FAT
    Cluster(usize),
    /// Index ordering logical sectors
    LogicalSector(usize)
}

impl Ptr {
    /// These pointers are just counts, extract the integer.
    pub fn unwrap(&self) -> usize {
        match self {
            Self::Entry(i) => *i,
            Self::Cluster(i) => *i,
            Self::LogicalSector(i) => *i
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


// We can directly re-use the CP/M encoder
type TextConverter = crate::fs::cpm::types::TextConverter;

/// Structured representation of sequential text files on disk.
/// MS-DOS text is like CP/M, except no padding to 128 byte boundaries.
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
        self.text.len() + 1
    }
}