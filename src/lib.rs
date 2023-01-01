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
//! ## Disk Encodings
//! 
//! The sequence of bits on a disk has to follow certain rules to maintain synchronization.
//! Encoding schemes were developed to represent arbitrary bits using the
//! hardware's allowed bit sequences.  There are disks that will not work on an emulator unless the
//! detailed bit stream of the original is carefully reproduced.  As a result, disk image formats
//! were invented that emulate a disk down to this level of detail.  As of this writing, the bit-level
//! formats supported by `a2kit` are `WOZ` versions 1 and 2.  High level operations with WOZ images
//! are supported to the extent that the track format and file system are supported.

pub mod fs;
pub mod lang;
pub mod img;
pub mod commands;

use img::DiskImage;
use fs::DiskFS;
use std::io::Read;
use std::fmt::Write;
use log::{warn,info};
use regex::Regex;
use hex;

/// Save the image file (make changes permanent)
pub fn save_img(disk: &mut Box<dyn DiskFS>,img_path: &str) -> Result<(),Box<dyn std::error::Error>> {
    std::fs::write(img_path,disk.get_img().to_bytes())?;
    Ok(())
}

/// Return the file system on a disk image, or None if one cannot be found.
/// If found, the file system takes ownership of the disk image.
fn try_img(img: Box<dyn DiskImage>) -> Option<Box<dyn DiskFS>> {
    if fs::dos3x::Disk::test_img(&img) {
        info!("identified DOS 3.x file system");
        return Some(Box::new(fs::dos3x::Disk::from_img(img)));
    }
    if fs::prodos::Disk::test_img(&img) {
        info!("identified ProDOS file system");
        return Some(Box::new(fs::prodos::Disk::from_img(img)));
    }
    if fs::pascal::Disk::test_img(&img) {
        info!("identified Pascal file system");
        return Some(Box::new(fs::pascal::Disk::from_img(img)));
    }
    let dpb = fs::cpm::types::DiskParameterBlock::create(&img::DiskKind::A2_525_16);
    if fs::cpm::Disk::test_img(&img,&dpb,[2,2,3]) {
        info!("identified CP/M file system on A2 disk");
        return Some(Box::new(fs::cpm::Disk::from_img(img,dpb,[2,2,3])));
    }
    let dpb = fs::cpm::types::DiskParameterBlock::create(&img::DiskKind::CPM1_8_26);
    if fs::cpm::Disk::test_img(&img,&dpb,[2,2,3]) {
        info!("identified CP/M file system on IBM SSSD disk");
        return Some(Box::new(fs::cpm::Disk::from_img(img,dpb,[2,2,3])));
    }
   return None;
}

/// Given a bytestream return a DiskFS, or Err if the bytestream cannot be interpreted.
pub fn create_fs_from_bytestream(disk_img_data: &Vec<u8>) -> Result<Box<dyn DiskFS>,Box<dyn std::error::Error>> {
    if let Some(img) = img::imd::Imd::from_bytes(disk_img_data) {
        info!("identified IMD image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    if let Some(img) = img::woz1::Woz1::from_bytes(disk_img_data) {
        info!("identified woz1 image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    if let Some(img) = img::woz2::Woz2::from_bytes(disk_img_data) {
        info!("identified woz2 image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    if let Some(img) = img::dsk_d13::D13::from_bytes(disk_img_data) {
        info!("Possible D13 image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    if let Some(img) = img::dsk_do::DO::from_bytes(disk_img_data) {
        info!("Possible DO image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    if let Some(img) = img::dsk_po::PO::from_bytes(disk_img_data) {
        info!("Possible PO image");
        if let Some(disk) = try_img(Box::new(img)) {
            return Ok(disk);
        }
    }
    warn!("cannot match any file system");
    return Err(Box::new(fs::Error::FileSystemMismatch));
}

/// Given a bytestream return a disk image without any file system.
/// N.b. the ordering cannot always be determined without the file system.
pub fn create_img_from_bytestream(disk_img_data: &Vec<u8>) -> Result<Box<dyn DiskImage>,Box<dyn std::error::Error>> {
    if let Some(img) = img::imd::Imd::from_bytes(disk_img_data) {
        info!("identified IMD image");
        return Ok(Box::new(img));
    }
    if let Some(img) = img::woz1::Woz1::from_bytes(disk_img_data) {
        info!("identified woz1 image");
        return Ok(Box::new(img));
    }
    if let Some(img) = img::woz2::Woz2::from_bytes(disk_img_data) {
        info!("identified woz2 image");
        return Ok(Box::new(img));
    }
    if let Some(img) = img::dsk_d13::D13::from_bytes(disk_img_data) {
        info!("Possible D13 image");
        return Ok(Box::new(img));
    }
    if let Some(img) = img::dsk_do::DO::from_bytes(disk_img_data) {
        info!("Possible DO image");
        return Ok(Box::new(img));
    }
    if let Some(img) = img::dsk_po::PO::from_bytes(disk_img_data) {
        info!("Possible PO image");
        return Ok(Box::new(img));
    }
    warn!("cannot match any image format");
    return Err(Box::new(img::Error::ImageTypeMismatch));
}

/// Calls `create_img_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
pub fn create_img_from_file(img_path: &str) -> Result<Box<dyn DiskImage>,Box<dyn std::error::Error>> {
    match std::fs::read(img_path) {
        Ok(disk_img_data) => create_img_from_bytestream(&disk_img_data),
        Err(e) => Err(Box::new(e))
    }
}

/// Calls `create_fs_from_bytestream` getting the bytes from stdin
pub fn create_fs_from_stdin() -> Result<Box<dyn DiskFS>,Box<dyn std::error::Error>> {
    let mut disk_img_data = Vec::new();
    match std::io::stdin().read_to_end(&mut disk_img_data) {
        Ok(_n) => create_fs_from_bytestream(&disk_img_data),
        Err(e) => Err(Box::new(e))
    }
}

/// Calls `create_fs_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
pub fn create_fs_from_file(img_path: &str) -> Result<Box<dyn DiskFS>,Box<dyn std::error::Error>> {
    match std::fs::read(img_path) {
        Ok(disk_img_data) => create_fs_from_bytestream(&disk_img_data),
        Err(e) => Err(Box::new(e))
    }
}

/// Display binary to stdout in columns of hex, +ascii, and -ascii
pub fn display_chunk(start_addr: u16,chunk: &Vec<u8>) {
    let mut slice_start = 0;
    loop {
        let row_label = start_addr as usize + slice_start;
        let mut slice_end = slice_start + 16;
        if slice_end > chunk.len() {
            slice_end = chunk.len();
        }
        let slice = chunk[slice_start..slice_end].to_vec();
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
        if slice_end==chunk.len() {
            break;
        }
    }
}

/// Display track bytes to stdout in columns of hex, track mnemonics
pub fn display_track(start_addr: u16,trk: &Vec<u8>) {
    let mut slice_start = 0;
    let mut addr_count = 0;
    let mut err_count = 0;
    loop {
        let row_label = start_addr as usize + slice_start;
        let mut slice_end = slice_start + 16;
        if slice_end > trk.len() {
            slice_end = trk.len();
        }
        let mut mnemonics = String::new();
        for i in slice_start..slice_end {
            let bak = match i {
                x if x>0 => trk[x-1],
                _ => 0
            };
            let fwd = match i {
                x if x+1<trk.len() => trk[x+1],
                _ => 0
            };
            if !img::disk525::DISK_BYTES_62.contains(&trk[i]) && trk[i]!=0xaa && trk[i]!=0xd5 {
                mnemonics += "?";
                err_count += 1;
            } else if addr_count>0 {
                if addr_count%2==1 {
                    write!(&mut mnemonics,"{:X}",img::disk525::decode_44([trk[i],fwd]) >> 4).unwrap();
                } else {
                    write!(&mut mnemonics,"{:X}",img::disk525::decode_44([bak,trk[i]]) & 0x0f).unwrap();
                }
                addr_count += 1;
            } else {
                mnemonics += match (bak,trk[i],fwd) {
                    (0xff,0xff,_) => ">",
                    (_,0xff,0xff) => ">",
                    (_,0xd5,0xaa) => "(",
                    (0xd5,0xaa,0x96|0xb5) => "A",
                    (0xaa,0x96|0xb5,_) => {addr_count=1;":"},
                    (0xd5,0xaa,0xad) => "D",
                    (0xaa,0xad,_) => ":",
                    (_,0xde,0xaa) => ":",
                    (0xde,0xaa,0xeb) => ":",
                    (0xaa,0xeb,_) => ")",
                    (_,0xd5,_) => "R",
                    (_,0xaa,_) => "R",
                    _ => "."
                };
            }
            if addr_count==9 {
                addr_count = 0;
            }
        }
        for _i in mnemonics.len()..16 {
            mnemonics += " ";
        }
        print!("{:04X} : ",row_label);
        for byte in trk[slice_start..slice_end].to_vec() {
            print!("{:02X} ",byte);
        }
        for _blank in slice_end..slice_start+16 {
            print!("   ");
        }
        println!("|{}|",mnemonics);
        slice_start += 16;
        if slice_end==trk.len() {
            break;
        }
    }
    if err_count > 0 {
        println!();
        println!("Encountered {} invalid bytes",err_count);
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