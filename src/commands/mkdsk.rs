use clap;
use std::io::Write;
use std::str::FromStr;
use std::error::Error;
use std::num::ParseIntError;
use log::info;
use crate::fs::{DiskFS,dos3x,prodos,pascal};
use crate::img;
use crate::img::{DiskKind,DiskImage,DiskImageType};
use super::CommandError;

const RCH: &str = "unreachable was reached";

fn mkimage(img_typ: &DiskImageType,kind: &DiskKind,vol: u8) -> Result<Box<dyn DiskImage>,Box<dyn Error>> {

    return match (img_typ,kind) {
        (DiskImageType::D13,DiskKind::A2_525_13) => Ok(Box::new(img::dsk_d13::D13::create(35))),
        (DiskImageType::WOZ1,DiskKind::A2_525_13) => Ok(Box::new(img::woz1::Woz1::create(vol,DiskKind::A2_525_13))),
        (DiskImageType::WOZ2,DiskKind::A2_525_13) => Ok(Box::new(img::woz2::Woz2::create(vol,DiskKind::A2_525_13))),
        (DiskImageType::DO,DiskKind::A2_525_16) => Ok(Box::new(img::dsk_do::DO::create(35,16))),
        (DiskImageType::PO,DiskKind::A2_525_16) => Ok(Box::new(img::dsk_po::PO::create(280))),
        (DiskImageType::WOZ1,DiskKind::A2_525_16) => Ok(Box::new(img::woz1::Woz1::create(vol,DiskKind::A2_525_16))),
        (DiskImageType::WOZ2,DiskKind::A2_525_16) => Ok(Box::new(img::woz2::Woz2::create(vol,DiskKind::A2_525_16))),
        (DiskImageType::PO,DiskKind::A2_35) => Ok(Box::new(img::dsk_po::PO::create(1600))),
        (DiskImageType::PO,DiskKind::A2Max) => Ok(Box::new(img::dsk_po::PO::create(65535))),
        _ => Err(Box::new(CommandError::UnsupportedItemType))
    };
}

fn mkdos3x(vol: Result<u8,ParseIntError>,boot: bool,blocks: u16,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if img.byte_capacity()!=35*13*256 && img.byte_capacity()!=35*16*256 {
        eprintln!("DOS 3.x only supports 5.25 inch disks");
        return Err(Box::new(CommandError::OutOfRange));
    }
    if img.what_am_i()==DiskImageType::PO {
        eprintln!("ProDOS ordered DOS is refused.  Use `reimage` if you really need to do this.");
        return Err(Box::new(CommandError::UnsupportedFormat));
    }
    if img.what_am_i()==DiskImageType::DO && blocks==228 {
        eprintln!("DOS 3.2 cannot use this image type");
        return Err(Box::new(CommandError::UnsupportedFormat));
    }
    match vol {
        Ok(v) if v>=1 || v<=254 => {
            if boot && v!=254 {
                eprintln!("we can only add the boot tracks if volume number is 254");
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
            let mut disk = Box::new(dos3x::Disk::from_img(img));
            if blocks==228 {
                disk.init32(v,boot);
            } else {
                disk.init33(v,boot);
            }
            return Ok(disk.get_img().to_bytes());
        },
        _ => {
            eprintln!("volume must be from 1 to 254");
            return Err(Box::new(CommandError::OutOfRange));
        }
    }
}

fn mkprodos(vol: &str,boot: bool,floppy: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        eprintln!("Please omit the boot flag, OS file images must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(prodos::Disk::from_img(img));
    disk.format(vol,floppy,None);
    return Ok(disk.get_img().to_bytes());
}

fn mkpascal(vol: &str,boot: bool,blocks: u16,kind: &DiskKind,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        eprintln!("Please omit the boot flag, OS file images must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(pascal::Disk::from_img(img));
    match disk.format(vol,0xee,kind,None) {
        Ok(()) => Ok(disk.get_img().to_bytes()),
        Err(e) => return Err(Box::new(e))
    }
}

pub fn mkdsk(cmd: &clap::ArgMatches) -> Result<(),Box<dyn Error>> {
    let which_fs = cmd.value_of("os").expect(RCH);
    if !["dos32","dos33","prodos","pascal"].contains(&which_fs) {
        return Err(Box::new(CommandError::UnknownItemType));
    }
    let str_vol = cmd.value_of("volume").expect(RCH);
    let dos_vol = u8::from_str(str_vol);
    let mut kind = DiskKind::from_str(cmd.value_of("kind").expect(RCH)).unwrap();
    let img_typ = DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap();
    // nibble types and soft sector arrangement must be inferred from FS selection
    if kind==DiskKind::A2_525_16 && which_fs=="dos32" {
        kind = DiskKind::A2_525_13;
    }
    let (blocks,floppy) = match kind {
        DiskKind::A2_525_13 => (228,true),
        DiskKind::A2_525_16 => (280,true),
        DiskKind::A2_35 => (1600,true),
        DiskKind::A2Max => (65535,false)
    };
    let boot = cmd.get_flag("bootable");
    if boot {
        info!("bootable requested");
    }
    let maybe_img = match dos_vol {
        Ok(v) => mkimage(&img_typ,&kind,v),
        _ => mkimage(&img_typ,&kind,254)
    };
    if let Ok(img) = maybe_img {
        let result = match which_fs {
            "dos32" => mkdos3x(dos_vol,boot,228,img),
            "dos33" => mkdos3x(dos_vol,boot,blocks,img),
            "prodos" => mkprodos(str_vol,boot,floppy,img),
            "pascal" => mkpascal(str_vol,boot,blocks,&kind,img),
            _ => panic!("unreachable")
        };
        if let Ok(buf) = result {
            eprintln!("writing {} bytes",buf.len());
            std::io::stdout().write_all(&buf).expect("write to stdout failed");
            return Ok(());
        }
        info!("result {:?}",result);
    }
    return Err(Box::new(CommandError::UnsupportedItemType));
}