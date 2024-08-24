use num_traits::FromPrimitive;
use std::str::FromStr;
use a2kit_macro::DiskStruct;
use super::types::*;
use super::{Packer,Error};
use super::super::{Packing,TextConversion,FileImage,UnpackedData,Records};
use crate::commands::ItemType;
use crate::{STDRESULT,DYNERR};

/// This will accept lower case; case will be automatically converted as appropriate
pub fn is_name_valid(s: &str) -> bool {
    for char in s.chars() {
        if !char.is_ascii() {
            log::debug!("non-ascii file name character `{}` (codepoint {})",char,char as u32);
            log::info!("use hex escapes to introduce arbitrary bytes");
            return false;
        }
    }
    if s.len()<1 {
        log::info!("file name is empty");
        return false;
    }
    if s.len()>30 {
        log::info!("file name too long, max 30");
        return false;
    }
    true
}

pub fn file_name_to_string(fname: [u8;30]) -> String {
    // fname is negative ASCII padded to the end with spaces
    // non-ASCII will go as hex escapes
    return String::from(crate::escaped_ascii_from_bytes(&fname.to_vec(),true,true).trim_end());
}

pub fn string_to_file_name(s: &str) -> [u8;30] {
    if s.len()> 30 {
        panic!("DOS filename was loo long");
    }
    let mut ans: [u8;30] = [0xa0;30]; // fill with negative spaces
    let unescaped = crate::escaped_ascii_to_bytes(s, true);
    for i in 0..30 {
        if i<unescaped.len() {
            ans[i] = unescaped[i];
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
            return Err(Box::new(Error::VolumeMismatch));
        }
        Ok(())
    }
}

impl Packing for Packer {
    fn set_path(&self, fimg: &mut FileImage, name: &str) -> STDRESULT {
        if is_name_valid(name) {
            fimg.full_path = name.to_string();
            Ok(())
        } else {
            Err(Box::new(Error::SyntaxError))
        }
    }
    fn get_load_address(&self,fimg: &FileImage) -> u16 {
        match FileType::from_u8(fimg.fs_type[0] & 0x7f) {
            Some(FileType::Integer) => 0,
            Some(FileType::Applesoft) => {
                match fimg.chunks.get(&0) {
                    Some(chunk) => match chunk.len()>2 {
                        true => crate::lang::applesoft::deduce_address(&chunk[2..]),
                        false => 0
                    },
                    None => 0
                }
            },
            Some(FileType::Binary) => {
                match fimg.chunks.get(&0) {
                    Some(chunk) => match chunk.len()>2 {
                        true => u16::from_le_bytes([chunk[0],chunk[1]]),
                        false => 0
                    },
                    None => 0
                }
            },
            _ => 0
        }
    }
    fn unpack(&self,fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
        Self::verify(fimg)?;
        let typ = fimg.fs_type[0] & 0x7f;
        match FileType::from_u8(typ) {
            Some(FileType::Text) => {
                let maybe = self.unpack_txt(fimg)?;
                if super::super::null_fraction(&maybe) < 0.01 {
                    Ok(UnpackedData::Text(maybe))
                } else {
                    let raw = self.unpack_raw(fimg,false)?;
                    if let Some(slice) = raw.split(|x| *x==0).next() {
                        Ok(UnpackedData::Binary(slice.to_vec()))
                    } else {
                        Ok(UnpackedData::Binary(raw))
                    }
                }
            },
            Some(FileType::Binary) => Ok(UnpackedData::Binary(self.unpack_bin(fimg)?)),
            Some(FileType::Applesoft) | Some(FileType::Integer) => Ok(UnpackedData::Binary(self.unpack_tok(fimg)?)),
            _ => Err(Box::new(Error::FileTypeMismatch))
        }
    }
    fn pack_raw(&self,fimg: &mut FileImage,dat: &[u8]) -> STDRESULT {
        Self::verify(fimg)?;
        fimg.desequence(dat);
        fimg.fs_type = vec![FileType::Text as u8];
        Ok(())
    }
    fn unpack_raw(&self,fimg: &FileImage,_trunc: bool) -> Result<Vec<u8>,DYNERR> {
        Self::verify(fimg)?;
        // eof is not generally available in DOS 3.x
        Ok(fimg.sequence())
    }
    fn pack_bin(&self,fimg: &mut FileImage,dat: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> STDRESULT {
        Self::verify(fimg)?;
        if let Some(addr) = load_addr {
            let file = BinaryData::pack(dat,u16::try_from(addr)?);
            let padded = match trailing {
                Some(v) => [file.to_bytes(),v.to_vec()].concat(),
                None => file.to_bytes()
            };
            fimg.desequence(&padded);
            fimg.fs_type = vec![FileType::Binary as u8];
            return Ok(());
        }
        log::error!("load-address not provided");
        return Err(Box::new(Error::SyntaxError));
    }
    fn unpack_bin(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        Self::verify(fimg)?;
        let ans = BinaryData::from_bytes(&fimg.sequence())?;
        Ok(ans.data)
    }
    fn pack_txt(&self,fimg: &mut FileImage,txt: &str) -> STDRESULT {
        Self::verify(fimg)?;
        let file = SequentialText::from_str(&txt)?;
        fimg.desequence(&file.to_bytes());
        fimg.fs_type = vec![FileType::Text as u8];
        Ok(())
    }
    fn unpack_txt(&self,fimg: &FileImage) -> Result<String,DYNERR> {
        Self::verify(fimg)?;
        let file = SequentialText::from_bytes(&fimg.sequence())?;
        Ok(file.to_string())
    }
    fn pack_tok(&self,fimg: &mut FileImage,tok: &[u8],lang: ItemType,trailing: Option<&[u8]>) -> STDRESULT {
        Self::verify(fimg)?;
        let padded = TokenizedProgram::pack(&tok,trailing).to_bytes();
        let fs_type = match lang {
            ItemType::ApplesoftTokens => FileType::Applesoft,
            ItemType::IntegerTokens => FileType::Integer,
            _ => return Err(Box::new(Error::FileTypeMismatch))
        };
        fimg.desequence(&padded);
        fimg.fs_type = vec![fs_type as u8];
        Ok(())
    }
    fn unpack_tok(&self,fimg: &FileImage) -> Result<Vec<u8>,DYNERR> {
        Self::verify(fimg)?;
        let tokens = TokenizedProgram::from_bytes(&fimg.sequence())?.program;
        Ok(tokens)
    }
    fn pack_rec(&self,fimg: &mut FileImage,recs: &Records) -> STDRESULT {
        Self::verify(fimg)?;
        let encoder = TextConverter::new(vec![0x8d]);
        fimg.fs_type = vec![FileType::Text as u8];
        recs.update_fimg(fimg, false, encoder, true)     
    }
    fn unpack_rec(&self,fimg: &FileImage,rec_len: Option<usize>) -> Result<Records,DYNERR> {
        Self::verify(fimg)?;
        if let Some(l) = rec_len {
            if l > 0 && l < 32768 {
                let encoder = TextConverter::new(vec![0x8d]);
                return Records::from_fimg(&fimg,l,encoder);
            }
        }
        log::error!("DOS 3.x requires specifying a record length from 1-32767");
        Err(Box::new(Error::Range))
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