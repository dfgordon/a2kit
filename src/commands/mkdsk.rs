use clap;
use std::io::Write;
use std::str::FromStr;
use std::error::Error;
use std::num::ParseIntError;
use log::info;
use a2kit::disk_base::*;
use a2kit::fs::dos33;
use a2kit::fs::prodos;
use a2kit::fs::pascal;
use a2kit::img;

const RCH: &str = "unreachable was reached";

fn mkimage(img_typ: &DiskImageType,fs: Box<dyn DiskFS>,vol: u8) -> Result<Box<dyn DiskImage>,Box<dyn Error>> {
    // If DSK buffer size is not divisible by 256*16 will panic.
    // First put fs in DOS order if necessary.
    let do_bytes = match fs.get_ordering() {
        DiskImageType::PO => img::reorder_po_to_do(&fs.to_img(),16),
        _ => fs.to_img()
    };
    return match img_typ {
        DiskImageType::DO => {
            if let Some(img) = img::dsk_do::DO::from_bytes(&do_bytes) {
                Ok(Box::new(img))
            } else {
                Err(Box::new(CommandError::UnsupportedItemType))
            }
        },
        DiskImageType::PO => {
            if let Some(img) = img::dsk_po::PO::from_bytes(&img::reorder_do_to_po(&do_bytes,16)) {
                Ok(Box::new(img))
            } else {
                Err(Box::new(CommandError::UnsupportedItemType))
            }
        },
        DiskImageType::WOZ1 => {
            let mut img = img::woz1::Woz1::create(vol,DiskKind::A2_525_16);
            match img.update_from_do(&do_bytes) {
                Ok(()) => Ok(Box::new(img))
                ,
                Err(e) => Err(e)
            }
        },
        DiskImageType::WOZ2 => {
            let mut img = img::woz2::Woz2::create(vol,DiskKind::A2_525_16);
            match img.update_from_do(&do_bytes) {
                Ok(()) => Ok(Box::new(img))
                ,
                Err(e) => Err(e)
            }
        }
    };
}

fn mkdos33(vol: Result<u8,ParseIntError>,boot: bool,blocks: u16,img_typ: &DiskImageType) -> Result<Vec<u8>,Box<dyn Error>> {
    if blocks!=280 {
        eprintln!("DOS 3.3 only supports 5.25 inch disks");
        return Err(Box::new(CommandError::OutOfRange));
    }
    if *img_typ==DiskImageType::PO {
        eprintln!("ProDOS ordered DOS is refused.  Use `reimage` if you really need to do this.");
        return Err(Box::new(CommandError::UnsupportedFormat));
    }
    match vol {
        Ok(v) if v>=1 || v<=254 => {
            if boot && v!=254 {
                eprintln!("we can only add the boot tracks if volume number is 254");
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
            let mut disk = Box::new(dos33::Disk::new());
            disk.format(v,boot,17);
            match mkimage(&img_typ,disk,v) {
                Ok(img) => {
                    return Ok(img.to_bytes());
                },
                Err(e) => return Err(e)
            }
        },
        _ => {
            eprintln!("volume must be from 1 to 254");
            return Err(Box::new(CommandError::OutOfRange));
        }
    }
}

fn mkprodos(vol: &str,boot: bool,floppy: bool,blocks: u16,img_typ: &DiskImageType) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        eprintln!("Please omit the boot flag, OS file images must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(prodos::Disk::new(blocks));
    disk.format(vol,floppy,None);
    match mkimage(&img_typ,disk,254) {
        Ok(img) => {
            return Ok(img.to_bytes());
},
        Err(e) => return Err(e)
    }
}

fn mkpascal(vol: &str,boot: bool,blocks: u16,kind: &DiskKind,img_typ: &DiskImageType) -> Result<Vec<u8>,Box<dyn Error>> {
    if boot {
        eprintln!("Please omit the boot flag, OS file images must be obtained elsewhere");
        return Err(Box::new(CommandError::UnsupportedItemType));
    }
    let mut disk = Box::new(pascal::Disk::new(blocks));
    match disk.format(vol,0xee,kind,None) {
        Ok(()) => {},
        Err(e) => return Err(Box::new(e))
    }
    match mkimage(&img_typ,disk,254) {
        Ok(img) => {
            return Ok(img.to_bytes());
},
        Err(e) => return Err(e)
    }
}

pub fn mkdsk(cmd: &clap::ArgMatches) -> Result<(),Box<dyn Error>> {
    let str_vol = cmd.value_of("volume").expect(RCH);
    let dos_vol = u8::from_str(str_vol);
    let kind = DiskKind::from_str(cmd.value_of("kind").expect(RCH)).unwrap();
    let img_typ = DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap();
    let (blocks,floppy) = match kind {
        DiskKind::A2_525_16 => (280,true),
        DiskKind::A2_35 => (1600,true),
        DiskKind::A2Max => (65535,false),
        _ => (280,true)
    };
    let boot = cmd.get_flag("bootable");
    if boot {
        info!("bootable requested");
    }
    let which_fs = cmd.value_of("os").expect(RCH);
    if !["dos33","prodos","pascal"].contains(&which_fs) {
        return Err(Box::new(CommandError::UnknownItemType));
    }
    let result = match which_fs {
        "dos33" => mkdos33(dos_vol,boot,blocks,&img_typ),
        "prodos" => mkprodos(str_vol,boot,floppy,blocks,&img_typ),
        "pascal" => mkpascal(str_vol,boot,blocks,&kind,&img_typ),
        _ => panic!("unreachable")
    };

    if let Ok(buf) = result {
        eprintln!("writing {} bytes",buf.len());
        std::io::stdout().write_all(&buf).expect("write to stdout failed");
        return Ok(());
    }
    return Err(Box::new(CommandError::UnsupportedItemType));
}