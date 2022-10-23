//! # `a2kit` main library
//! 
//! This library manipulates disk images that can be used with Apple II emulators.
//! Manipulations can be done at a level as low as track bits, or as high as language files.
//! 
//! ## Architecture
//! 
//! Disk image operations are built around two trait objects found in the `disk_base` module:
//! * `DiskImage` encodes/decodes disk tracks, does not try to interpret a file system
//! * `A2Disk` imposes a file system on the already decoded track data
//! 
//! Internally, the `A2Disk` object contains its own track data,
//! but always in the `DSK` image format, with the sector order chosen to match the file system.
//! The `DSK` image format uses already-decoded track data from the get-go, so it is ideal for
//! running a file system.  Because the `DSK` format is at the heart of all file operations,
//! the beginning and end of many workflows involves transforming between `DSK` and
//! some other image format (including ordering variants of `DSK`)
//! 
//! Language services are built on tree-sitter parsers.  Generalized syntax checking is in `walker`.
//! Specific language services are in modules named after the language, at present:
//! * `applesoft` handles (de)tokenization of Applesoft BASIC
//! * `integer` handles (de)tokenization of Integer BASIC
//! 
//! ## File Systems
//! 
//! In order to manipulate files, `a2kit` must understand the file system it finds on the disk image.
//! As of this writing standard DOS 3.3 and ProDOS are supported.
//! 
//! ## Disk Encodings
//! 
//! The disk hardware used with the Apple II line of computers (and perhaps others)
//! could not handle an arbitrary sequence of bits, i.e., the bit sequence had to
//! follow certain rules.  Encoding schemes were developed to represent arbitrary bits using the
//! hardware's allowed bit sequences.  There are disks that will not work on an emulator unless the
//! detailed bit stream of the original is carefully reproduced.  As a result, disk image formats
//! were invented that emulate a disk down to this level of detail.  As of this writing, the bit-level
//! formats supported by `a2kit` are `WOZ` versions 1 and 2.  High level operations with WOZ images
//! are supported to the extent that the track format and file system are supported.

pub mod dos33;
pub mod prodos;
pub mod applesoft;
pub mod integer;
pub mod walker;
pub mod disk_base;
pub mod img_do;
pub mod img_po;
pub mod img_woz;
pub mod img_woz1;
pub mod img_woz2;
pub mod disk525;

use crate::disk_base::{DiskImage,A2Disk};
use std::io::Read;
use std::fmt::Write;
use log::{info};

/// Use the sectors on an `A2Disk` to update the sectors on a `DiskImage` and save the image file
/// This will almost always be used if we are making permanent changes to a file system.
pub fn update_img_and_save(img: &mut Box<dyn DiskImage>,disk: &Box<dyn A2Disk>,img_path: &str) -> Result<(),Box<dyn std::error::Error>> {
    let temp_po = match disk.get_ordering() {
        disk_base::DiskImageType::DO => {
            let temp_do = img_do::DO::from_bytes(&disk.to_img()).expect("unexpected file system metrics");
            temp_do.to_po().expect("unexpected file system metrics")
        },
        disk_base::DiskImageType::PO => disk.to_img(),
        _ => panic!("unexpected ordering in file system layer")
    };
    match img.update_from_po(&temp_po) {
        Ok(()) => {
            std::fs::write(img_path,img.to_bytes()).expect("could not write disk image to disk");
            Ok(())
        },
        Err(e) => Err(e)
    }
}

/// Return the file system on a disk image, or None if one cannot be found.
fn try_img(img: &impl DiskImage) -> Option<Box<dyn A2Disk>> {
    if let Ok(bytestream) = img.to_do() {
        if let Some(disk) = dos33::Disk::from_img(&bytestream) {
            info!("identified DOS 3.3 file system");
            return Some(Box::new(disk));
        }
    }
    if let Ok(bytestream) = img.to_po() {
        if let Some(disk) = prodos::Disk::from_img(&bytestream) {
            info!("identified ProDOS file system");
            return Some(Box::new(disk));
        }
    }
    return None;
}

/// Given a bytestream return a tuple with (DiskImage, A2Disk), or None if the bytestream cannot be interpreted.
/// DiskImage is the disk structure and data, A2Disk is a higher level representation including a file system (e.g. DOS or ProDOS).
/// Manipulation of files that may be on the image is done via the A2Disk object.
/// The changes are only permanent if they are written back to the DiskImage, and explicitly saved to local storage.
pub fn create_img_and_disk_from_bytestream(disk_img_data: &Vec<u8>) -> Option<(Box<dyn DiskImage>,Box<dyn A2Disk>)> {
    if let Some(img) = img_woz1::Woz1::from_bytes(disk_img_data) {
        info!("identified woz1 image");
        if let Some(disk) = try_img(&img) {
            return Some((Box::new(img),disk));
        }
    }
    if let Some(img) = img_woz2::Woz2::from_bytes(disk_img_data) {
        info!("identified woz2 image");
        if let Some(disk) = try_img(&img) {
            return Some((Box::new(img),disk));
        }
    }
    if let Some(img) = img_do::DO::from_bytes(disk_img_data) {
        info!("Possible DO image");
        if let Some(disk) = try_img(&img) {
            return Some((Box::new(img),disk));
        }
    }
    if let Some(img) = img_po::PO::from_bytes(disk_img_data) {
        info!("Possible PO image");
        if let Some(disk) = try_img(&img) {
            return Some((Box::new(img),disk));
        }
    }
    return None;
}

/// Given a bytestream return a disk image without any file system.
/// N.b. the ordering cannot always be determined without the file system.
pub fn create_img_from_bytestream(disk_img_data: &Vec<u8>) -> Option<Box<dyn DiskImage>> {
    if let Some(img) = img_woz1::Woz1::from_bytes(disk_img_data) {
        info!("identified woz1 image");
        return Some(Box::new(img));
    }
    if let Some(img) = img_woz2::Woz2::from_bytes(disk_img_data) {
        info!("identified woz2 image");
        return Some(Box::new(img));
    }
    if let Some(img) = img_do::DO::from_bytes(disk_img_data) {
        info!("Possible DO image");
        return Some(Box::new(img));
    }
    if let Some(img) = img_po::PO::from_bytes(disk_img_data) {
        info!("Possible PO image");
        return Some(Box::new(img));
    }
    return None;
}

/// Calls `create_img_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
pub fn create_img_from_file(img_path: &str) -> Option<Box<dyn DiskImage>> {
    let disk_img_data = std::fs::read(img_path).expect("failed to read file");
    return create_img_from_bytestream(&disk_img_data);
}

/// Calls `create_img_and_disk_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
pub fn create_img_and_disk_from_file(img_path: &str) -> Option<(Box<dyn DiskImage>,Box<dyn A2Disk>)> {
    let disk_img_data = std::fs::read(img_path).expect("failed to read file");
    return create_img_and_disk_from_bytestream(&disk_img_data);
}

/// Given a bytestream try to identify the type of disk image and create a disk object.
/// N.b. this discards metadata and track layout details, only the high level data remains.
/// This will also panic if the data cannot be interpreted.
#[deprecated(since="0.2.0", note="will be modified to return an Option")]
pub fn create_disk_from_bytestream(disk_img_data: &Vec<u8>) -> Box<dyn A2Disk> {
    if let Some((_img,disk)) = create_img_and_disk_from_bytestream(disk_img_data) {
        return disk;
    }
    panic!("could not interpret disk image data");
}

/// Calls `create_disk_from_bytestream` getting the bytes from stdin
#[deprecated(since="0.2.0", note="will be modified to return an Option")]
pub fn create_disk_from_stdin() -> Box<dyn A2Disk> {
    let mut disk_img_data = Vec::new();
    std::io::stdin().read_to_end(&mut disk_img_data).expect("failed to read input stream");
    return create_disk_from_bytestream(&disk_img_data);
}

/// Calls `create_disk_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
#[deprecated(since="0.2.0", note="will be modified to return an Option")]
pub fn create_disk_from_file(img_path: &str) -> Box<dyn A2Disk> {
    let disk_img_data = std::fs::read(img_path).expect("failed to read file");
    return create_disk_from_bytestream(&disk_img_data);
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
            if !disk525::DISK_BYTES_62.contains(&trk[i]) && trk[i]!=0xaa && trk[i]!=0xd5 {
                mnemonics += "?";
                err_count += 1;
            } else if addr_count>0 {
                if addr_count%2==1 {
                    write!(&mut mnemonics,"{:X}",disk525::decode_44([trk[i],fwd]) >> 4).unwrap();
                } else {
                    write!(&mut mnemonics,"{:X}",disk525::decode_44([bak,trk[i]]) & 0x0f).unwrap();
                }
                addr_count += 1;
            } else {
                mnemonics += match (bak,trk[i],fwd) {
                    (0xff,0xff,_) => ">",
                    (_,0xff,0xff) => ">",
                    (_,0xd5,0xaa) => "(",
                    (0xd5,0xaa,0x96) => "A",
                    (0xaa,0x96,_) => {addr_count=1;":"},
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