use clap;
use std::io::{Cursor, Read};
use std::str::FromStr;
use binrw::BinRead;
use crate::bios::r#as::{AppleSingleFile, EntryData, EntryType};
use super::{ItemType, CommandError};
use crate::fs::FileImage;
use crate::STDRESULT;

const RANGED_ACCESS: &str =
"Writing to multiple blocks is only allowed if the buffers match exactly";

fn pack_primitive(fimg: &mut FileImage, dat: &[u8], load_addr: Option<usize>, typ: ItemType) -> STDRESULT {
    match typ {
        ItemType::Raw => fimg.pack_raw(&dat),
        ItemType::Binary => fimg.pack_bin(&dat,load_addr,None),
        ItemType::AppleSingle => {
            let parsed = AppleSingleFile::read(&mut Cursor::new(dat))?;

            let data = match parsed.get_entry(EntryType::DataFork) {
                Some(EntryData::DataFork(data)) => Ok(data),
                _ => {
                    log::error!("AppleSingle file does not contain any data");
                    Err(Box::new(CommandError::UnknownFormat))
                },
            }?;

            let resource: Option<&[u8]> = match parsed.get_entry(EntryType::ResourceFork) {
                Some(EntryData::DataFork(data)) => Some(data),
                _ => None,
            };

            let prodos_load_addr = match parsed.get_entry(EntryType::ProdosFileInfo) {
                Some(EntryData::ProDOSFileInfo(file_info)) => Some(usize::try_from(file_info.aux_type).unwrap()),
                _ => {
                    log::warn!("AppleSingle file does not contain any ProDOS file info");
                    None
                },
            };

            fimg.pack_bin(&data, load_addr.or(prodos_load_addr), resource)
        },
        ItemType::ApplesoftTokens => fimg.pack_tok(&dat,ItemType::ApplesoftTokens,None),
        ItemType::IntegerTokens => fimg.pack_tok(&dat,ItemType::IntegerTokens,None),
        ItemType::MerlinTokens => fimg.pack_raw(&dat),
        ItemType::Text => {
            let txt = std::str::from_utf8(&dat)?;
            fimg.pack_txt(txt)
        },
        ItemType::Records => {
            let json_str = std::str::from_utf8(&dat)?;
            fimg.pack_rec_str(json_str)
        },
        _ => return Err(Box::new(CommandError::UnsupportedItemType))
    }
}

pub fn pack(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        log::error!("cannot use `put` with console input, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let mut dat = Vec::new();
    std::io::stdin().read_to_end(&mut dat).expect("failed to read input stream");
    if dat.len()==0 {
        log::error!("put did not receive any data from previous node");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let path = cmd.get_one::<String>("file").unwrap();
    let typ = ItemType::from_str(cmd.get_one::<String>("type").unwrap())?;
    let maybe_chunk_len = cmd.get_one::<u16>("block");
    let load_addr: Option<usize> = match cmd.get_one::<String>("addr") {
        Some(a) => Some(usize::from_str(a)?),
        _ => None
    };
    let which_fs = cmd.get_one::<String>("os").unwrap();
    let chunk_len = match (which_fs.as_str(),maybe_chunk_len) {
        ("dos32",_) | ("dos33",_) => 256,
        ("prodos",_) => crate::fs::prodos::types::BLOCK_SIZE,
        ("pascal",_) => crate::fs::pascal::types::BLOCK_SIZE,
        (_,Some(x)) => *x as usize,
        (_,None) => {
            log::error!("this file system requires explicit block size");
            return Err(Box::new(CommandError::InvalidCommand));
        }
    };
    let mut fimg = match which_fs.as_str() {
        "cpm2" | "cpm3" => crate::fs::cpm::new_fimg(chunk_len, true, path)?,
        "dos32" | "dos33" => crate::fs::dos3x::new_fimg(chunk_len, path)?,
        "prodos" => crate::fs::prodos::new_fimg(chunk_len, true, path)?,
        "pascal" => crate::fs::pascal::new_fimg(chunk_len, true, path)?,
        "fat" => crate::fs::fat::new_fimg(chunk_len, true, path)?,
        _ => return Err(Box::new(CommandError::UnknownItemType))
    };
    pack_primitive(&mut fimg, &dat, load_addr, typ)?;
    println!("{}",fimg.to_json(cmd.get_one::<u16>("indent").copied()));
    Ok(())
}

pub fn put(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        log::error!("cannot use `put` with console input, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    // if !atty::is(atty::Stream::Stdout) {
    //     log::error!("output is redirected, but `put` must end the pipeline");
    //     return Err(Box::new(CommandError::InvalidCommand));
    // }
    let maybe_dest_path = cmd.get_one::<String>("file");
    let maybe_typ = cmd.get_one::<String>("type");
    let maybe_img = cmd.get_one::<String>("dimg");
    let mut dat = Vec::new();
    std::io::stdin().read_to_end(&mut dat).expect("failed to read input stream");
    if dat.len()==0 {
        log::error!("put did not receive any data from previous node");
        return Err(Box::new(CommandError::InvalidCommand));
    }

    match (maybe_typ,maybe_img,maybe_dest_path) {
        
        // we are putting a specific item to a disk image
        (Some(typ_str),Some(img_path),Some(dest_path)) => {
            let typ = ItemType::from_str(typ_str)?;
            // For items that don't need a file system, handle differently
            match typ {
                ItemType::Track | ItemType::RawTrack | ItemType::Sector => return super::put_img::put(cmd,&dat),
                ItemType::Metadata => return super::put_img::put_meta(cmd,&dat),
                _ => {}
            }
            let load_addr: Option<usize> = match cmd.get_one::<String>("addr") {
                Some(a) => Some(usize::from_str(a)?),
                _ => None
            };
            let mut disk = crate::create_fs_from_file(img_path)?;

            // Handle block ranges
            if typ == ItemType::Block {
                let mut ptr = 0;
                let blocks = super::parse_block_request(&dest_path)?;
                for b in &blocks {
                    // read the block to get its length
                    let block_len = disk.read_block(&b.to_string())?.len();
                    if ptr + block_len > dat.len() && block_len > 1 {
                        log::error!("{}",RANGED_ACCESS);
                        return Err(Box::new(CommandError::InvalidCommand));
                    }
                    if ptr >= dat.len() {
                        log::error!("{}",RANGED_ACCESS);
                        return Err(Box::new(CommandError::InvalidCommand));
                    }
                    disk.write_block(&b.to_string(),&dat[ptr..ptr+block_len])?;
                    ptr += block_len;
                }
                if blocks.len() > 1 && ptr != dat.len() {
                    log::error!("{}",RANGED_ACCESS);
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                return crate::save_img(&mut disk, img_path);
            }

            // If not a block, handle a file
            let mut fimg = disk.new_fimg(None, true, dest_path)?;
            if typ == ItemType::FileImage {
                let json_str = std::str::from_utf8(&dat)?;
                fimg = FileImage::from_json(json_str)?;
                fimg.set_path(dest_path)?;
            } else {
                pack_primitive(&mut fimg, &dat, load_addr, typ)?;
            }
            disk.put(&fimg)?;
            crate::save_img(&mut disk,img_path)
        },

        // this pattern can be used for metadata only
        (Some(type_str),Some(_),None) => {
            match ItemType::from_str(type_str) {
                Ok(ItemType::Metadata) => return super::put_img::put_meta(cmd,&dat),
                Ok(_) => {
                    log::error!("please narrow the item with `-f`");
                    Err(Box::new(CommandError::InvalidCommand))
                },
                Err(e) => Err(Box::new(e))
            }
        },

        // this pattern means we have a local file
        (None,None,Some(dest_path)) => {
            std::fs::write(&dest_path,&dat).expect("could not write data to disk");
            return Ok(());
        },

        // arguments inconsistent
        _ => {
            match (maybe_typ,maybe_img) {
                (Some(_),None) => log::error!("please specify disk image with `-d`"),
                (Some(_),Some(_)) => log::error!("please narrow the item with `-f`"),
                (None,Some(_)) => log::error!("please narrow the type of item with `-t`"),
                (None,None) => log::error!("please provide arguments")
            }
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}

pub fn mput(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        log::error!("line entry is not supported for `mput`, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let maybe_dest_path = cmd.get_one::<String>("file");
    let path_to_img = cmd.get_one::<String>("dimg").unwrap();
    let json_list = super::get_json_list_from_stdin()?;
    let mut disk = crate::create_fs_from_file(&path_to_img)?;

    for fimg_value in json_list.members() {
        let mut fimg = FileImage::from_json(&fimg_value.to_string())?;
        if let Some(dest_path_primitive) = maybe_dest_path {
            if ["prodos","fat"].contains(&fimg.file_system.as_str()) {
                let fname = fimg.full_path.split("/").last().unwrap();
                let dest_path = match dest_path_primitive.ends_with("/") {
                    true => [dest_path_primitive,fname].concat(),
                    false => [dest_path_primitive,"/",fname].concat()
                };
                log::debug!("{} overridden by {}",&fimg.full_path,dest_path);
                fimg.set_path(&dest_path)?;
            } else if fimg.file_system == "cpm" {
                let fname = fimg.full_path.split(":").last().unwrap();
                let dest_path = match dest_path_primitive.ends_with(":") {
                    true => [dest_path_primitive,fname].concat(),
                    false => return Err(Box::new(CommandError::UnknownFormat))
                };
                log::debug!("{} overridden by {}",&fimg.full_path,dest_path);
                fimg.set_path(&dest_path)?;
            } else {
                log::warn!("ignoring destination path due to flat file system");
            }
        }
        disk.put(&fimg)?;
    }
    return crate::save_img(&mut disk, path_to_img);
}