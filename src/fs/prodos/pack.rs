use chrono::{Datelike,Timelike};
use num_traits::FromPrimitive;
use std::str::FromStr;
use a2kit_macro::DiskStruct;
use super::types::*;
use super::{Packer,Error};
use super::super::{Packing,TextConversion,FileImage,UnpackedData,Records};
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
    // Suppose the earliest date stamp we can find originates from the year before
    // SOS was released, i.e., 1979.  Use this to help decide the century.
    // This scheme will work until 2079.
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

    fn get_load_address(&self,fimg: &FileImage) -> u16 {
        fimg.get_aux() as u16
    }
    
    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
        Self::verify(fimg)?;
        let typ = fimg.fs_type[0];
        match FileType::from_u8(typ) {
            Some(FileType::Text) => {
                match fimg.get_aux() {
                    0 => Ok(UnpackedData::Text(self.unpack_txt(fimg)?)),
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
        fimg.desequence(dat);
        fimg.fs_type = vec![FileType::Text as u8];
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
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
            fimg.desequence(&padded);
            fimg.fs_type = vec![FileType::Binary as u8];
            fimg.access = vec![STD_ACCESS | DIDCHANGE];
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
        fimg.desequence(&file.to_bytes());
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
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
        fimg.desequence(&padded);
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
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
        fimg.access = vec![STD_ACCESS | DIDCHANGE];
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
}
