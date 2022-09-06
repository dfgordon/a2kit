
use std::collections::HashMap;
use thiserror::Error;

pub const TYPE_MAP: [(u8,&str);10] = [
    (0x00, "???"),
    (0x04, "TXT"),
    (0x06, "BIN"),
    (0x0f, "DIR"),
    (0xfa, "INT"),
    (0xfb, "IVR"),
    (0xfc, "BAS"),
    (0xfd, "VAR"),
    (0xfe, "REL"),
    (0xff, "SYS")
];

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

#[derive(Clone,Copy)]
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

/// ProDOS files are in general sparse, i.e., index pointers are allowed to be null.
/// Sequential files can be viewed as sparse files with no null-pointers.
/// In this library, all files are based on a sparse file structure, with no loss in generality.
/// The `SparseFileData` struct does not mirror the actual disk structure, which is imposed elsewhere.
pub struct SparseFileData {
    index: Vec<u16>,
    map: HashMap<u16,[u8;512]>
}