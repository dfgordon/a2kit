
//! ### CP/M Packing Module
//! 
//! Functions to help pack or unpack dates, filenames, and passwords.
//! N.b. CP/M passwords are stored with a trivial encryption algorithm
//! and should not be considered secure.

use chrono::{Timelike,Duration};
use std::str::FromStr;
use a2kit_macro::DiskStruct;
use super::types;
use super::{Packer,Error};
use super::super::{Packing,FileImage,UnpackedData};
use crate::{STDRESULT,DYNERR};
const RCH: &str = "unreachable was reached";

pub fn pack_date(time: Option<chrono::NaiveDateTime>) -> [u8;4] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let ref_date = chrono::NaiveDate::from_ymd_opt(1978, 1, 1).unwrap()
        .and_hms_opt(0,0,0).unwrap();
    let days = match now.signed_duration_since(ref_date).num_days() {
        d if d>u16::MAX as i64 => {
            log::warn!("timestamp is pegged at {} days after reference date",u16::MAX);
            u16::to_le_bytes(u16::MAX)
        },
        d if d<0 => {
            log::warn!("date prior to reference date, pegging to reference date");
            [0,0]
        }
        d => u16::to_le_bytes(d as u16 + 1)
    };
    let hours = (now.hour() / 10)*16 + now.hour() % 10;
    let minutes = (now.minute() / 10)*16 + now.minute() % 10;
    return [days[0],days[1],hours as u8,minutes as u8];
}

// TODO: return an option
pub fn unpack_date(cpm_date: [u8;4]) -> chrono::NaiveDateTime {
    let ref_date = chrono::NaiveDate::from_ymd_opt(1978, 1, 1).unwrap();
    let now = ref_date + Duration::days(u16::from_le_bytes([cpm_date[0],cpm_date[1]]) as i64 - 1);
    let hours = (cpm_date[2] & 0x0f) + 10*(cpm_date[2] >> 4);
    let minutes = (cpm_date[3] & 0x0f) + 10*(cpm_date[3] >> 4);
    return now.and_hms_opt(hours.into(), minutes.into(), 0).unwrap();
}

/// Take string such as `2:USER2.TXT` and return (2,"USER2.TXT")
pub fn split_user_filename(xname: &str) -> Result<(u8,String),DYNERR> {
    let parts: Vec<&str> = xname.split(':').collect();
    if parts.len()==1 {
        return Ok((0,xname.to_string()));
    } else {
        if let Ok(user) = u8::from_str(parts[0]) {
            if user<types::USER_END {
                return Ok((user,parts[1].to_string()));
            } else {
                log::error!("invalid user number");
                return Err(Box::new(types::Error::BadFormat));
            }
        }
        log::error!("prefix in this context should be a user number");
        return Err(Box::new(types::Error::BadFormat));
    }
}

/// Accepts lower case, case is raised by string_to_file_name.
/// Does not accept user number prefix (use is_xname_valid).
pub fn is_name_valid(name: &str) -> bool {
    let it: Vec<&str> = name.split('.').collect();
    if it.len()>2 {
        return false;
    }
    let base = it[0];
    let ext = match it.len() {
        1 => "",
        _ => it[1]
    };

    for char in [base,ext].concat().chars() {
        if !char.is_ascii() || types::INVALID_CHARS.contains(char) || char.is_ascii_control() {
            log::debug!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if base.len()>8 {
        log::info!("base name too long, max 8");
        return false;
    }
    if ext.len()>3 {
        log::info!("extension name too long, max 3");
        return false;
    }
    true
}

pub fn is_xname_valid(xname: &str) -> bool {
    if let Ok((user,name)) = split_user_filename(xname) {
        is_name_valid(&name) && user < 16
    } else {
        false
    }
}

/// put the filename bytes as an ASCII string, result can be tested for validity
/// with `is_name_valid`
pub fn file_name_to_string(name: [u8;8],typ: [u8;3]) -> String {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    [
        &String::from_utf8(base).expect(RCH).trim_end(),
        ".",
        &String::from_utf8(ext).expect(RCH).trim_end(),
    ].concat()
}

/// put the filename bytes as a split ASCII string (name,type)
pub fn file_name_to_split_string(name: [u8;8],typ: [u8;3]) -> (String,String) {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    (   String::from_utf8(base).expect(RCH).trim_end().to_string(),
        String::from_utf8(ext).expect(RCH).trim_end().to_string()
    )
}

/// put the filename bytes as an ASCII string with hex escapes
pub fn file_name_to_string_escaped(name: [u8;8],typ: [u8;3]) -> String {
    // in CP/M high bits are explicitly not part of the name
    let base: Vec<u8> = name.iter().map(|x| x & 0x7f).collect();
    let ext: Vec<u8> = typ.iter().map(|x| x & 0x7f).collect();
    let base_str = crate::escaped_ascii_from_bytes(&base, true, false);
    let ext_str = crate::escaped_ascii_from_bytes(&ext, true, false);
    match ext_str.trim_end().len() {
        0 => base_str.trim_end().to_string(),
        _ => [base_str.trim_end(),".",ext_str.trim_end()].concat()
    }
}

/// Convert string to name and type bytes for directory.
/// Assumes string contains a valid filename.
pub fn string_to_file_name(s: &str) -> ([u8;8],[u8;3]) {
    let mut ans: ([u8;8],[u8;3]) = ([0;8],[0;3]);
    let upper = s.to_uppercase();
    let it: Vec<&str> = upper.split('.').collect();
    let base = it[0].as_bytes().to_vec();
    let ext = match it.len() {
        1 => Vec::new(),
        _ => it[1].as_bytes().to_vec()
    };
    for i in 0..8 {
        if i<base.len() {
            ans.0[i] = base[i];
        } else {
            ans.0[i] = 0x20;
        }
    }
    for i in 0..3 {
        if i<ext.len() {
            ans.1[i] = ext[i];
        } else {
            ans.1[i] = 0x20;
        }
    }
    return ans;
}

/// Accepts lower case, case is raised by string_to_password
pub fn is_password_valid(s: &str) -> bool {
    for char in s.chars() {
        if !char.is_ascii() || types::INVALID_CHARS.contains(char) || char.is_ascii_control() {
            log::debug!("bad password character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if s.len()>8 {
        log::info!("password too long, max 8");
        return false;
    }
    true
}

/// Convert password to (decoder,encrypted bytes) for directory.
/// Assumes string contains a valid password.
pub fn string_to_password(s: &str) -> (u8,[u8;8]) {
    // assumes is_password_valid was true;
    let mut ans: (u8,[u8;8]) = (0,[0;8]);
    let decrypted = s.to_uppercase().as_bytes().to_vec();
    for i in 0..8 {
        let delta = match i<decrypted.len() { true => decrypted[i], false => 0x20 };
        ans.0 = ((ans.0 as u16 + delta as u16) % 256) as u8;
    }
    for i in 0..8 {
        if i<decrypted.len() {
            ans.1[7-i] = ans.0 ^ decrypted[i];
        } else {
            ans.1[7-i] = ans.0 ^ 0x20;
        }
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

    fn set_path(&self, fimg: &mut FileImage, xname: &str) -> STDRESULT {
        if is_xname_valid(xname) {
            fimg.full_path = xname.to_string();
            Ok(())
        } else {
            Err(Box::new(Error::BadFormat))
        }
    }
    
    fn get_load_address(&self,_fimg: &FileImage) -> usize {
        0
    }

    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
        Self::verify(fimg)?;
        let typ = String::from_utf8(fimg.fs_type.clone())?;
        match typ.as_str() {
            "TXT" | "ASM" | "SUB" => {
                let maybe = self.unpack_txt(fimg)?;
                if super::super::null_fraction(&maybe) < 0.01 {
                    Ok(UnpackedData::Text(maybe))
                } else {
                    Ok(UnpackedData::Binary(self.unpack_raw(fimg,true)?))
                }
            },
            _ => {
                let maybe = self.unpack_txt(fimg)?;
                if super::super::null_fraction(&maybe) == 0.0 {
                    Ok(UnpackedData::Text(maybe))
                } else {
                    Ok(UnpackedData::Binary(self.unpack_bin(fimg)?))
                }
            }
        }
    }

    fn pack_raw(&self,fimg: &mut FileImage,dat: &[u8]) -> STDRESULT {
        Self::verify(fimg)?;
        fimg.desequence(&dat);
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
        if load_addr.is_some() {
            log::warn!("load-address is not used with CP/M");
        }
        let padded = match trailing {
            Some(v) => [dat,v].concat(),
            None => dat.to_vec()
        };
        fimg.desequence(&padded);
        Ok(())
    }

    fn unpack_bin(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        self.unpack_raw(fimg,true)
    }

    fn pack_txt(&self,fimg: &mut FileImage,txt: &str) -> STDRESULT {
        Self::verify(fimg)?;
        let file = types::SequentialText::from_str(&txt)?;
        fimg.desequence(&file.to_bytes());
        Ok(())
    }

    fn unpack_txt(&self,fimg: &FileImage) -> Result<String,DYNERR> {
        Self::verify(fimg)?;
        let file = types::SequentialText::from_bytes(&fimg.sequence())?;
        Ok(file.to_string())
    }

    fn pack_tok(&self,_fimg: &mut FileImage,_tok: &[u8],_lang: crate::commands::ItemType,_trailing: Option<&[u8]>) -> STDRESULT {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }

    fn unpack_tok(&self,_fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }

    fn pack_rec_str(&self,_fimg: &mut FileImage,_json: &str) -> STDRESULT {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }

    fn unpack_rec_str(&self,_fimg: &FileImage,_rec_len: Option<usize>,_indent: Option<u16>) -> Result<String,crate::DYNERR> {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }

    fn pack_rec(&self,_fimg: &mut FileImage,_recs: &crate::fs::Records) -> STDRESULT {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }

    fn unpack_rec(&self,_fimg: &FileImage,_rec_len: Option<usize>) -> Result<crate::fs::Records,crate::DYNERR> {
        log::error!("CP/M implementation does not support operation");
        return Err(Box::new(Error::Select));
    }
}
