/// Enumerates MS-DOS 3.3 errors.  The `Display` trait will print the long message.
/// Some have been paraphrased, many omitted.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("general")]
    General,
    #[error("non-DOS disk")]
    NonDOSDisk,
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

// MS-DOS handles text just like CP/M, so we simply make the CP/M
// handlers available from here.
type Encoder = crate::fs::cpm::types::Encoder;
pub type SequentialText = crate::fs::cpm::types::SequentialText;

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
