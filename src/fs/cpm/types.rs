use std::str::FromStr;
use std::fmt;
use log::debug;
use a2kit_macro::DiskStruct;
use super::super::TextEncoder;

/// Status byte for a deleted file, also fill value for unused blocks.
pub const DELETED: u8 = 0xe5;
/// Largest possible user number plus one
pub const USER_END: u8 = 0x10;
/// Unit of data transfer in bytes as seen by the CP/M BDOS.
/// This was the sector size on the original 8 inch disks.
pub const RECORD_SIZE: usize = 128;
/// Size of the directory entry in bytes, always 32
pub const DIR_ENTRY_SIZE: usize = 32;
/// Maximum number of logical extents in a file, array is indexed by major version number
pub const MAX_LOGICAL_EXTENTS: [usize;4] = [32,2048,2048,2048];
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

/// The Disk Parameter Block (DPB) was introduced with CP/M v2.
/// This allows CP/M to work with a variety of disk formats.
/// The DPB was stored somewhere in the BIOS.
/// The parameters are interdependent in a complicated way, see `verify` function.
/// Fields are public, but should be changed by hand only with caution.
pub struct DiskParameterBlock {
    /// number of 128-byte records per track
    pub spt: u16,
    /// block shift factor, records in block = 1 << bsh, bytes in block = 1 << bsh << 7
    pub bsh: u8,
    /// block mask, 2**bsh - 1, records in a block minus 1
    pub blm: u8,
    /// extent mask = logical extents per extent - 1.  Can be 0,1,3,7,15.
    /// Extent capacity is 16K * (EXM+1).
    /// The extent holds up to 16 8-bit refs for DSM<256, 8 16-bit refs otherwise.
    pub exm: u8,
    /// total blocks minus 1, not counting OS tracks
    pub dsm: u16,
    /// directory entries minus 1
    pub drm: u16,
    /// bitmap of directory blocks 1
    pub al0: u8,
    /// bitmap of directory blocks 2
    pub al1: u8,
    /// size of directory check vector
    pub cks: u16,
    /// number of reserved tracks, also track where directory starts
    pub off: u16,
    /// Physical record shift factor, PSH = log2(sector_bytes/128).
    /// Set to 0 if we don't need BDOS to translate.
    /// Requires CP/M v3 or higher.
    pub psh: u8,
    /// Physical record mask, PHM = sector_bytes/128 - 1
    /// Set to 0 if we don't need BDOS to translate.
    /// Requires CP/M v3 or higher.
    pub phm: u8
}

impl DiskParameterBlock {
    pub fn create(kind: &crate::img::DiskKind) -> Self {
        match *kind {
            crate::img::names::A2_DOS33_KIND => {
                Self {
                    spt: 32,
                    bsh: 3,
                    blm: 7,
                    exm: 0,
                    dsm: 127,
                    drm: 63,
                    al0: 0b11000000,
                    al1: 0b00000000,
                    cks: 0x8000,
                    off: 3,
                    psh: 0,
                    phm: 0
                }
            },
            crate::img::names::IBM_CPM1_KIND => {
                Self {
                    spt: 26,
                    bsh: 3,
                    blm: 7,
                    exm: 0,
                    dsm: 242,
                    drm: 63,
                    al0: 0b11000000,
                    al1: 0b00000000,
                    cks: 0x8000,
                    off: 2,
                    psh: 0,
                    phm: 0
                }
            }
            _ => panic!("Disk kind not supported")
        }
    }
    /// Check that parameter dependencies are all satisfied.
    pub fn verify(&self) -> bool {
        // n.b. order of these checks can matter
        if self.bsh<3 || self.bsh>7 {
            debug!("BSH is invalid");
            return false;
        }
        if self.blm as usize!=num_traits::pow(2,self.bsh as usize)-1 {
            debug!("BLM must be 2^BSH-1");
            return false;
        }
        if self.dsm>0x7fff {
            debug!("block count exceeds maximum");
            return false;
        }
        if self.bsh==3 && self.dsm>0xff {
            debug!("block count exceeds maximum for 1K blocks");
            return false;
        }
        let bls = (128 as usize) << self.bsh as usize;
        let max_exm = match self.dsm {
            dsm if dsm<256 => 16*bls/LOGICAL_EXTENT_SIZE - 1,
            _ => 8*bls/LOGICAL_EXTENT_SIZE - 1
        };
        if self.exm as usize > max_exm {
            debug!("too many logical extents");
            return false;
        }
        match self.exm {
            0b0 | 0b1 | 0b11 | 0b111 | 0b1111 => {},
            _ => {
                debug!("invalid extent mask {}",self.exm);
                return false;
            }
        }
        if self.drm as usize + 1 > 16*bls/32 {
            debug!("too many directory entries");
            return false;
        }
        let mut entry_bits = 0;
        for i in 0..8 {
            entry_bits += (self.al0 >> i) & 0x01;
            entry_bits += (self.al1 >> i) & 0x01;
        }
        if entry_bits as usize != (self.drm as usize + 1)*32/bls {
            debug!("directory block map mismatch");
            return false;
        }
        if self.dir_blocks() > self.user_blocks() {
            debug!("directory end block out of range");
            return false;
        }
        return true;
    }
    /// size of block in bytes
    pub fn block_size(&self) -> usize {
        (128 as usize) << self.bsh as usize
    }
    /// size of block pointer in bytes
    pub fn ptr_size(&self) -> usize {
        match self.dsm {
            dsm if dsm<256 => 1,
            _ => 2
        }
    }
    /// capacity of a full extent in bytes
    pub fn extent_capacity(&self) -> usize {
        (self.exm as usize + 1) * LOGICAL_EXTENT_SIZE
    }
    /// blocks available for directory and data
    pub fn user_blocks(&self) -> usize {
        self.dsm as usize + 1
    }
    /// maximum directory entries
    pub fn dir_entries(&self) -> usize {
        self.drm as usize + 1
    }
    /// number of directory blocks
    pub fn dir_blocks(&self) -> usize {
        self.dir_entries()*DIR_ENTRY_SIZE/self.block_size()
    }
    /// Work out the total byte capacity, accounting for OS tracks and unused "remainder sectors" on the last track.
    /// This assumes that every track is used, and that all tracks have the same capacity (we cannot do better with
    /// what is provided in the DPB).
    pub fn disk_capacity(&self) -> usize {
        let track_capacity = self.spt as usize * RECORD_SIZE;
        let os = self.off as usize * track_capacity;
        let user = self.user_blocks() * self.block_size();
        let remainder = user % track_capacity;
        if remainder>0 {
            return os + user + track_capacity - remainder;
        } else {
            return os + user;
        }
    }
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
#[derive(PartialEq,Copy,Clone)]
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

/// Transforms between UTF8 and CP/M text.
/// CP/M text is +ASCII with CRLF line separators.
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
    fn decode(&self,src: &Vec<u8>) -> Option<String> {
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
/// This replaces LF/CRLF with CR and flips positive ASCII. Negative ASCII is an error.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let encoder = Encoder::new(vec![]);
        if let Some(dat) = encoder.encode(s) {
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
            terminator: 0x1a
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        Self {
            text: match dat.split(|x| *x==0x1a).next() {
                Some(v) => v.to_vec(),
                _ => dat.clone()
            },
            terminator: 0x1a
        }
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
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = SequentialText::from_bytes(&dat);
        self.text = temp.text.clone();
        self.terminator = 0x1a;
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