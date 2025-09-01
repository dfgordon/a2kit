use chrono::{Datelike,Timelike};
use num_traits::FromPrimitive;
use std::str::FromStr;
use a2kit_macro::DiskStruct;
use super::types::*;
use super::{Packer,Error};
use super::super::{Packing,TextConversion,FileImage,UnpackedData,Records};
use super::super::fimg::r#as::AppleSingleFile;
use binrw::{BinRead,BinWrite};
use crate::commands::ItemType;
use crate::{STDRESULT,DYNERR};

pub fn pack_time(time: Option<chrono::NaiveDateTime>) -> [u8;4] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let (_is_common_era,year) = now.year_ce();
    let packed_date = (now.day() + (now.month() << 5) + (year%100 << 9)) as u16;
    let packed_time = (now.minute() + (now.hour() << 8)) as u16;
    let bytes_date = u16::to_le_bytes(packed_date);
    let bytes_time = u16::to_le_bytes(packed_time);
    return [bytes_date[0],bytes_date[1],bytes_time[0],bytes_time[1]];
}

pub fn unpack_time(prodos_date_time: [u8;4]) -> Option<chrono::NaiveDateTime> {
    let date = u16::from_le_bytes([prodos_date_time[0],prodos_date_time[1]]);
    let time = u16::from_le_bytes([prodos_date_time[2],prodos_date_time[3]]);
    let yearmod100 = date >> 9;
    // There is a Y2K scheme in ProDOS tech-note #28 with valid range 1940-2039.
    // Our scheme is compatible where it likely matters and allows the range 1979-2078.
    // The notion here is to start 1 year before SOS was released.
    let year = match yearmod100 < 79 {
        true => 2000 + yearmod100,
        false => 1900 + yearmod100
    };
    let month = (date >> 5) & 15;
    let day = date & 31;
    let hour = (time >> 8) & 255;
    let minute = time & 255;
    match chrono::NaiveDate::from_ymd_opt(year as i32,month as u32,day as u32) {
        Some(date) => date.and_hms_opt(hour as u32,minute as u32,0),
        None => None
    }
}

/// unpack time from slice, if vec is wrong size return None
fn unpack_time_from_slice(vec_time: &[u8]) -> Option<chrono::DateTime<chrono::Utc>> {
    if vec_time.len() == 4 {
        match unpack_time([vec_time[0],vec_time[1],vec_time[2],vec_time[3]]) {
            Some(dt) => Some(dt.and_utc()),
            None => None
        }
    } else {
        None
    }
}

/// Test the string for validity as a ProDOS name.
/// This can be used to check names before passing to functions that may panic.
pub fn is_name_valid(s: &str) -> bool {
    let fname_patt = regex::Regex::new(r"^[A-Z][A-Z0-9.]{0,14}$").unwrap();
    if !fname_patt.is_match(&s.to_uppercase()) {
        return false;
    } else {
        return true;
    }
}

/// Test the string for validity as a ProDOS file path.
/// Directory paths ending with `/` are rejected.
/// Checks each path segment, and length of overall path.
pub fn is_path_valid(path: &str) -> bool {
    if path.len() > 128 {
        log::error!("ProDOS path is too long");
        return false;
    }
    if path.len() > 64 {
        log::warn!("ProDOS path length is {}, will need prefixing in real ProDOS",path.len());
    }
    let mut iter = path.split("/");
    if path.starts_with("/") {
        iter.next();
    }
    while let Some(segment) = iter.next() {
        if !is_name_valid(segment) {
            return false;
        }
    }
    true
}

/// Convert filename bytes to a string.  Will not panic, will escape the string if necessary.
/// Must pass the stor_len_nibs field into nibs.
pub fn file_name_to_string(nibs: u8, fname: [u8;15]) -> String {
    let name_len = nibs & 0x0f;
    if let Ok(result) = String::from_utf8(fname[0..name_len as usize].to_vec()) {
        return result;
    }
    log::warn!("continuing with invalid filename");
    crate::escaped_ascii_from_bytes(&fname[0..name_len as usize].to_vec(), true, false)
}

/// Convert storage type and String to (stor_len_nibs,fname).
/// Panics if the string is not a valid ProDOS name.
pub fn string_to_file_name(stype: &StorageType, s: &str) -> (u8,[u8;15]) {
    if !is_name_valid(s) {
        panic!("attempt to create a bad file name {}",s);
    }
    let new_nibs = ((*stype as u8) << 4) + s.len() as u8;
    let mut ans: [u8;15] = [0;15];
    let mut i = 0;
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    (new_nibs,ans)
}

impl Packer {
    pub fn new() -> Self {
        Self {}
    }
    fn verify(fimg: &FileImage) -> STDRESULT {
        if &fimg.file_system != super::FS_NAME {
            return Err(Box::new(Error::FileTypeMismatch));
        }
        Ok(())
    }
}

impl Packing for Packer {

    fn set_path(&self, fimg: &mut FileImage, path: &str) -> STDRESULT {
        if is_path_valid(path) {
            fimg.full_path = path.to_string();
            Ok(())
        } else {
            Err(Box::new(Error::Syntax))
        }
    }

    fn get_load_address(&self,fimg: &FileImage) -> usize {
        fimg.get_aux()
    }

    fn pack(&self,fimg: &mut FileImage, dat: &[u8], load_addr: Option<usize>) -> STDRESULT {
        if AppleSingleFile::test(dat) {
            log::info!("auto packing AppleSingle as FileImage");
            self.pack_apple_single(fimg, dat, load_addr)
        } else {
            log::error!("could not automatically pack");
            Err(Box::new(crate::fs::Error::FileFormat))
        }
    }
    
    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
        Self::verify(fimg)?;
        let typ = fimg.fs_type[0];
        match FileType::from_u8(typ) {
            Some(FileType::Text) => {
                match fimg.get_aux() {
                    0 => {
                        let maybe = self.unpack_txt(fimg)?;
                        if super::super::null_fraction(&maybe) < 0.01 {
                            Ok(UnpackedData::Text(maybe))
                        } else {
                            Ok(UnpackedData::Binary(self.unpack_raw(fimg,true)?))
                        }
                    },
                    _ => Ok(UnpackedData::Records(self.unpack_rec(fimg,None)?))
                }
            },
            Some(FileType::Binary) => Ok(UnpackedData::Binary(self.unpack_bin(fimg)?)),
            Some(FileType::System) => Ok(UnpackedData::Binary(self.unpack_bin(fimg)?)),
            Some(FileType::ApplesoftCode) => Ok(UnpackedData::Binary(self.unpack_tok(fimg)?)),
            Some(FileType::IntegerCode) => Ok(UnpackedData::Binary(self.unpack_tok(fimg)?)),
            _ => Err(Box::new(Error::FileTypeMismatch))
        }
    }
    
    fn pack_raw(&self,fimg: &mut FileImage,dat: &[u8]) -> STDRESULT {
        Self::verify(fimg)?;
        fimg.desequence(dat,None);
        fimg.fs_type = vec![FileType::Text as u8];
        fimg.access = vec![STD_ACCESS | Access::Backup as u8];
        Ok(())
    }
    
    fn unpack_raw(&self,fimg: &FileImage,trunc: bool) -> Result<Vec<u8>,DYNERR> {
        Self::verify(fimg)?;
        if trunc {
            let eof = fimg.get_eof();
            Ok(fimg.sequence_limited(eof))
        } else {
            Ok(fimg.sequence())
        }
    }
    
    fn pack_bin(&self,fimg: &mut FileImage,dat: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> STDRESULT {
        Self::verify(fimg)?;
        let padded = match trailing {
            Some(v) => [dat,v].concat(),
            None => dat.to_vec()
        };
        if let Some(addr) = load_addr {
            fimg.desequence(&padded,None);
            fimg.fs_type = vec![FileType::Binary as u8];
            fimg.access = vec![STD_ACCESS | Access::Backup as u8];
            fimg.aux = u16::to_le_bytes(u16::try_from(addr)?).to_vec();
            return Ok(());    
        }
        log::error!("load-address not provided");
        Err(Box::new(Error::InvalidOption))
    }
    
    fn unpack_bin(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        self.unpack_raw(fimg,true)
    }
    
    fn pack_txt(&self,fimg: &mut FileImage,txt: &str) -> STDRESULT {
        Self::verify(fimg)?;
        let file = SequentialText::from_str(txt)?;
        fimg.desequence(&file.to_bytes(),None);
        fimg.access = vec![STD_ACCESS | Access::Backup as u8];
        fimg.fs_type = vec![FileType::Text as u8];
        Ok(())
    }
    
    fn unpack_txt(&self,fimg: &FileImage) -> Result<String,DYNERR> {
        Self::verify(fimg)?;
        let dat = self.unpack_raw(fimg,true)?;
        let file = SequentialText::from_bytes(&dat)?;
        Ok(file.to_string())
    }
    
    fn pack_tok(&self,fimg: &mut FileImage,tok: &[u8],lang: ItemType,trailing: Option<&[u8]>) -> STDRESULT {
        Self::verify(fimg)?;
        let padded = match trailing {
            Some(v) => [tok,v].concat(),
            None => tok.to_vec()
        };
        fimg.desequence(&padded,None);
        fimg.access = vec![STD_ACCESS | Access::Backup as u8];
        match lang {
            ItemType::ApplesoftTokens => {
                let addr = crate::lang::applesoft::deduce_address(tok);
                fimg.fs_type = vec![FileType::ApplesoftCode as u8];
                fimg.aux = u16::to_le_bytes(addr).to_vec();
                log::debug!("Applesoft metadata {:?}, {:?}",fimg.fs_type,fimg.aux);
            },
            ItemType::IntegerTokens => {
                fimg.fs_type = vec![FileType::IntegerCode as u8];
            }
            _ => return Err(Box::new(Error::FileTypeMismatch))
        }
        Ok(())
    }
    
    fn unpack_tok(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        self.unpack_raw(fimg,true)
    }
    
    fn pack_rec(&self,fimg: &mut FileImage,recs: &Records) -> STDRESULT {
        Self::verify(fimg)?;
        let converter = TextConverter::new(vec![0x0d]);
        fimg.fs_type = vec![FileType::Text as u8];
        fimg.aux = u16::to_le_bytes(recs.record_len.try_into()?).to_vec();
        fimg.access = vec![STD_ACCESS | Access::Backup as u8];
        recs.update_fimg(fimg, true, converter, true)
    }
    
    fn unpack_rec(&self,fimg: &FileImage,rec_len: Option<usize>) -> Result<Records,DYNERR> {
        Self::verify(fimg)?;
        let l = match rec_len {
            Some(l) => {
                log::warn!("user specified record length shadows metadata");
                if l > 0 && l < 32768 {
                    l
                } else {
                    log::error!("record length must be between 1 and 32767");
                    return Err(Box::new(Error::Range));
                }
            },
            None => fimg.get_aux()
        };
        let converter = TextConverter::new(vec![0x0d]);
        Records::from_fimg(&fimg,l,converter)
    }
    fn pack_rec_str(&self,fimg: &mut FileImage,json: &str) -> STDRESULT {
        Self::verify(fimg)?;
        let recs = Records::from_json(json)?;
        self.pack_rec(fimg,&recs)
    }
    fn unpack_rec_str(&self,fimg: &FileImage,rec_len: Option<usize>,indent: Option<u16>) -> Result<String,DYNERR> {
        Self::verify(fimg)?;
        let recs = self.unpack_rec(fimg,rec_len)?;
        Ok(recs.to_json(indent))
    }
    fn pack_apple_single(&self,fimg: &mut FileImage, dat: &[u8], new_load_addr: Option<usize>) -> STDRESULT {
        let apple_single = AppleSingleFile::read(&mut std::io::Cursor::new(dat))?;
        let dat = apple_single.get_data_fork()?;
        let created = apple_single.get_create_time();
        let modified = apple_single.get_modify_time();
        match apple_single.get_prodos_info() {
            Some((typ,aux,access)) => {
                if typ > 0xff || aux > 0xffff || access > 0xff {
                    log::error!("ProDOS 16 information cannot be handled");
                    return Err(Box::new(crate::fs::Error::FileSystemMismatch));
                }
                let load_addr = match new_load_addr {
                    Some(a) => a,
                    None => aux as usize
                };
                match FileType::from_u16(typ) {
                    Some(FileType::Text) => self.pack_raw(fimg,&dat)?,
                    Some(FileType::ApplesoftCode) => self.pack_tok(fimg,&dat,ItemType::ApplesoftTokens,None)?,
                    Some(FileType::IntegerCode) => self.pack_tok(fimg,&dat,ItemType::IntegerTokens,None)?,
                    Some(FileType::Binary) | Some(FileType::System) => self.pack_bin(fimg,&dat,Some(load_addr),None)?,
                    _ => {
                        log::warn!("unknown file type being treated as binary");
                        self.pack_bin(fimg,&dat,Some(load_addr),None)?
                    }
                };
                // Only use the AppleSingle name if the FileImage name is empty; the
                // command line name is loaded into the FileImage before we get here.
                if fimg.full_path.len() == 0 {
                    fimg.full_path = apple_single.get_real_name();
                }
                fimg.access = vec![(access & 0xff) as u8];
                fimg.created = pack_time(Some(created)).to_vec();
                fimg.modified = pack_time(Some(modified)).to_vec();
                Ok(())
            },
            None => {
                log::error!("AppleSingle is missing ProDOS information");
                Err(Box::new(crate::fs::Error::FileSystemMismatch))
            }
        }
    }
    fn unpack_apple_single(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        if fimg.fs_type.len() != 1 {
            log::error!("unexpected file type in file image");
            return Err(Box::new(crate::fs::Error::FileImageFormat));
        }

        let access = match fimg.access.len() { 1 => fimg.access[0] as u16, _ => STD_ACCESS as u16 };
        let created = unpack_time_from_slice(&fimg.created);
        let modified = unpack_time_from_slice(&fimg.modified);
        let dat = match FileType::from_u8(fimg.fs_type[0]) {
            Some(FileType::Text) => self.unpack_txt(fimg)?.as_bytes().to_vec(),
            Some(FileType::ApplesoftCode) => self.unpack_tok(fimg)?,
            Some(FileType::IntegerCode) => self.unpack_tok(fimg)?,
            Some(FileType::Binary) | Some(FileType::System) => self.unpack_bin(fimg)?,
            _ => {
                log::warn!("unknown file type being treated as binary");
                self.unpack_bin(fimg)?
            }
        };

        let mut apple_single = AppleSingleFile::new();
        if !is_path_valid(&fimg.full_path) {
            log::error!("attempt to load invalid ProDOS path into AppleSingle");
            return Err(Box::new(Error::Syntax));
        }
        apple_single.add_real_name(&fimg.full_path.to_uppercase());
        apple_single.add_dates(created,modified,None,None);
        apple_single.add_prodos_info(fimg.get_ftype() as u16,fimg.get_aux() as u32, access );
        apple_single.add_data_fork(&dat);
        let mut ans = std::io::Cursor::new(Vec::new());
        AppleSingleFile::write(&mut apple_single,&mut ans)?;
        Ok(ans.into_inner())
    }
}
