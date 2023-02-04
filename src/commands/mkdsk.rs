use clap;
use std::str::FromStr;
use log::{error,info};
use crate::bios::dpb;
use crate::fs::{DiskFS,cpm,dos3x,prodos,pascal};
use crate::img;
use crate::img::{DiskKind,DiskImage,DiskImageType,names};
use super::CommandError;
use crate::{STDRESULT,DYNERR};

const RCH: &str = "unreachable was reached";
const BOOT_MESS: &str = "omit boot flag; for this OS you will need to copy boot files after formatting";
const BOOT_MESS_CPM: &str = "omit boot flag; for this OS you will need to copy reserved tracks after formatting";

/// Create an image of a specific kind of disk.  If the pairing is not explicitly allowed
/// return an error.  N.b. there is no file system selection whatever at this point.
fn mkimage(img_typ: &DiskImageType,kind: &DiskKind,maybe_vol: Option<&str>) -> Result<Box<dyn DiskImage>,DYNERR> {
    let vol = match maybe_vol {
        Some(vstr) => match u8::from_str_radix(vstr,10) {
            Ok(v) => v,
            _ => 254
        },
        _ => 254
    };
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
        (DiskImageType::IMD,names::OSBORNE1_SD_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::OSBORNE1_DD_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::KAYPROII_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::KAYPRO4_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::TRS80_M2_CPM_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        (DiskImageType::IMD,names::NABU_CPM_KIND) => Ok(Box::new(img::imd::Imd::create(*kind))),
        _ => {
            error!("pairing of image type and disk kind is not supported");
            Err(Box::new(CommandError::UnsupportedItemType))
        }
    };
}

fn mkdos3x(vol: Option<&str>,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,DYNERR> {
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
        (DiskKind::LogicalSectors(img::names::A2_DOS32),DiskImageType::DO) |
        (DiskKind::D525(img::names::A2_DOS32),DiskImageType::DO) => {
            error!("DOS 3.2 cannot use DO image type, use D13");
            return Err(Box::new(CommandError::UnsupportedFormat))
        },
        _ => {}
    }
    if vol==None {
        error!("DOS 3.x requires volume number");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    match u8::from_str_radix(vol.unwrap(), 10) {
        Ok(v) if v>=1 || v<=254 => {
            if boot && v!=254 {
                error!("we can only add the boot tracks if volume number is 254");
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
            let mut disk = Box::new(dos3x::Disk::from_img(img));
            match kind {
                DiskKind::LogicalSectors(img::names::A2_DOS32) => disk.init32(v,boot)?,
                DiskKind::D525(img::names::A2_DOS32) => disk.init32(v,boot)?,
                DiskKind::LogicalSectors(img::names::A2_DOS33) => disk.init33(v,boot)?,
                DiskKind::D525(img::names::A2_DOS33) => disk.init33(v,boot)?,
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

fn mkprodos(vol: Option<&str>,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,DYNERR> {
    if boot {
        error!("{}",BOOT_MESS);
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let floppy = match img.kind() {
        DiskKind::D35(_) => true,
        DiskKind::D525(_) => true,
        DiskKind::D8(_) => true,
        _ => false
    };
    if let Some(vol_name) = vol {
        let mut disk = Box::new(prodos::Disk::from_img(img));
        disk.format(vol_name,floppy,None)?;
        return Ok(disk.get_img().to_bytes());
    } else {
        error!("prodos fs requires volume name");
        return Err(Box::new(CommandError::InvalidCommand));
    }
}

fn mkpascal(vol: Option<&str>,boot: bool,img: Box<dyn DiskImage>) -> Result<Vec<u8>,DYNERR> {
    if boot {
        error!("{}",BOOT_MESS);
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    if let Some(vol_name) = vol {
        let mut disk = Box::new(pascal::Disk::from_img(img));
        match disk.format(vol_name,0xee,None) {
            Ok(()) => Ok(disk.get_img().to_bytes()),
            Err(e) => return Err(e)
        }
    } else {
        error!("pascal fs requires volume name");
        return Err(Box::new(CommandError::InvalidCommand));
    }
}

fn mkcpm(vol: Option<&str>,boot: bool,kind: &DiskKind,img: Box<dyn DiskImage>) -> Result<Vec<u8>,DYNERR> {
    if boot {
        error!("{}",BOOT_MESS_CPM);
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(cpm::Disk::from_img(img,dpb::DiskParameterBlock::create(&kind),[2,2,3]));
    let vol_name = match vol {
        Some(nm) => nm,
        None => "A"
    };
    match disk.format(vol_name,None) {
        Ok(()) => Ok(disk.get_img().to_bytes()),
        Err(e) => return Err(e)
    }
}

pub fn mkdsk(cmd: &clap::ArgMatches) -> STDRESULT {
    let dest_path= cmd.value_of("dimg").expect(RCH);
    let which_fs = cmd.value_of("os").expect(RCH);
    if !["cpm2","dos32","dos33","prodos","pascal"].contains(&which_fs) {
        return Err(Box::new(CommandError::UnknownItemType));
    }
    // First make sure destination is OK
    let dest_path_abstract = std::path::Path::new(dest_path);
    if let Some(parent) = std::path::Path::parent(dest_path_abstract) {
        if parent.to_string_lossy().len()>0 {
            match std::path::Path::try_exists(parent) {
                Ok(true) => {},
                Ok(false) => {
                    error!("destination directory does not exist ({})",parent.to_string_lossy());
                    return Err(Box::new(CommandError::InvalidCommand));
                },
                Err(e) => {
                    error!("problem with this destination path");
                    return Err(Box::new(e))
                }
            }
        }
    }
    match std::path::Path::try_exists(dest_path_abstract) {
        Ok(true) => {
            error!("cannot overwrite existing disk image");
            return Err(Box::new(CommandError::InvalidCommand));
        },
        Ok(false) => info!("destination path OK, preparing to write"),
        Err(e) => {
            error!("problem with this destination path");
            return Err(Box::new(e))
        }
    }
    // Destination is OK, proceed
    let maybe_vol = cmd.value_of("volume");
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
    let maybe_img = mkimage(&img_typ,&kind,maybe_vol);
    if let Ok(img) = maybe_img {
        if let Some(fext) = dest_path.split(".").last() {
            if !img.file_extensions().contains(&fext.to_string().to_lowercase()) {
                error!("Extension was {}, should be {:?}",fext,img.file_extensions());
                return Err(Box::new(CommandError::InvalidCommand));
            }
        } else {
            error!("Extension missing, should be {:?}",img.file_extensions());
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let result = match which_fs {
            "cpm2" => mkcpm(maybe_vol,boot,&kind,img),
            "dos32" => mkdos3x(maybe_vol,boot,img),
            "dos33" => mkdos3x(maybe_vol,boot,img),
            "prodos" => mkprodos(maybe_vol,boot,img),
            "pascal" => mkpascal(maybe_vol,boot,img),
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