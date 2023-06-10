use clap;
use std::io::Write;
use std::str::FromStr;
use log::{warn,error};

use super::{ItemType,CommandError};
use crate::fs::DiskFS;
use crate::{STDRESULT,DYNERR};

const RCH: &str = "unreachable was reached";

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
    // if !atty::is(atty::Stream::Stdin) {
    //     error!("input is redirected, but `get` must start the pipeline");
    //     return Err(Box::new(CommandError::InvalidCommand));
    // }
    let src_path = cmd.get_one::<String>("file").expect(RCH);
    let maybe_typ = cmd.get_one::<String>("type");
    let maybe_img = cmd.get_one::<String>("dimg");
    let trunc = cmd.get_flag("trunc");

    match (maybe_typ,maybe_img) {

        // we are getting from a disk image
        (Some(typ_str),Some(img_path)) => {
            // If getting a track or sector, no need to resolve file system, handle differently.
            // Also verify truncation flag.
            match (ItemType::from_str(typ_str),trunc) {
                (Ok(ItemType::Track),false) => return super::get_img::get(cmd),
                (Ok(ItemType::RawTrack),false) => return super::get_img::get(cmd),
                (Ok(ItemType::Sector),false) => return super::get_img::get(cmd),
                (Ok(ItemType::Raw),_) => {},
                (Ok(_),false) => {},
                (_,true) => {
                    eprintln!("`trunc` flag only used with raw type");
                    return Err(Box::new(CommandError::InvalidCommand));
                },
                (Err(e),_) => return Err(Box::new(e))
            }
            let typ = ItemType::from_str(typ_str);
            match crate::create_fs_from_file(img_path) {
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
                        Ok(ItemType::Block) => disk.read_block(&src_path),
                        _ => Err::<(u16,Vec<u8>),DYNERR>(Box::new(CommandError::UnsupportedItemType))
                    };
                    return output_get(maybe_object,typ,Some(disk));
                },
                Err(e) => return Err(e)
            }
        },
        // we are getting a local file
        (None,None) => {
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
            error!("for `get` provide either `-f` alone, or all of `-f`, `-d`, and `-t`");
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}