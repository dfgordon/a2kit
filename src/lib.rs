//! # `a2kit` main library
//! 
//! This library manipulates disk images that can be used with Apple II emulators.
//! Manipulations can be done at a level as low as track bits, or as high as language files.
//! 
//! ## Architecture
//! 
//! Disk image operations are built around three trait objects:
//! * `img::DiskImage` encodes/decodes disk tracks, does not try to interpret a file system
//! * `fs::DiskFS` imposes a file system on the already decoded track data
//! * `fs::FileImage` provides a representation of a file that can be restored to a disk image
//! 
//! When a `DiskFS` object is created it takes ownership of some `DiskImage`.
//! It then uses this owned image as storage.  Any changes are not permanent until the
//! image is saved to whatever file system is hosting a2kit.
//! 
//! ## Language Files
//! 
//! Language services are built on tree-sitter parsers.  Generalized syntax checking is in `lang`.
//! Specific language services are in modules named after the language, at present:
//! * `lang::applesoft` handles (de)tokenization of Applesoft BASIC
//! * `lang::integer` handles (de)tokenization of Integer BASIC
//! * `lang::merlin` handles encodings for Merlin assembly source files
//! * Pascal source files are handled through the file system module
//! 
//! ## File Systems
//! 
//! In order to manipulate files, `a2kit` must understand the file system it finds on the disk image.
//! As of this writing `a2kit` supports
//! * CP/M 1,2, some 3
//! * DOS 3.x
//! * ProDOS
//! * Pascal File System
//! 
//! ## Disk Images
//! 
//! In order to manipulate tracks and sectors, `a2kit` must understand the way the track data is packed
//! into a disk image.  As of this writing `a2kit` supports
//! * DSK, D13, DO, PO
//! * WOZ1, WOZ2
//! * IMD
//! 
//! ## Disk Kinds
//! 
//! A disk image can typically represent some number of disk kinds (defined by mechanical and
//! encoding characteristics).  The kinds `a2kit` supports include
//! * Logical ProDOS volumes
//! * 3.5 inch Apple formats (400K/800K)
//! * 5.25 inch Apple formats (114K/140K)
//! * 8 inch CP/M formats (IBM 250K, Nabu 1M, TRS-80 600K)
//! * 5.25 inch CP/M formats (Osborne 100K/200K, Kaypro 200K/400K)

pub mod fs;
pub mod lang;
pub mod bios;
pub mod img;
pub mod commands;

use img::DiskImage;
use fs::DiskFS;
use std::io::Read;
use std::fmt::Write;
use log::{warn,info};
use regex::Regex;
use hex;

type DYNERR = Box<dyn std::error::Error>;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

const KNOWN_FILE_EXTENSIONS: &str = "dsk,d13,do,po,woz,imd";

/// Save the image file (make changes permanent)
pub fn save_img(disk: &mut Box<dyn DiskFS>,img_path: &str) -> STDRESULT {
    std::fs::write(img_path,disk.get_img().to_bytes())?;
    Ok(())
}

/// Return the file system on a disk image, or None if one cannot be found.
/// If found, the file system takes ownership of the disk image.
fn try_img(mut img: Box<dyn DiskImage>) -> Option<Box<dyn DiskFS>> {
    if fs::dos3x::Disk::test_img(&mut img) {
        info!("identified DOS 3.x file system");
        return Some(Box::new(fs::dos3x::Disk::from_img(img)));
    }
    if fs::prodos::Disk::test_img(&mut img) {
        info!("identified ProDOS file system");
        return Some(Box::new(fs::prodos::Disk::from_img(img)));
    }
    if fs::pascal::Disk::test_img(&mut img) {
        info!("identified Pascal file system");
        return Some(Box::new(fs::pascal::Disk::from_img(img)));
    }
    // For CP/M we have to try all these DPB heuristically
    let dpb_list = vec![
        bios::dpb::A2_525,
        bios::dpb::CPM1,
        bios::dpb::SSSD_525,
        bios::dpb::SSDD_525_OFF1,
        bios::dpb::SSDD_525_OFF3,
        bios::dpb::DSDD_525_OFF1,
        bios::dpb::TRS80_M2,
        bios::dpb::NABU,

    ];
    for dpb in &dpb_list {
        if fs::cpm::Disk::test_img(&mut img,dpb,[2,2,3]) {
            info!("identified CP/M file system on {}",dpb);
            return Some(Box::new(fs::cpm::Disk::from_img(img,dpb.clone(),[2,2,3])));
        }
    }
   return None;
}

/// Given a bytestream return a DiskFS, or Err if the bytestream cannot be interpreted.
/// Optional `maybe_ext` restricts the image types that will be tried based on file extension.
pub fn create_fs_from_bytestream(disk_img_data: &Vec<u8>,maybe_ext: Option<&str>) -> Result<Box<dyn DiskFS>,DYNERR> {
    let ext = match maybe_ext {
        Some(x) => x.to_string().to_lowercase(),
        None => "".to_string()
    };
    if img::imd::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::imd::Imd::from_bytes(disk_img_data) {
            info!("identified IMD image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    if img::woz1::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::woz1::Woz1::from_bytes(disk_img_data) {
            info!("identified woz1 image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    if img::woz2::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::woz2::Woz2::from_bytes(disk_img_data) {
            info!("identified woz2 image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    if img::dsk_d13::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_d13::D13::from_bytes(disk_img_data) {
            info!("Possible D13 image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    if img::dsk_do::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_do::DO::from_bytes(disk_img_data) {
            info!("Possible DO image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    if img::dsk_po::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_po::PO::from_bytes(disk_img_data) {
            info!("Possible PO image");
            if let Some(disk) = try_img(Box::new(img)) {
                return Ok(disk);
            }
        }
    }
    warn!("cannot match any file system");
    return Err(Box::new(fs::Error::FileSystemMismatch));
}

/// Given a bytestream return a disk image without any file system.
/// Optional `maybe_ext` restricts the image types that will be tried based on file extension.
/// N.b. the ordering for DSK types cannot always be determined without the file system.
pub fn create_img_from_bytestream(disk_img_data: &Vec<u8>,maybe_ext: Option<&str>) -> Result<Box<dyn DiskImage>,DYNERR> {
    let ext = match maybe_ext {
        Some(x) => x.to_string().to_lowercase(),
        None => "".to_string()
    };
    if img::imd::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::imd::Imd::from_bytes(disk_img_data) {
            info!("identified IMD image");
            return Ok(Box::new(img));
        }
    }
    if img::woz1::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::woz1::Woz1::from_bytes(disk_img_data) {
            info!("identified woz1 image");
            return Ok(Box::new(img));
        }
    }
    if img::woz2::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::woz2::Woz2::from_bytes(disk_img_data) {
            info!("identified woz2 image");
            return Ok(Box::new(img));
        }
    }
    if img::dsk_d13::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_d13::D13::from_bytes(disk_img_data) {
            info!("Possible D13 image");
            return Ok(Box::new(img));
        }
    }
    if img::dsk_do::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_do::DO::from_bytes(disk_img_data) {
            info!("Possible DO image");
            return Ok(Box::new(img));
        }
    }
    if img::dsk_po::file_extensions().contains(&ext) || ext=="" {
        if let Some(img) = img::dsk_po::PO::from_bytes(disk_img_data) {
            info!("Possible PO image");
            return Ok(Box::new(img));
        }
    }
    warn!("cannot match any image format");
    return Err(Box::new(img::Error::ImageTypeMismatch));
}

/// Calls `create_img_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
/// File extension will be used to restrict image types that are tried,
/// unless the extension is unknown, in which case all will be tried.
pub fn create_img_from_file(img_path: &str) -> Result<Box<dyn DiskImage>,DYNERR> {
    match std::fs::read(img_path) {
        Ok(disk_img_data) => {
            let mut maybe_ext = img_path.split('.').last();
            if let Some(ext) = maybe_ext {
                if !KNOWN_FILE_EXTENSIONS.contains(ext) {
                    maybe_ext = None;
                }
            }
            create_img_from_bytestream(&disk_img_data,maybe_ext)
        },
        Err(e) => Err(Box::new(e))
    }
}

/// Calls `create_fs_from_bytestream` getting the bytes from stdin.
/// All image types and file systems will be tried heuristically.
pub fn create_fs_from_stdin() -> Result<Box<dyn DiskFS>,DYNERR> {
    let mut disk_img_data = Vec::new();
    match std::io::stdin().read_to_end(&mut disk_img_data) {
        Ok(_n) => create_fs_from_bytestream(&disk_img_data,None),
        Err(e) => Err(Box::new(e))
    }
}

/// Calls `create_fs_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
/// File extension will be used to restrict image types that are tried,
/// unless the extension is unknown, in which case all will be tried.
pub fn create_fs_from_file(img_path: &str) -> Result<Box<dyn DiskFS>,DYNERR> {
    match std::fs::read(img_path) {
        Ok(disk_img_data) => {
            let mut maybe_ext = img_path.split('.').last();
            if let Some(ext) = maybe_ext {
                if !KNOWN_FILE_EXTENSIONS.contains(ext) {
                    maybe_ext = None;
                }
            }
            create_fs_from_bytestream(&disk_img_data,maybe_ext)
        },
        Err(e) => Err(Box::new(e))
    }
}

/// Display binary to stdout in columns of hex, +ascii, and -ascii
pub fn display_block(start_addr: u16,block: &Vec<u8>) {
    let mut slice_start = 0;
    loop {
        let row_label = start_addr as usize + slice_start;
        let mut slice_end = slice_start + 16;
        if slice_end > block.len() {
            slice_end = block.len();
        }
        let slice = block[slice_start..slice_end].to_vec();
        let txt: Vec<u8> = slice.iter().map(|c| match *c {
            x if x<32 => '.' as u8,
            x if x<127 => x,
            _ => '.' as u8
        }).collect();
        let neg_txt: Vec<u8> = slice.iter().map(|c| match *c {
            x if x>=160 && x<255 => x - 128,
            _ => 46
        }).collect();
        print!("{:04X} : ",row_label);
        for byte in slice {
            print!("{:02X} ",byte);
        }
        for _blank in slice_end..slice_start+16 {
            print!("   ");
        }
        print!("|+| {} ",String::from_utf8_lossy(&txt));
        for _blank in slice_end..slice_start+16 {
            print!(" ");
        }
        println!("|-| {}",String::from_utf8_lossy(&neg_txt));
        slice_start += 16;
        if slice_end==block.len() {
            break;
        }
    }
}

/// This takes any bytes and makes an ascii friendly string
/// by using hex escapes, e.g., `\xFF`.
/// if `escape_cc` is true, ascii control characters are also escaped.
/// if `inverted` is true, assume we have negative ascii bytes.
pub fn escaped_ascii_from_bytes(bytes: &Vec<u8>,escape_cc: bool,inverted: bool) -> String {
    let mut result = String::new();
    let (lb,ub) = match (escape_cc,inverted) {
        (true,false) => (0x20,0x7e),
        (false,false) => (0x00,0x7f),
        (true,true) => (0xa0,0xfe),
        (false,true) => (0x80,0xff)
    };
    for i in 0..bytes.len() {
        if bytes[i]>=lb && bytes[i]<=ub {
            if inverted {
                result += std::str::from_utf8(&[bytes[i]-0x80]).expect("unreachable");
            } else {
                result += std::str::from_utf8(&[bytes[i]]).expect("unreachable");
            }
        } else {
            let mut temp = String::new();
            write!(&mut temp,"\\x{:02X}",bytes[i]).expect("unreachable");
            result += &temp;
        }
    }
    return result;
}

/// Interpret a UTF8 string as pure ascii and put into bytes.
/// Non-ascii characters are omitted from the result, but arbitrary
/// bytes can be introduced using escapes, e.g., `\xFF`.
/// if `inverted` is true the sign of the non-escaped bytes is flipped.
pub fn escaped_ascii_to_bytes(s: &str,inverted: bool) -> Vec<u8> {
    let mut ans: Vec<u8> = Vec::new();
    let patt = Regex::new(r"\\x[0-9A-Fa-f][0-9A-Fa-f]").expect("unreachable");
    let mut hexes = patt.find_iter(s);
    let mut maybe_hex = hexes.next();
    let mut curs = 0;
    let mut skip = 0;
    for c in s.chars() {
    
        if skip>0 {
            skip -= 1;
            continue;
        }
        if let Some(hex) = maybe_hex {
            if curs==hex.start() {
                ans.append(&mut hex::decode(s.get(curs+2..curs+4).unwrap()).expect("unreachable"));
                curs += 4;
                maybe_hex = hexes.next();
                skip = 3;
                continue;
            }
        }
        
        if c.is_ascii() {
            let mut buf: [u8;1] = [0;1];
            c.to_uppercase().next().unwrap().encode_utf8(&mut buf);
            ans.push(buf[0] + match inverted { true => 128, false => 0 });
        }
        curs += 1;
    }
    return ans;
}