use clap;
use std::io::Read;
use std::str::FromStr;
use log::error;
use super::{ItemType,CommandError};
use crate::fs::{Records,FileImage};
use crate::{STDRESULT,DYNERR};

const RCH: &str = "unreachable was reached";

pub fn put(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        error!("cannot use `put` with console input, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    // if !atty::is(atty::Stream::Stdout) {
    //     error!("output is redirected, but `put` must end the pipeline");
    //     return Err(Box::new(CommandError::InvalidCommand));
    // }
    let dest_path = String::from(cmd.value_of("file").expect(RCH));
    let maybe_typ = cmd.value_of("type");
    let maybe_img = cmd.value_of("dimg");
    let mut file_data = Vec::new();
    std::io::stdin().read_to_end(&mut file_data).expect("failed to read input stream");
    if file_data.len()==0 {
        error!("put did not receive any data from previous node");
        return Err(Box::new(CommandError::InvalidCommand));
    }

    match (maybe_typ,maybe_img) {
        
        // we are writing to a disk image
        (Some(typ_str),Some(img_path)) => {
            let typ = ItemType::from_str(typ_str);
            // If putting a track or sector, no need to resolve file system, handle differently
            match typ {
                Ok(ItemType::Track) | Ok(ItemType::RawTrack) | Ok(ItemType::Sector) => return super::put_img::put(cmd,&file_data),
                _ => {}
            }
            let load_address: u16 = match (cmd.value_of("addr"),&typ) {
                (Some(a),_) => u16::from_str(a).expect("bad address"),
                (_ ,Ok(ItemType::Binary)) => {
                    error!("binary file requires an address");
                    return Err(Box::new(CommandError::InvalidCommand));
                },
                _ => 768 as u16
            };
            match crate::create_fs_from_file(img_path) {
                Ok(mut disk) => {
                    let result = match typ {
                        Ok(ItemType::ApplesoftTokens) => disk.save(&dest_path,&file_data,ItemType::ApplesoftTokens,None),
                        Ok(ItemType::IntegerTokens) => disk.save(&dest_path,&file_data,ItemType::IntegerTokens,None),
                        Ok(ItemType::MerlinTokens) => disk.write_text(&dest_path,&file_data),
                        Ok(ItemType::Binary) => disk.bsave(&dest_path,&file_data,load_address,None),
                        Ok(ItemType::Text) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match disk.encode_text(s) {
                                Ok(encoded) => disk.write_text(&dest_path,&encoded),
                                Err(e) => Err::<usize,DYNERR>(e)
                            },
                            _ => {
                                error!("could not encode data as UTF8");
                                Err::<usize,DYNERR>(Box::new(CommandError::UnknownFormat))
                            }
                        },
                        Ok(ItemType::Raw) => disk.write_text(&dest_path,&file_data),
                        Ok(ItemType::Block) => disk.write_block(&dest_path,&file_data),
                        Ok(ItemType::Records) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match Records::from_json(s) {
                                Ok(recs) => disk.write_records(&dest_path,&recs),
                                Err(e) => Err(e)
                            },
                            _ => {
                                error!("could not encode data as UTF8");
                                Err::<usize,DYNERR>(Box::new(CommandError::UnknownFormat))
                            }
                        },
                        Ok(ItemType::FileImage) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match FileImage::from_json(s) {
                                Ok(fimg) => disk.write_any(&dest_path,&fimg),
                                Err(e) => Err(e)
                            },
                            _ => {
                                error!("could not encode data as UTF8");
                                Err::<usize,DYNERR>(Box::new(CommandError::UnknownFormat))
                            }
                        },
                        _ => Err::<usize,DYNERR>(Box::new(CommandError::UnsupportedItemType))
                    };
                    return match result {
                        Ok(_len) => crate::save_img(&mut disk,img_path),
                        Err(e) => Err(e)
                    }
                },
                Err(e) => return Err(e)
            }
        },

        // we are writing to a local file
        (None,None) => {
            std::fs::write(&dest_path,&file_data).expect("could not write data to disk");
            return Ok(());
        },

        // arguments inconsistent
        _ => {
            error!("for `put` provide either `-f` alone, or all of `-f`, `-d`, and `-t`");
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}