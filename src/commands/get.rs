use clap;
use std::io::Write;
use std::io::Read;
use std::str::FromStr;

use super::{ItemType,CommandError};
use crate::fs::{FileImage,UnpackedData};
use crate::{DYNERR,STDRESULT};

fn output_get(result: UnpackedData, load_addr: usize) -> STDRESULT {
    match (result,atty::is(atty::Stream::Stdout)) {
        (UnpackedData::Text(txt),_) => {
            print!("{}",txt);
            std::io::stdout().flush()?;
            if !txt.ends_with("\n") {
                eprintln!();
                log::warn!("string ended without a newline");
            }
        },
        (UnpackedData::Binary(dat),true) => crate::display_block(load_addr,&dat),
        (UnpackedData::Binary(dat),false) => std::io::stdout().write_all(&dat).expect("could not write stdout"),
        (UnpackedData::Records(recs),_) => println!("{}",recs.to_json(Some(2)))
    }
    Ok(())
}

fn unpack_primitive(fimg: &FileImage,typ: ItemType,rec_len: Option<usize>,trunc: bool) -> Result<UnpackedData,DYNERR> {
    match typ {
        ItemType::Automatic => fimg.unpack(),
        ItemType::FileImage => Ok(UnpackedData::Text(fimg.to_json(Some(2)))),
        ItemType::Records => Ok(UnpackedData::Text(fimg.unpack_rec_str(rec_len,Some(2))?)),
        ItemType::ApplesoftTokens => Ok(UnpackedData::Binary(fimg.unpack_tok()?)),
        ItemType::IntegerTokens => Ok(UnpackedData::Binary(fimg.unpack_tok()?)),
        ItemType::MerlinTokens => Ok(UnpackedData::Binary(fimg.unpack_raw(true)?)),
        ItemType::Binary => Ok(UnpackedData::Binary(fimg.unpack_bin()?)),
        ItemType::Text => Ok(UnpackedData::Text(fimg.unpack_txt()?)),
        ItemType::Raw => Ok(UnpackedData::Binary(fimg.unpack_raw(trunc)?)),
        _ => Err(Box::new(CommandError::UnsupportedItemType))
    }
}

pub fn unpack(cmd: &clap::ArgMatches) -> STDRESULT {
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
    let trunc = cmd.get_flag("trunc");
    let typ = ItemType::from_str(cmd.get_one::<String>("type").unwrap())?;
    let rec_len = match cmd.get_one::<String>("len") {
        Some(s) => Some(usize::from_str(s)?),
        None => None
    };
    let json_str = String::from_utf8(dat)?;
    let fimg = FileImage::from_json(&json_str)?;
    let result = unpack_primitive(&fimg, typ, rec_len, trunc)?;
    output_get(result, fimg.get_load_address() as usize)
}

pub fn get(cmd: &clap::ArgMatches) -> STDRESULT {

    let maybe_src_path = cmd.get_one::<String>("file");
    let maybe_typ = cmd.get_one::<String>("type");
    let maybe_img = cmd.get_one::<String>("dimg");
    let pipe_or_img = !atty::is(atty::Stream::Stdin) || maybe_img.is_some();
    let trunc = cmd.get_flag("trunc");
    let rec_len = match cmd.get_one::<String>("len") {
        Some(s) => Some(usize::from_str(s)?),
        None => None
    };

    match (maybe_typ, pipe_or_img, maybe_src_path) {

        // we are getting a specific item from a disk image
        (Some(typ_str),true,Some(src_path)) => {
            // For items that don't need a file system handle differently.
            // Also verify truncation flag.
            let typ = ItemType::from_str(typ_str)?;
            match (typ,trunc) {
                (ItemType::Track,false) => return super::get_img::get(cmd),
                (ItemType::RawTrack,false) => return super::get_img::get(cmd),
                (ItemType::Sector,false) => return super::get_img::get(cmd),
                (ItemType::Metadata,false) => return super::get_img::get_meta(cmd),
                (ItemType::Raw,_) => {},
                (_,false) => {},
                (_,true) => {
                    eprintln!("`trunc` flag only used with raw type");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
            }
            let mut disk = crate::create_fs_from_file_or_stdin(maybe_img)?;
            if typ == ItemType::Block {
                let mut cum: Vec<u8> = Vec::new();
                let blocks = super::parse_block_request(&src_path)?;
                for b in blocks {
                    cum.append(&mut disk.read_block(&b.to_string())?);
                }
                return output_get(UnpackedData::Binary(cum),0);
            }
            let fimg = disk.get(&src_path)?;
            let result = unpack_primitive(&fimg, typ, rec_len, trunc)?;
            return output_get(result,fimg.get_load_address() as usize);
        },

        // this pattern can be used for metadata only
        (Some(type_str),true,None) => {
            match ItemType::from_str(type_str) {
                Ok(ItemType::Metadata) => return super::get_img::get_meta(cmd),
                Ok(_) => {
                    log::error!("please narrow the item with `-f`");
                    Err(Box::new(CommandError::InvalidCommand))
                },
                Err(e) => Err(Box::new(e))
            }
        },

        // this pattern means we have a local file
        (None,false,Some(src_path)) => {
            match std::fs::read(&src_path) {
                Ok(object) => {
                    std::io::stdout().write_all(&object).expect("could not write stdout");
                    return Ok(());
                },
                Err(e) => return Err(Box::new(e))
            }
        },

        // arguments inconsistent
        _ => {
            match (maybe_typ,maybe_img) {
                (Some(_),None) => log::error!("please pipe a disk image or use `-d`"),
                (Some(_),Some(_)) => log::error!("please narrow the item with `-f`"),
                (None,Some(_)) => log::error!("please narrow the type of item with `-t`"),
                (None,None) => log::error!("please provide arguments")
            }
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}

pub fn mget(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        log::error!("line entry is not supported for `mget`, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let path_to_img = cmd.get_one::<String>("dimg").unwrap();
    let json_list = super::get_json_list_from_stdin()?;
    let mut disk = crate::create_fs_from_file(&path_to_img)?;

    let mut ans = json::array![];
    for path in json_list.members() {
        if !path.is_string() {
            log::error!("element of input to mget was not a string");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let fimg = disk.get(path.as_str().unwrap())?;
        ans.push(json::parse(&fimg.to_json(None))?)?;
    }
    println!("{}",ans.to_string());
    return Ok(());
}