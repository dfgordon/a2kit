use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::str::FromStr;
use std::fmt;
use a2kit_macro::DiskStruct;
use super::TextEncoder;

pub const VTOC_TRACK: u8 = 17;
pub const MAX_DIRECTORY_REPS: usize = 100;
pub const MAX_TSLIST_REPS: usize = 1000;

/// Enumerates DOS errors.  The `Display` trait will print equivalent DOS message such as `FILE NOT FOUND`.  Following DOS errors are omitted:
/// LANGUAGE NOT AVAILABLE, NO BUFFERS AVAILABLE, PROGRAM TOO LARGE, NOT DIRECT COMMAND
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("RANGE ERROR")]
    Range,
    #[error("END OF DATA")]
    EndOfData,
    #[error("FILE NOT FOUND")]
    FileNotFound,
    #[error("VOLUME MISMATCH")]
    VolumeMismatch,
    #[error("I/O ERROR")]
    IOError,
    #[error("DISK FULL")]
    DiskFull,
    #[error("FILE LOCKED")]
    FileLocked,
    #[error("FILE TYPE MISMATCH")]
    FileTypeMismatch,
    #[error("WRITE PROTECTED")]
    WriteProtected,
    #[error("SYNTAX ERROR")]
    SyntaxError
}

/// Enumerates the four basic file types, available conversions are:
/// * FileType to u8,u16,u32: `as u8` etc.
/// * u8,u16,u32 to FileType: `FileType::from_u8` etc., (use FromPrimitive trait)
/// * &str to FileType: `FileType::from_str`, str can be a number or mnemonic
#[derive(FromPrimitive)]
pub enum FileType {
    Text = 0x00,
    Integer = 0x01,
    Applesoft = 0x02,
    Binary = 0x04
}

impl FromStr for FileType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        // string can be the number itself
        if let Ok(num) = u8::from_str(s) {
            return match FileType::from_u8(num) {
                Some(typ) => Ok(typ),
                _ => Err(Error::FileTypeMismatch)
            };
        }
        // or a mnemonic
        match s {
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "atok" => Ok(Self::Applesoft),
            "itok" => Ok(Self::Integer),
            _ => Err(Error::FileTypeMismatch)
        }
    }
}

/// This is for convenience in testing.  Sometimes the emulator will pad the data with random bytes at the end.
/// We need a way to append these bytes without changing the length calculation for comparisons.
fn append_junk(dat: &Vec<u8>,trailing: Option<&Vec<u8>>) -> Vec<u8> {
    match trailing {
        Some(v) => [dat.clone(),v.clone()].concat(),
        None => dat.clone()
    }
}

/// Transforms between UTF8 and DOS text encodings.
/// DOS uses negative ASCII with CR line separators.
pub struct Encoder {
    line_terminator: Vec<u8>
}

impl TextEncoder for Encoder {
    fn new(line_terminator: Vec<u8>) -> Self {
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
                ans.push(0x8d);
            } else if src[i]<128 {
                ans.push(src[i]+0x80);
            } else {
                return None;
            }
        }
        if !Self::is_terminated(&ans, &self.line_terminator) {
            ans.append(&mut self.line_terminator.clone());
        }
        return Some(ans);
    }
    fn decode(&self,src: &Vec<u8>) -> Option<String> {
        let mut ans: Vec<u8> = Vec::new();
        for i in 0..src.len() {
            if src[i]==0x8d {
                ans.push(0x0a);
            } else if src[i]>127 {
                ans.push(src[i]-0x80);
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

/// Structured representation of the bytes on disk that are stored with a BASIC program.  Works with either Applesoft or Integer.
pub struct TokenizedProgram {
    length: [u8;2],
    pub program: Vec<u8>
}

impl TokenizedProgram {
    /// Take unstructured bytes representing the tokens only (sans header) and pack it into the structure
    pub fn pack(prog: &Vec<u8>,trailing: Option<&Vec<u8>>) -> Self {
        let padded = append_junk(prog,trailing);
        Self {
            length: u16::to_le_bytes(prog.len() as u16),
            program: padded.clone()
        }
    }
}

impl DiskStruct for TokenizedProgram {
    /// Create an empty structure
    fn new() -> Self
    {
        Self {
            length: [0;2],
            program: Vec::new()
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        let end_byte = u16::from_le_bytes([dat[0],dat[1]]) as usize;
        // equality is not required because there could be sector padding
        if end_byte > dat.len() {
            panic!("inconsistent tokenized program length");
        }
        return Self {
            length: [dat[0],dat[1]],
            program: dat[2..end_byte+2].to_vec().clone()
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.length.to_vec());
        ans.append(&mut self.program.clone());
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = TokenizedProgram::from_bytes(&dat);
        self.length = temp.length;
        self.program = temp.program.clone();
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return 2 + self.program.len();
    }
}

/// Structured representation of sequential text files on disk.
/// For random access text use `fs::Records` instead.
pub struct SequentialText {
    pub text: Vec<u8>,
    terminator: u8
}

/// Allows the structure to be created from string slices using `from_str`.
/// This replaces LF/CRLF with CR and flips positive ASCII. Negative ASCII is an error.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let encoder = Encoder::new(vec![]);
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
/// This replaces CR with LF, flips negative ASCII, and nulls positive ASCII.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoder = Encoder::new(vec![]);
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
        ans.push(self.terminator);
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

/// Structured representation of binary data on disk
pub struct BinaryData {
    pub start: [u8;2],
    length: [u8;2],
    pub data: Vec<u8>
}

impl BinaryData {
    /// Take unstructured bytes representing the data only (sans header) and pack it into the structure
    pub fn pack(bin: &Vec<u8>, addr: u16) -> Self {
        Self {
            start: u16::to_le_bytes(addr),
            length: u16::to_le_bytes(bin.len() as u16),
            data: bin.clone()
        }
    }
}

impl DiskStruct for BinaryData {
    /// Create an empty structure
    fn new() -> Self
    {
        Self {
            start: [0;2],
            length: [0;2],
            data: Vec::new()
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        let end_byte = u16::from_le_bytes([dat[2],dat[3]]) + 4;
        Self {
            start: [dat[0],dat[1]],
            length: [dat[2],dat[3]],
            data: dat[4..end_byte as usize].to_vec()
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.start.to_vec());
        ans.append(&mut self.length.to_vec());
        ans.append(&mut self.data.clone());
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = BinaryData::from_bytes(&dat);
        self.start = temp.start;
        self.length = temp.length;
        self.data = temp.data.clone();
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return 4 + self.data.len();
    }
}