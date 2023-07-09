//! ## Support for 2MG disk images
//! 
//! This format consists of a header followed by data in either DSK or NIB format.
//! At the end of the data there can be a comment and creator information.

use chrono;
use std::str::FromStr;
use log::{warn,debug,info,error};
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;
use crate::img;
use crate::img::meta;
use crate::fs::Block;
use crate::{STDRESULT,DYNERR,putHex,getHex,getHexEx,putString};

/// These are all in the header branch
const RO_META_ITEMS: [&str;9] = [
    "header_len",
    "version",
    "img_fmt",
    "data_offset",
    "data_len",
    "comment_offset",
    "comment_len",
    "creator_offset",
    "creator_len"
];

const BLOCK_SIZE: u32 = 512;

pub fn file_extensions() -> Vec<String> {
    vec!["2mg".to_string(),"2img".to_string()]
}

// all header entries are LE numbers
#[derive(DiskStruct)]
pub struct Header {
    magic: [u8;4], // always '2IMG`
    creator_id: [u8;4], // a2kit = '2KIT'
    header_len: [u8;2],
    version: [u8;2], // 1
    img_fmt: [u8;4], // 0=DO, 1=PO, 2=nib
    flags: [u8;4], // bits 0-7=volume if bit 8 (otherwise 254), disk write protected if bit 31 
    blocks: [u8;4], // set to 0 for DO images
    data_offset: [u8;4], // from start of file
    data_len: [u8;4],
    comment_offset: [u8;4],
    comment_len: [u8;4],
    creator_offset: [u8;4],
    creator_len: [u8;4], 
    pad: [u8;16]
}

pub struct Dot2mg {
    kind: img::DiskKind,
    header: Header,
    // use the strategy of wrapping another disk image
    raw_img: Box<dyn img::DiskImage>,
    comment: String,
    creator_info: String,
}

impl Dot2mg {
    pub fn create(vol: u8,kind: img::DiskKind,maybe_wrap: Option<&String>) -> Result<Box<dyn img::DiskImage>,DYNERR> {
        let now = chrono::Local::now().naive_local();
        let creator_info = "a2kit v".to_string() + env!("CARGO_PKG_VERSION") + " " + &now.format("%d-%m-%Y %H:%M:%S").to_string();
        let wrap = match maybe_wrap {
            None => None,
            Some(s) => {
                match img::DiskImageType::from_str(s) {
                    Ok(typ) => Some(typ),
                    Err(_) => panic!("create received unexpected image type string") // should not happen
                }
            }
        };
        let raw_img: Box<dyn img::DiskImage> = match (kind,wrap) {
            (img::names::A2_DOS33_KIND,Some(img::DiskImageType::DO)) => Box::new(img::dsk_do::DO::create(35,16)),
            (img::names::A2_DOS33_KIND,Some(img::DiskImageType::NIB)) => Box::new(img::nib::Nib::create(vol,kind)),
            (img::names::A2_400_KIND,Some(img::DiskImageType::PO)) => Box::new(img::dsk_po::PO::create(800)),
            (img::names::A2_800_KIND,Some(img::DiskImageType::PO)) => Box::new(img::dsk_po::PO::create(1600)),
            (img::names::A2_HD_MAX,Some(img::DiskImageType::PO)) => Box::new(img::dsk_po::PO::create(65535)),
            (img::names::A2_DOS33_KIND,None) => Box::new(img::dsk_do::DO::create(35,16)),
            (img::names::A2_400_KIND,None) => Box::new(img::dsk_po::PO::create(800)),
            (img::names::A2_800_KIND,None) => Box::new(img::dsk_po::PO::create(1600)),
            (img::names::A2_HD_MAX,None) => Box::new(img::dsk_po::PO::create(65535)),
            _ => {
                error!("the disk kind could not be paired with the wrapped image");
                return Err(Box::new(img::Error::ImageTypeMismatch));
            }
        };
        let flags = match kind {
            img::names::A2_DOS33_KIND => [vol,1,0,0],
            _ => [0,0,0,0]
        };
        let actual_blocks = raw_img.byte_capacity() as u32 / BLOCK_SIZE;
        let (fmt,blocks,buf_len) = match raw_img.what_am_i() {
            // Some sources say blocks should be 0 unless we have fmt=1 (PO).
            // However, CiderPress, for one, will reject a DO with blocks=0.
            // So we will write the blocks unconditionally.
            // When we are reading, we ignore blocks unless fmt=1.
            img::DiskImageType::DO => (0, actual_blocks, raw_img.byte_capacity() as u32),
            img::DiskImageType::PO => (1, actual_blocks, raw_img.byte_capacity() as u32),
            img::DiskImageType::NIB => (2, actual_blocks, img::nib::TRACK_BYTE_CAPACITY_NIB as u32*35),
            _ => {
                error!("attempt to wrap unsupported image type in 2MG");
                return Err(Box::new(img::Error::ImageTypeMismatch));
            }
        };
        Ok(Box::new(Self {
            kind,
            header: Header {
                magic: u32::to_be_bytes(0x32494D47), // '2IMG'
                creator_id: u32::to_be_bytes(0x324b4954), // '2KIT'
                header_len: [64,0],
                version: [1,0],
                img_fmt: [fmt,0,0,0],
                flags,
                blocks: u32::to_le_bytes(blocks),
                data_offset: [64,0,0,0],
                data_len: u32::to_le_bytes(buf_len),
                comment_offset: [0,0,0,0],
                comment_len: [0,0,0,0],
                creator_offset: u32::to_le_bytes(64 + buf_len),
                creator_len: u32::to_le_bytes(creator_info.len() as u32),
                pad: [0;16]
            },
            raw_img,
            comment: "".to_string(),
            creator_info
        }))
    }
}

impl img::DiskImage for Dot2mg {
    fn track_count(&self) -> usize {
        self.raw_img.track_count()
    }
    fn byte_capacity(&self) -> usize {
        self.raw_img.byte_capacity()
    }
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR> {
        self.raw_img.read_block(addr)
    }
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT {
        if self.header.flags[3]>127 {
            error!("2MG disk is write protected");
            return Err(Box::new(img::Error::SectorAccess));
        }
        self.raw_img.write_block(addr,dat)
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        self.raw_img.read_sector(cyl,head,sec)
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        if self.header.flags[3]>127 {
            error!("2MG disk is write protected");
            return Err(Box::new(img::Error::SectorAccess));
        }
        self.raw_img.write_sector(cyl,head,sec,dat)
    }
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        if data.len()<64 {
            return None;
        }
        let header = Header::from_bytes(&data[0..64].to_vec());
        match header.magic {
            [0x32,0x49,0x4D,0x47] => info!("identified 2MG header"),
            _ => return None
        }
        if u16::from_le_bytes(header.header_len)!=64 {
            warn!("unexpected 2MG header length {}",u16::from_le_bytes(header.header_len));
        }
        if u16::from_le_bytes(header.version)!=1 {
            warn!("unexpected 2MG version {}",u16::from_le_bytes(header.version));
        }
        let fmt = u32::from_le_bytes(header.img_fmt);
        if fmt>2 {
            error!("illegal 2MG format {}",fmt);
            return None;
        }
        let blocks = u32::from_le_bytes(header.blocks);
        let offset = u32::from_le_bytes(header.data_offset) as usize;
        let len = u32::from_le_bytes(header.data_len) as usize;
        if data.len()<offset+len {
            error!("end of data {} runs past EOF",offset+len);
            return None;
        }
        let maybe_raw_img: Option<Box<dyn img::DiskImage>> = match fmt {
            0 => {
                info!("2MG flagged as DOS ordered");
                match img::dsk_do::DO::from_bytes(&data[offset..offset+len].to_vec()) {
                    Some(im) => Some(Box::new(im)),
                    None => None
                }
            },
            1 => {
                info!("2MG flagged as ProDOS ordered");
                match img::dsk_po::PO::from_bytes(&data[offset..offset+len].to_vec()) {
                    Some(im) => Some(Box::new(im)),
                    None => None
                }
            },
            2 => {
                info!("2MG flagged as nibbles");
                match img::nib::Nib::from_bytes(&data[offset..offset+len].to_vec()) {
                    Some(im) => Some(Box::new(im)),
                    None => None
                }
            },
            _ => panic!("unhandled format")
        };
        let comment_off = u32::from_le_bytes(header.comment_offset) as usize;
        let comment_len = u32::from_le_bytes(header.comment_len) as usize;
        let mut comment = String::new();
        if data.len()<comment_off+comment_len {
            warn!("end of comment {} runs past EOF, ignoring",comment_off+comment_len);
        } else {
            comment = match String::from_utf8(data[comment_off..comment_off+comment_len].to_vec()) {
                Ok(s) => {
                    info!("2MG comment: {}",s);
                    s
                },
                _ => {
                    warn!("comment field could not be read as UTF8 string");
                    "".to_string()
                }
            };
        }
        let creator_offset = u32::from_le_bytes(header.creator_offset) as usize;
        let creator_len = u32::from_le_bytes(header.creator_len) as usize;
        let mut creator_info = String::new();
        if data.len()<creator_offset+creator_len {
            warn!("end of creator info {} runs past EOF, ignoring",creator_offset+creator_len);
        } else {
            creator_info = match String::from_utf8(data[creator_offset..creator_offset+creator_len].to_vec()) {
                Ok(s) => {
                    info!("2MG creator info: {}",s);
                    s
                },
                _ => {
                    warn!("creator info could not be read as UTF8 string");
                    "".to_string()
                }
            };
        }
        if let Some(raw_img) = maybe_raw_img {
            if fmt==1 && blocks as usize * BLOCK_SIZE as usize != raw_img.byte_capacity() {
                error!("2MG block count does not match data size");
                return None;
            }
            return Some(Self {
                kind: raw_img.kind(),
                header,
                raw_img,
                comment,
                creator_info 
            })
        }
        return None;
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::DOT2MG
    }
    fn file_extensions(&self) -> Vec<String> {
        file_extensions()
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        let buf_len = u32::from_le_bytes(self.header.data_len);
        let rem_len = self.comment.len() as u32;
        let cre_len = self.creator_info.len() as u32; 
        self.header.data_offset = u32::to_le_bytes(64);
        self.header.comment_offset = u32::to_le_bytes(match rem_len { 0 => 0, _ => 64+buf_len });
        self.header.comment_len = u32::to_le_bytes(rem_len);
        self.header.creator_offset = u32::to_le_bytes(match cre_len { 0 => 0, _ => 64+buf_len+rem_len});
        self.header.creator_len = u32::to_le_bytes(cre_len);
        ans.append(&mut self.header.to_bytes());
        ans.append(&mut self.raw_img.to_bytes());
        if !self.comment.is_ascii() {
            warn!("2MG comment is not ASCII");
        }
        ans.append(&mut self.comment.as_bytes().to_vec());
        if !self.creator_info.is_ascii() {
            warn!("2MG creator info is not ASCII");
        }
        ans.append(&mut self.creator_info.as_bytes().to_vec());
        return ans;
    }
    fn get_track_buf(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        self.raw_img.get_track_buf(cyl, head)
    }
    fn set_track_buf(&mut self,cyl: usize,head: usize,dat: &[u8]) -> STDRESULT {
        self.raw_img.set_track_buf(cyl, head, dat)
    }
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        self.raw_img.get_track_nibbles(cyl, head)
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        self.raw_img.display_track(bytes)
    }
    fn get_metadata(&self,indent: u16) -> String {
        let mg = self.what_am_i().to_string();
        let mut root = json::JsonValue::new_object();
        root[&mg] = json::JsonValue::new_object();
        getHexEx!(root,mg,self.header.creator_id);
        root[&mg]["header"]["creator_id"]["_pretty"] = json::JsonValue::String(String::from_utf8_lossy(&self.header.creator_id).into());
        getHex!(root,mg,self.header.header_len);
        getHex!(root,mg,self.header.version);
        getHexEx!(root,mg,self.header.img_fmt);
        root[&mg]["header"]["img_fmt"]["_pretty"] = json::JsonValue::String(match self.header.img_fmt {
                [0,0,0,0] => "DOS ordered sectors (DO)".to_string(),
                [1,0,0,0] => "ProDOS ordered blocks (PO)".to_string(),
                [2,0,0,0] => "Track data as nibbles (NIB)".to_string(),
                _ => "Unexpected format code".to_string()
        });
        getHex!(root,mg,self.header.flags);
        getHex!(root,mg,self.header.blocks);
        getHex!(root,mg,self.header.data_offset);
        getHex!(root,mg,self.header.data_len);
        getHex!(root,mg,self.header.comment_offset);
        getHex!(root,mg,self.header.comment_len);
        getHex!(root,mg,self.header.creator_offset);
        getHex!(root,mg,self.header.creator_len);
        root[&mg]["comment"] = json::JsonValue::String(self.comment.clone());
        root[&mg]["creator_info"] = json::JsonValue::String(self.creator_info.clone());
        if indent==0 {
            json::stringify(root)
        } else {
            json::stringify_pretty(root, indent)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            debug!("put key `{:?}` with val `{}`",key_path,val);
            meta::test_metadata(key_path, self.what_am_i())?;
            if key_path.len()>2 && key_path[1]=="header" {
                if RO_META_ITEMS.contains(&key_path[2].as_str()) {
                    warn!("skipping read-only `{}`",key_path[2]);
                    return Ok(());
                }
            }
            let mg = self.what_am_i().to_string();
            putHex!(val,key_path,mg,self.header.creator_id);
            putHex!(val,key_path,mg,self.header.flags);
            putHex!(val,key_path,mg,self.header.blocks);
            putString!(val,key_path,mg,self.comment);
            putString!(val,key_path,mg,self.creator_info);
        }
        error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}