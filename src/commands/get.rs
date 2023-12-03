use clap;
use std::io::Write;
use std::str::FromStr;
use log::{warn,error};

use super::{ItemType,CommandError};
use crate::fs::DiskFS;
use crate::{STDRESULT,DYNERR};

// TODO: somehow fold FileImage and Records into the pattern
fn output_get(
    maybe_object: Result<(u16,Vec<u8>),DYNERR>,
    maybe_typ: Result<ItemType,CommandError>,
    maybe_disk: Option<Box<dyn DiskFS>>
) -> STDRESULT {
    match maybe_object {
        Ok((start_addr,object)) => {
            match (maybe_typ,maybe_disk,atty::is(atty::Stream::Stdout)) {
                (Ok(ItemType::Text),Some(disk),_) => {
                    let str = disk.decode_text(&object)?;
                    print!("{}",str);
                    std::io::stdout().flush()?;
                    if !str.ends_with("\n") {
                        eprintln!();
                        warn!("string ended without a newline");
                    }
                },
                (_,_,true) => crate::display_block(start_addr,&object),
                (_,_,false) => std::io::stdout().write_all(&object).expect("could not write stdout")
            }
            return Ok(())
        },
        Err(e) => return Err(e)
    }
}

pub fn get(cmd: &clap::ArgMatches) -> STDRESULT {

    let maybe_src_path = cmd.get_one::<String>("file");
    let maybe_typ = cmd.get_one::<String>("type");
    let maybe_img = cmd.get_one::<String>("dimg");
    let local_file = atty::is(atty::Stream::Stdin) && maybe_img.is_none();
    let trunc = cmd.get_flag("trunc");

    match (maybe_typ,local_file,maybe_src_path) {

        // we are getting a specific item from a disk image
        (Some(typ_str),false,Some(src_path)) => {
            // For items that don't need a file system handle differently.
            // Also verify truncation flag.
            match (ItemType::from_str(typ_str),trunc) {
                (Ok(ItemType::Track),false) => return super::get_img::get(cmd),
                (Ok(ItemType::RawTrack),false) => return super::get_img::get(cmd),
                (Ok(ItemType::Sector),false) => return super::get_img::get(cmd),
                (Ok(ItemType::Metadata),false) => return super::get_img::get_meta(cmd),
                (Ok(ItemType::Raw),_) => {},
                (Ok(_),false) => {},
                (_,true) => {
                    eprintln!("`trunc` flag only used with raw type");
                    return Err(Box::new(CommandError::InvalidCommand));
                },
                (Err(e),_) => return Err(Box::new(e))
            }
            let typ = ItemType::from_str(typ_str);
            match crate::create_fs_from_file_or_stdin(maybe_img) {
                Ok(mut disk) => {
                    // special handling for sparse data
                    if let Ok(ItemType::FileImage) = typ {
                        return match disk.read_any(&src_path) {
                            Ok(fimg) => {
                                println!("{}",fimg.to_json(4));
                                Ok(())
                            },
                            Err(e) => Err(e)
                        }
                    }
                    // special handling for random access text
                    if let Ok(ItemType::Records) = typ {
                        let record_length = match cmd.get_one::<String>("len") {
                            Some(s) => {
                                if let Ok(l) = usize::from_str(s) {
                                    l
                                } else {
                                    0 as usize
                                }
                            },
                            _ => 0 as usize
                        };
                        return match disk.read_records(&src_path,record_length) {
                            Ok(recs) => {
                                println!("{}",recs.to_json(4));
                                Ok(())
                            },
                            Err(e) => Err(e)
                        }
                    }
                    // other file types
                    let maybe_object = match typ {
                        Ok(ItemType::ApplesoftTokens) => disk.load(&src_path),
                        Ok(ItemType::IntegerTokens) => disk.load(&src_path),
                        Ok(ItemType::MerlinTokens) => disk.read_text(&src_path),
                        Ok(ItemType::Binary) => disk.bload(&src_path),
                        Ok(ItemType::Text) => disk.read_text(&src_path),
                        Ok(ItemType::Raw) => disk.read_raw(&src_path,trunc),
                        Ok(ItemType::Block) => {
                            let mut cum: Vec<u8> = Vec::new();
                            let blocks = super::parse_block_request(&src_path)?;
                            for b in blocks {
                                cum.append(&mut disk.read_block(&b.to_string())?.1);
                            }
                            Ok((0,cum))
                        },
                        _ => Err::<(u16,Vec<u8>),DYNERR>(Box::new(CommandError::UnsupportedItemType))
                    };
                    return output_get(maybe_object,typ,Some(disk));
                },
                Err(e) => return Err(e)
            }
        },

        // this pattern can be used for metadata only
        (Some(type_str),false,None) => {
            match ItemType::from_str(type_str) {
                Ok(ItemType::Metadata) => return super::get_img::get_meta(cmd),
                Ok(_) => {
                    error!("please narrow the item with `-f`");
                    Err(Box::new(CommandError::InvalidCommand))
                },
                Err(e) => Err(Box::new(e))
            }
        },

        // this pattern means we have a local file
        (None,true,Some(src_path)) => {
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
                (Some(_),None) => error!("please pipe a disk image or use `-d`"),
                (Some(_),Some(_)) => error!("please narrow the item with `-f`"),
                (None,Some(_)) => error!("please narrow the type of item with `-t`"),
                (None,None) => error!("please provide arguments")
            }
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}