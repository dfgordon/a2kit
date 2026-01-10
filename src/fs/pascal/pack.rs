use chrono::Datelike;
use std::str::FromStr;
use num_traits::FromPrimitive;
use a2kit_macro::DiskStruct;
use super::types::*;
use super::{Packer,Error};
use super::super::{Packing,Records,FileImage,UnpackedData};
use super::super::fimg::r#as::AppleSingleFile;
use crate::{STDRESULT,DYNERR};

pub fn pack_date(time: Option<chrono::NaiveDateTime>) -> [u8;2] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let (_is_common_era,year) = now.year_ce();
    let packed_date = (now.month() + (now.day() << 4) + ((year%100) << 9)) as u16;
    return u16::to_le_bytes(packed_date);
}

pub fn unpack_date(pascal_date: [u8;2]) -> chrono::NaiveDateTime {
    let date = u16::from_le_bytes(pascal_date);
    let year = 1900 + (date >> 9); // choose to stay in the 20th century (Y2K bug)
    let month = date & 15;
    let day = (date >> 4) & 31;
    return chrono::NaiveDate::from_ymd_opt(year as i32,month as u32,day as u32).unwrap()
        .and_hms_opt(0, 0, 0).unwrap();
}

/// This will accept lower case; case will be automatically converted as appropriate
pub fn is_name_valid(s: &str,is_vol: bool) -> bool {
    for char in s.chars() {
        if !char.is_ascii() || INVALID_CHARS.contains(char) || char.is_ascii_control() {
            log::debug!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if s.len()<1 {
        log::info!("name is empty");
        return false;
    }
    if s.len()>7 && is_vol {
        log::info!("volume name too long, max 7");
        return false;
    }
    if s.len()>15 && !is_vol {
        log::info!("file name too long, max 15");
        return false;
    }
    true
}

pub fn file_name_to_string(fname: [u8;15],len: u8) -> String {
    // UTF8 failure will cause panic
    let copy = fname[0..len as usize].to_vec();
    if let Ok(result) = String::from_utf8(copy) {
        return result.trim_end().to_string();
    }
    panic!("encountered a bad file name");
}

pub fn vol_name_to_string(fname: [u8;7],len: u8) -> String {
    // UTF8 failure will cause panic
    let copy = fname[0..len as usize].to_vec();
    if let Ok(result) = String::from_utf8(copy) {
        return result.trim_end().to_string();
    }
    panic!("encountered a bad file name");
}

pub fn string_to_file_name(s: &str) -> [u8;15] {
    // this panics if the argument is invalid; 
    let mut ans: [u8;15] = [0;15]; // load with null
    let mut i = 0;
    if !is_name_valid(s, false) {
        panic!("attempt to create a bad file name")
    }
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    return ans;
}

pub fn string_to_vol_name(s: &str) -> [u8;7] {
    // this panics if the argument is invalid; 
    let mut ans: [u8;7] = [0;7]; // load with null
    let mut i = 0;
    if !is_name_valid(s, true) {
        panic!("attempt to create a bad volume name")
    }
    for char in s.to_uppercase().chars() {
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    return ans;
}

impl Packer {
    pub fn new() -> Self {
        Self {}
    }
    fn verify(fimg: &FileImage) -> STDRESULT {
        if &fimg.file_system != super::FS_NAME {
            return Err(Box::new(Error::BadFormat));
        }
        Ok(())
    }
}

impl Packing for Packer {
    fn set_path(&self, fimg: &mut FileImage, name: &str) -> STDRESULT {
        if is_name_valid(name,false) {
            fimg.full_path = name.to_string();
            Ok(())
        } else {
            Err(Box::new(Error::BadFormat))
        }
    }
    fn get_load_address(&self,_fimg: &FileImage) -> usize {
        0
    }
    fn pack(&self,fimg: &mut FileImage, dat: &[u8], load_addr: Option<usize>) -> STDRESULT {
        if AppleSingleFile::test(dat) {
            log::error!("cannot auto pack AppleSingle");
            Err(Box::new(Error::BadFormat))
        } else if dat.is_ascii() {
            if Records::test(dat) {
                log::error!("cannot auto pack records");
                Err(Box::new(Error::BadFormat))
            } else if FileImage::test(dat) {
                log::info!("auto packing FileImage as FileImage");
                *fimg = FileImage::from_json(str::from_utf8(dat)?)?;
                Ok(())
            } else {
                log::info!("auto packing text as FileImage");
                self.pack_txt(fimg,str::from_utf8(dat)?)
            }
        } else {
            log::info!("auto packing binary as FileImage");
            self.pack_bin(fimg,dat,load_addr,None)
        }
    }
    
    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
        Self::verify(fimg)?;
        let typ = fimg.fs_type[0];
        match FileType::from_u8(typ) {
            Some(FileType::Text) => {
                let maybe = self.unpack_txt(fimg)?;
                if super::super::null_fraction(&maybe) < 0.01 {
                    Ok(UnpackedData::Text(maybe))
                } else {
                    Ok(UnpackedData::Binary(self.unpack_raw(fimg,true)?))
                }
            },
            _ => Ok(UnpackedData::Binary(self.unpack_bin(fimg)?))
        }
    }
    fn pack_raw(&self,fimg: &mut FileImage,dat: &[u8]) -> STDRESULT {
        Self::verify(fimg)?;
        fimg.desequence(dat,None);
        fimg.fs_type = vec![FileType::Text as u8,0];
        fimg.eof = u32::to_le_bytes(dat.len() as u32).to_vec();
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
    fn pack_bin(&self,fimg: &mut FileImage,bin: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> STDRESULT {
        Self::verify(fimg)?;
        if load_addr.is_some() {
            log::warn!("load-address is not used for Pascal");
        }
        let padded = match trailing {
            Some(v) => [bin,v].concat(),
            None => bin.to_vec()
        };
        fimg.desequence(&padded,None);
        fimg.fs_type = vec![FileType::Data as u8,0];
        Ok(())
    }
    fn unpack_bin(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        Self::verify(fimg)?;
        let eof = fimg.get_eof();
        Ok(fimg.sequence_limited(eof))
    }
    fn pack_tok(&self,_fimg: &mut FileImage,_tok: &[u8],_lang: crate::commands::ItemType,_trailing: Option<&[u8]>) -> STDRESULT {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn unpack_tok(&self,_fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn pack_txt(&self,fimg: &mut FileImage,txt: &str) -> STDRESULT {
        Self::verify(fimg)?;
        let dat = SequentialText::from_str(txt)?.to_bytes();
        fimg.desequence(&dat,None);
        fimg.fs_type = vec![FileType::Text as u8,0];
        // The encoder is keeping the trailing zeros to end of page
        let mut bytes_remaining: u32 = 0;
        for i in (0..dat.len()).rev() {
            if dat[i]!=0 {
                break;
            }
            bytes_remaining += 1;
        }
        // it seems the bytes remaining is truncated to block boundaries
        fimg.eof = u32::to_le_bytes(dat.len() as u32 - 512*(bytes_remaining/512)).to_vec();
        Ok(())
    }
    fn unpack_txt(&self,fimg: &FileImage) -> Result<String,DYNERR> {
        Self::verify(fimg)?;
        let eof = fimg.get_eof();
        let dat = fimg.sequence_limited(eof);
        if dat.len()<TEXT_PAGE+1 {
            log::error!("file too small to be pascal text file");
            return Err(Box::new(Error::BadFormat));
        }
        Ok(SequentialText::from_bytes(&dat)?.to_string())
    }
    fn pack_rec(&self,_fimg: &mut FileImage,_recs: &crate::fs::Records) -> STDRESULT {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn unpack_rec(&self,_fimg: &FileImage,_rec_len: Option<usize>) -> Result<crate::fs::Records,DYNERR> {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn pack_rec_str(&self,_fimg: &mut FileImage,_json: &str) -> STDRESULT {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
    fn unpack_rec_str(&self,_fimg: &FileImage,_rec_len: Option<usize>,_indent: Option<u16>) -> Result<String,DYNERR> {
        log::error!("pascal implementation does not support operation");
        Err(Box::new(Error::DevErr))
    }
}
