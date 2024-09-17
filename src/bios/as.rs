//! ### AppleSingle (as)
//!
//! This module handles the parsing of AppleSingle files, a file format developed by Apple to store
//! both data and resource forks of a file in a single entity. It supports version 1 and 2 of the format,
//! and it uses the `binrw` crate to facilitate reading and interpreting the binary structures.

use binrw::io::SeekFrom;
use std::fmt::Display;
use std::fs::File;
use std::path::Path;
use binrw::BinRead;
use chrono::DateTime;

#[derive(BinRead, Debug, Clone)]
#[brw(big, magic = 0x0051600u32)]
#[br(assert(version == 0x00010000 || version == 0x00020000, "Unknown AppleSingle version {:X}, only version 1 and 2 are supported", version))]
pub struct AppleSingleFile {
    pub version: u32,
    #[br(count = 16, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]
    pub home_fs: String,
    num_entries: u16,
    #[br(count = num_entries)]
    pub entries: Vec<Entry>,  // Array of entries in the AppleSingle file
}

impl AppleSingleFile {
    pub fn get_entry(&self, entry_type: EntryType) -> Option<&EntryData> {
        self.entries.iter().find(|e| e.r#type == entry_type).map(|e| &e.data)
    }
}

#[derive(BinRead, Debug, Clone)]
#[br(big)]
pub struct Entry {
    pub r#type: EntryType,
    offset: u32,
    length: u32,
    #[br(
        seek_before = SeekFrom::Start(offset.into()),
        restore_position,
        args { r#type: r#type, length: length }
    )]
    pub data: EntryData,
}

#[derive(BinRead, PartialEq, Clone, Copy, Debug)]
#[brw(repr=u32)]
pub enum EntryType {
    DataFork = 1,
    ResourceFork = 2,
    RealName = 3,
    Comment = 4,
    IconBw = 5,
    IconColor = 6,
    FileInfo = 7,
    FileDatesInfo = 8,
    FinderInfo = 9,
    MacintoshFileInfo = 10,
    ProdosFileInfo = 11,
    MsdosFileInfo = 12,
    ShortName = 13,
    AfpFileInfo = 14,
    DirectoryId = 15,
}

impl Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            EntryType::DataFork => "Data Fork",
            EntryType::ResourceFork => "Resource Fork",
            EntryType::RealName => "Real Name",
            EntryType::Comment => "Comment",
            EntryType::IconBw => "Icon, B&W",
            EntryType::IconColor => "Icon, Color",
            EntryType::FileInfo => "File Info",
            EntryType::FileDatesInfo => "File Dates Info",
            EntryType::FinderInfo => "Finder Info",
            EntryType::MacintoshFileInfo => "Macintosh File Info",
            EntryType::ProdosFileInfo => "ProDOS File Info",
            EntryType::MsdosFileInfo => "MS-DOS File Info",
            EntryType::ShortName => "Short Name",
            EntryType::AfpFileInfo => "AFP File Info",
            EntryType::DirectoryId => "Directory ID",
        })
    }
}

#[derive(BinRead, Debug, Clone)]
#[br(import { r#type: EntryType, length: u32 })]
pub enum EntryData {
    #[br(pre_assert(r#type == EntryType::DataFork))]
    DataFork(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::ResourceFork))]
    ResourceFork(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::RealName))]
    RealName(#[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]String),
    #[br(pre_assert(r#type == EntryType::Comment))]
    Comment(#[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]String),
    #[br(pre_assert(r#type == EntryType::IconBw))]
    IconBw(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::IconColor))]
    IconColor(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::FileInfo))]
    FileInfo(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::FileDatesInfo))]
    FileDatesInfo(FileDatesInfo),
    #[br(pre_assert(r#type == EntryType::FinderInfo))]
    FinderInfo(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::MacintoshFileInfo))]
    MacintoshFileInfo(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::ProdosFileInfo))]
    ProDOSFileInfo(ProdosFileInfo),
    #[br(pre_assert(r#type == EntryType::ShortName))]
    ShortName(#[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]String),
    #[br(pre_assert(r#type == EntryType::AfpFileInfo))]
    AfpFileInfo(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::DirectoryId))]
    DirectoryId(#[br(count = length)]Vec<u8>),
}


// Epoch is set at 2000-01-01T00:00:00Z
const EPOCH: i64 = 946684800;

fn parse_time(time: u32) -> DateTime<chrono::Utc> {
    DateTime::<chrono::Utc>
        ::from_timestamp(EPOCH + i64::from(time), 0)
        .expect("Invalid timestamp")
}

#[derive(BinRead, Debug, Clone)]
#[br(big)]
pub struct ProdosFileInfo {
    pub access: u16,
    pub file_type: u16,
    pub aux_type: u32,
}

#[derive(BinRead, Debug, Clone)]
#[br(big)]
pub struct FileDatesInfo {
    #[br(map = |timestamp: u32| parse_time(timestamp))]
    pub create_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: u32| parse_time(timestamp))]
    pub modification_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: u32| parse_time(timestamp))]
    pub backup_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: u32| parse_time(timestamp))]
    pub access_time: DateTime<chrono::Utc>,
}

#[test]
fn test_apple_single_parsing() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("test.as");
    let mut file = File::open(path).expect("Can't open file");

    let data = AppleSingleFile::read(&mut file).expect("Can't read file");

    assert_eq!(data.version, 0x00020000);
    assert_eq!(data.num_entries, 2);
    assert_eq!(data.entries[0].r#type, EntryType::DataFork);
    assert_eq!(data.entries[1].r#type, EntryType::ProdosFileInfo);
}