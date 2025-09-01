//! ### AppleSingle (as)
//!
//! This module handles the parsing of AppleSingle files, a file format developed by Apple to store
//! both data and resource forks of a file in a single entity. It supports version 1 and 2 of the format,
//! and it uses the `binrw` crate to facilitate reading and interpreting the binary structures.

use binrw::io::SeekFrom;
use std::fmt::Display;
use binrw::{BinRead,BinWrite};
use chrono::DateTime;
use crate::DYNERR;

/// encode a string as bytes with fixed length, either truncating or padding with zeros as needed
fn fixed_len_str(s: &String, fixed_len: usize) -> Vec<u8> {
    let full = s.as_bytes();
    let padding = fixed_len as isize - full.len() as isize;
    match padding > 0 {
        true => [full.to_vec(),vec![0;padding as usize]].concat(),
        false => full[0..fixed_len].to_vec(),
    }
}

/// AppleSingle does not speak Apple DOS 3.x
fn prodos_to_dos_type(typ: u16) -> u8 {
    match typ {
        0x04 => 0, // txt
        0xfa => 1, // integer
        0xfc => 2, // applesoft
        _ => 3 // otherwise use binary
    }
}

/// AppleSingle does not speak Apple DOS 3.x
fn dos_to_prodos_type(typ: u8) -> u16 {
    match typ {
        0 => 0x04,
        1 => 0xfa,
        2 => 0xfc,
        _ => 0x06
    }
}

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big, magic = 0x00051600u32)]
#[br(assert(version == 0x00010000 || version == 0x00020000, "Unknown AppleSingle version {:X}, only version 1 and 2 are supported", version))]
pub struct AppleSingleFile {
    pub version: u32,
    #[br(count = 16, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]
    #[bw(map = |s: &String| fixed_len_str(s,16))]
    pub home_fs: String,
    num_entries: u16,
    #[br(count = num_entries)]
    pub entries: Vec<Entry>,  // Array of entries in the AppleSingle file
}

impl AppleSingleFile {
    pub fn new() -> Self {
        Self {
            version: 0x00020000,
            home_fs: String::new(), // buffer will be filled with NULL per v2 spec
            num_entries: 0,
            entries: Vec::new()
        }
    }
    /// is the data an AppleSingle, checks magic and version
    pub fn test(dat: &[u8]) -> bool {
        if dat.len() < 8 {
            false
        } else {
            dat[0..8] == [0,5,0x16,0,0,1,0,0] || dat[0..8] == [0,5,0x16,0,0,2,0,0]
        }
    }
    fn get_entry(&self, entry_type: EntryType) -> Option<&EntryData> {
        self.entries.iter().find(|e| e.r#type == entry_type).map(|e| &e.data)
    }
    /// When adding an entry, just set its offset to 0, then call this at the end.
    /// The offsets are only used during serialization.
    fn finish_entry(&mut self) {
        self.num_entries += 1;
        let mut curr_end = 26 + 12 * self.entries.len() as u32;
        for entry in &mut self.entries {
            entry.offset = curr_end;
            curr_end += entry.length;
        }
    }
    /// add the entry for the file's name on the home file system
    pub fn add_real_name(&mut self, name: &str) {
        self.entries.push(Entry {
            r#type: EntryType::RealName,
            offset: 0,
            data: EntryData::RealName(name.to_string()),
            length: name.as_bytes().len() as u32
        });
        self.finish_entry();
    }
    /// get the name on the home file system, or "UNTITLED" if not found
    pub fn get_real_name(&self) -> String {
        match self.get_entry(EntryType::RealName) {
            Some(EntryData::RealName(name)) => name.to_owned(),
            _ => "UNTITLED".to_string()
        }
    }
    /// add the entry for the file's dates and times, None means use the unknown time marker, which is the earliest representable time
    pub fn add_dates(&mut self, create: Option<DateTime<chrono::Utc>>, modify: Option<DateTime<chrono::Utc>>, backup: Option<DateTime<chrono::Utc>>, access: Option<DateTime<chrono::Utc>>) {
        let unknown = unknown_time();
        self.entries.push(Entry {
            r#type: EntryType::FileDatesInfo,
            offset: 0,
            data: EntryData::FileDatesInfo(FileDatesInfo {
                create_time: create.unwrap_or(unknown),
                modification_time: modify.unwrap_or(unknown),
                backup_time: backup.unwrap_or(unknown),
                access_time: access.unwrap_or(unknown),
            }),
            length: 16
        });
        self.finish_entry();
    }
    /// get the accessed time or now if none
    pub fn get_access_time(&self) -> chrono::NaiveDateTime {
        match self.get_entry(EntryType::FileDatesInfo) {
            Some(EntryData::FileDatesInfo(dt)) => dt.access_time.naive_local(),
            _ => chrono::Local::now().naive_local()
        }
    }
    /// get the create time or now if none
    pub fn get_create_time(&self) -> chrono::NaiveDateTime {
        match self.get_entry(EntryType::FileDatesInfo) {
            Some(EntryData::FileDatesInfo(dt)) => dt.create_time.naive_local(),
            _ => chrono::Local::now().naive_local()
        }
    }
    /// get the modify time or now if none
    pub fn get_modify_time(&self) -> chrono::NaiveDateTime {
        match self.get_entry(EntryType::FileDatesInfo) {
            Some(EntryData::FileDatesInfo(dt)) => dt.modification_time.naive_local(),
            _ => chrono::Local::now().naive_local()
        }
    }
    /// add the data fork entry
    pub fn add_data_fork(&mut self, dat: &[u8]) {
        self.entries.push(Entry {
            r#type: EntryType::DataFork,
            offset: 0,
            data: EntryData::DataFork(dat.to_vec()),
            length: dat.len() as u32
        });
        self.finish_entry();
    }
    /// for the file systems we handle the data fork is expected to be all that matters
    pub fn get_data_fork(&self) -> Result<Vec<u8>,DYNERR> {
        match self.get_entry(EntryType::DataFork) {
            Some(EntryData::DataFork(data)) => Ok(data.clone()),
            _ => {
                log::debug!("AppleSingle file does not contain any data");
                Err(Box::new(crate::fs::Error::FileFormat))
            },
        }
    }
    /// Translate DOS 3.x info to the equivalent ProDOS info and add it to the AppleSingle.
    /// Remember write protection is the high bit of the file type.
    pub fn add_dos3x_info(&mut self, file_type: u8, load_addr: u16) {
        let access = if file_type & 0x80 > 0 {
            0xc3
        } else {
            0x01
        };
        self.add_prodos_info(dos_to_prodos_type(file_type & 0x7f),load_addr as u32,access)
    }
    /// Get DOS 3.x info as (file_type,load_addr) by translating the ProDOS info.
    /// Remember write protection is the high bit of the file type.
    pub fn get_dos3x_info(&self) -> Option<(u8,u16)> {
        match self.get_prodos_info() {
            Some((typ,aux,access)) => Some(
                (prodos_to_dos_type(typ) + match access > 1 { true => 0x80, false => 0 },
                (aux & 0xffff) as u16)
            ),
            None => None
        }
    }
    /// add the ProDOS info entry as (type,aux,access)
    pub fn add_prodos_info(&mut self, file_type: u16, aux_type: u32, access: u16) {
        self.entries.push(Entry {
            r#type: EntryType::ProdosFileInfo,
            offset: 0,
            data: EntryData::ProDOSFileInfo( ProdosFileInfo {
                file_type,
                aux_type,
                access
            }),
            length: 8
        });
        self.finish_entry();
    }
    /// get the ProDOS (type,aux,access) if it exists
    pub fn get_prodos_info(&self) -> Option<(u16,u32,u16)> {
        match self.get_entry(EntryType::ProdosFileInfo) {
            Some(EntryData::ProDOSFileInfo(file_info)) => Some((file_info.file_type,file_info.aux_type,file_info.access)),
            _ => {
                log::debug!("AppleSingle file does not contain any ProDOS file info");
                None
            },
        }
    }
    /// add the MS-DOS info entry as (attributes)
    pub fn add_msdos_info(&mut self, attrib: u8) {
        self.entries.push(Entry {
            r#type: EntryType::MsdosFileInfo,
            offset: 0,
            data: EntryData::MSDOSFileInfo( MsdosFileInfo { attrib: attrib as u16 }),
            length: 2
        });
        self.finish_entry();
    }
    /// get the MS-DOS (attrib) if it exists
    pub fn get_msdos_info(&self) -> Option<u8> {
        match self.get_entry(EntryType::MsdosFileInfo) {
            Some(EntryData::MSDOSFileInfo(file_info)) => Some((file_info.attrib & 0xff) as u8),
            _ => {
                log::debug!("AppleSingle file does not contain any MS-DOS file info");
                None
            },
        }
    }
}

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big)]
pub struct Entry {
    pub r#type: EntryType,
    offset: u32,
    length: u32,
    #[br(
        seek_before = SeekFrom::Start(offset.into()),
        restore_position,
        args { r#type: r#type, length: length }
    )]
    #[bw(
        seek_before = SeekFrom::Start(*offset as u64),
        restore_position,
        args { length: *length }
    )]
    pub data: EntryData,
}

#[derive(BinRead, BinWrite, PartialEq, Clone, Copy, Debug)]
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

#[derive(BinRead, BinWrite, Debug, Clone)]
#[br(import { r#type: EntryType, length: u32 })]
#[bw(import { length: u32 })]
pub enum EntryData {
    #[br(pre_assert(r#type == EntryType::DataFork))]
    DataFork(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::ResourceFork))]
    ResourceFork(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::RealName))]
    RealName(
        #[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]
        #[bw(map = |s: &String| fixed_len_str(s,length as usize))]
        String
    ),
    #[br(pre_assert(r#type == EntryType::Comment))]
    Comment(
        #[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]
        #[bw(map = |s: &String| fixed_len_str(s,length as usize))]
        String
    ),
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
    #[br(pre_assert(r#type == EntryType::MsdosFileInfo))]
    MSDOSFileInfo(MsdosFileInfo),
    #[br(pre_assert(r#type == EntryType::ShortName))]
    ShortName(
        #[br(count = length, map = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).to_string())]
        #[bw(map = |s: &String| fixed_len_str(s,length as usize))]
        String
    ),
    #[br(pre_assert(r#type == EntryType::AfpFileInfo))]
    AfpFileInfo(#[br(count = length)]Vec<u8>),
    #[br(pre_assert(r#type == EntryType::DirectoryId))]
    DirectoryId(#[br(count = length)]Vec<u8>),
}


// Epoch is set at 2000-01-01T00:00:00Z
const EPOCH: i64 = 946684800;

fn unknown_time() -> DateTime<chrono::Utc> {
    parse_time(i32::MIN)
}

fn parse_time(time: i32) -> DateTime<chrono::Utc> {
    DateTime::<chrono::Utc>
        ::from_timestamp(EPOCH + i64::from(time), 0)
        .expect("Invalid timestamp")
}

fn stamp_time(dt: &DateTime<chrono::Utc>) -> i32 {
    let epoch = DateTime::<chrono::Utc>
        ::from_timestamp(EPOCH,0)
        .expect("Invalid timestamp");
    dt.to_utc().signed_duration_since(epoch).num_seconds() as i32
}

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big)]
pub struct ProdosFileInfo {
    pub access: u16,
    pub file_type: u16,
    pub aux_type: u32,
}

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big)]
pub struct MsdosFileInfo {
    // Apple's documentation does not tell us how to extract the usual
    // 8 bits of FAT file attributes from these 16 bits.
    pub attrib: u16,
}

#[derive(BinRead, BinWrite, Debug, Clone)]
#[brw(big)]
pub struct FileDatesInfo {
    #[br(map = |timestamp: i32| parse_time(timestamp))]
    #[bw(map = |dt: &DateTime<chrono::Utc>| stamp_time(dt))]
    pub create_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: i32| parse_time(timestamp))]
    #[bw(map = |dt: &DateTime<chrono::Utc>| stamp_time(dt))]
    pub modification_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: i32| parse_time(timestamp))]
    #[bw(map = |dt: &DateTime<chrono::Utc>| stamp_time(dt))]
    pub backup_time: DateTime<chrono::Utc>,
    #[br(map = |timestamp: i32| parse_time(timestamp))]
    #[bw(map = |dt: &DateTime<chrono::Utc>| stamp_time(dt))]
    pub access_time: DateTime<chrono::Utc>,
}

#[test]
fn test_apple_single_parsing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fimg")
        .join("test-as-parse.as");
    let mut file = std::fs::File::open(path).expect("Can't open file");

    let data = AppleSingleFile::read(&mut file).expect("Can't read file");

    assert_eq!(data.version, 0x00020000);
    assert_eq!(data.num_entries, 2);
    assert_eq!(data.entries[0].r#type, EntryType::DataFork);
    assert_eq!(data.entries[1].r#type, EntryType::ProdosFileInfo);
}