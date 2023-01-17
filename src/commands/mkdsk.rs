use clap;
use std::str::FromStr;
use std::error::Error;
use std::num::ParseIntError;
use log::{error,info};
use crate::bios::dpb;
use crate::fs::{DiskFS,cpm,dos3x,prodos,pascal};
use crate::img;
use crate::img::{DiskKind,DiskImage,DiskImageType,names};
use super::CommandError;

const RCH: &str = "unreachable was reached";
const BOOT_MESS: &str = "omit boot flag; for this OS you will need to copy boot files after formatting";

/// Create an image of a specific kind of disk.  If the pairing is not explicitly allowed
/// return an error.  N.b. there is no file system selection whatever at this point.
fn mkimage(img_typ: &DiskImageType,kind: &DiskKind,vol: u8) -> Result<Box<dyn DiskImage>,Box<dyn Error>> {

    return match (img_typ,*kind) {
        (DiskImageType::D13,names::A2_DOS32_KIND) => Ok(Box::new(img::dsk_d13::D13::create(35))),
        (DiskImageType::DO,names::A2_DOS33_KIND) => Ok(Box::new(img::dsk_do::DO::create(35,16))),
        (DiskImageType::WOZ1,names::A2_DOS32_KIND) => Ok(Box::new(img::woz1::Woz1::create(vol,*kind))),
        (DiskImageType::WOZ1,names::A2_DOS33_KIND) => Ok(Box::new(img::woz1::Woz1::create(vol,*kind))),
        (DiskImageType::WOZ2,names::A2_DOS32_KIND) => Ok(Box::new(img::woz2::Woz2::create(vol,*kind))),
        (DiskImageType::WOZ2,names::A2_DOS33_KIND) => Ok(Box::new(img::woz2::Woz2::create(vol,*kind))),
        (DiskImageType::WOZ2,names::A2_400_KIND) => Ok(Box::new(img::woz2::Woz2::create(vol,*kind))),
        (DiskImageType::WOZ2,names::A2_800_KIND) => Ok(Box::new(img::woz2::Woz2::create(vol,*kind))),
        (DiskImageType::PO,names::A2_DOS33_KIND) => Ok(Box::new(img::dsk_po::PO::create(280))),
        (DiskImageType::PO,names::A2_400_KIND) => Ok(Box::new(img::dsk_po::PO::create(800))),
        (DiskImageType::PO,names::A2_800_KIND) => Ok(Box::new(img::dsk_po::PO::create(1600))),
        (DiskImageType::PO,names::A2_HD_MAX) => Ok(Box::new(img::dsk_po::PO::create(65535))),
        (DiskImageType::IMD,names::IBM_CPM1_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::OSBORNE_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        _ => {
            error!("pairing of image type and disk kind is not supported");
            Err(Box::new(CommandError::UnsupportedItemType))
        }
    };
}

fn mkdos3x(vol: Result<u8,ParseIntError>,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if img.byte_capacity()!=35*13*256 && img.byte_capacity()!=35*16*256 {
        error!("disk image capacity {} not consistent with DOS 3.x",img.byte_capacity());
        return Err(Box::new(CommandError::OutOfRange));
    }
    let kind = img.kind(); // need to copy since img will be moved
    match (kind,img.what_am_i()) {
        (_,DiskImageType::PO) => {
            error!("attempt to create ProDOS ordered DOS disk");
            return Err(Box::new(CommandError::UnsupportedFormat));
        },
        (DiskKind::LogicalSectors(img::names::A2_DOS32_SECS),DiskImageType::DO) |
        (DiskKind::D525(img::names::A2_DOS32_SECS,_,_),DiskImageType::DO) => {
            error!("DOS 3.2 cannot use DO image type, use D13");
            return Err(Box::new(CommandError::UnsupportedFormat))
        },
        _ => {}
    }
    match vol {
        Ok(v) if v>=1 || v<=254 => {
            if boot && v!=254 {
                error!("we can only add the boot tracks if volume number is 254");
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
            let mut disk = Box::new(dos3x::Disk::from_img(img));
            match kind {
                DiskKind::LogicalSectors(img::names::A2_DOS32_SECS) => disk.init32(v,boot),
                DiskKind::D525(img::names::A2_DOS32_SECS,_,_) => disk.init32(v,boot),
                DiskKind::LogicalSectors(img::names::A2_DOS33_SECS) => disk.init33(v,boot),
                DiskKind::D525(img::names::A2_DOS33_SECS,_,_) => disk.init33(v,boot),
                _ => {
                    error!("disk incompatible with DOS 3.x");
                    return Err(Box::new(CommandError::UnsupportedFormat));
                }
            }
            return Ok(disk.get_img().to_bytes());
        },
        _ => {
            error!("volume must be from 1 to 254");
            return Err(Box::new(CommandError::OutOfRange));
        }
    }
}

fn mkprodos(vol: &str,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        error!("{}",BOOT_MESS);
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let floppy = match img.kind() {
        DiskKind::D35(_, _, _) => true,
        DiskKind::D525(_, _, _) => true,
        DiskKind::D8(_, _, _) => true,
        _ => false
    };
    let mut disk = Box::new(prodos::Disk::from_img(img));
    disk.format(vol,floppy,None);
    return Ok(disk.get_img().to_bytes());
}

fn mkpascal(vol: &str,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        error!("omit boot flag; OS file images must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(pascal::Disk::from_img(img));
    match disk.format(vol,0xee,None) {
        Ok(()) => Ok(disk.get_img().to_bytes()),
        Err(e) => return Err(Box::new(e))
    }
}

fn mkcpm(vol: &str,boot: bool,kind: &DiskKind,img: Box<dyn DiskImage>) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        error!("Please omit the boot flag, OS tracks must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(cpm::Disk::from_img(img,dpb::DiskParameterBlock::create(&kind),[2,2,3]));
    match disk.format(vol,None) {
        Ok(()) => Ok(disk.get_img().to_bytes()),
        Err(e) => return Err(Box::new(e))
    }
}

pub fn mkdsk(cmd: &clap::ArgMatches) -> Result<(),Box<dyn Error>> {
    let dest_path= cmd.value_of("dimg").expect(RCH);
    let which_fs = cmd.value_of("os").expect(RCH);
    if !["cpm2","dos32","dos33","prodos","pascal"].contains(&which_fs) {
        return Err(Box::new(CommandError::UnknownItemType));
    }
    let str_vol = cmd.value_of("volume").expect(RCH);
    let dos_vol = u8::from_str(str_vol);
    let mut kind = DiskKind::from_str(cmd.value_of("kind").expect(RCH)).unwrap();
    let img_typ = DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap();
    // Refine disk kind based on combined inputs
    if kind==names::A2_DOS33_KIND && which_fs=="dos32" {
        kind = names::A2_DOS32_KIND;
    }
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
            "cpm2" => mkcpm(str_vol,boot,&kind,img),
            "dos32" => mkdos3x(dos_vol,boot,img),
            "dos33" => mkdos3x(dos_vol,boot,img),
            "prodos" => mkprodos(str_vol,boot,img),
            "pascal" => mkpascal(str_vol,boot,img),
            _ => panic!("unreachable")
        };
        if let Ok(buf) = result {
            eprintln!("writing {} bytes",buf.len());
            std::fs::write(&dest_path,&buf).expect("could not write data to disk");
            return Ok(());
        }
        info!("result {:?}",result);
    }
    return Err(Box::new(CommandError::UnsupportedItemType));
}