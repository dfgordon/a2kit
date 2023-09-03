use clap;
use std::io::Read;
use std::str::FromStr;
use log::error;
use super::{ItemType,CommandError};
use crate::fs::{Records,FileImage};
use crate::{STDRESULT,DYNERR};

const RANGED_ACCESS: &str =
"Writing to multiple blocks is only allowed if the buffers match exactly";

pub fn put(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        error!("cannot use `put` with console input, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    // if !atty::is(atty::Stream::Stdout) {
    //     error!("output is redirected, but `put` must end the pipeline");
    //     return Err(Box::new(CommandError::InvalidCommand));
    // }
    let maybe_dest_path = cmd.get_one::<String>("file");
    let maybe_typ = cmd.get_one::<String>("type");
    let maybe_img = cmd.get_one::<String>("dimg");
    let mut file_data = Vec::new();
    std::io::stdin().read_to_end(&mut file_data).expect("failed to read input stream");
    if file_data.len()==0 {
        error!("put did not receive any data from previous node");
        return Err(Box::new(CommandError::InvalidCommand));
    }

    match (maybe_typ,maybe_img,maybe_dest_path) {
        
        // we are putting a specific item to a disk image
        (Some(typ_str),Some(img_path),Some(dest_path)) => {
            let typ = ItemType::from_str(typ_str);
            // For items that don't need a file system, handle differently
            match typ {
                Ok(ItemType::Track) | Ok(ItemType::RawTrack) | Ok(ItemType::Sector) => return super::put_img::put(cmd,&file_data),
                Ok(ItemType::Metadata) => return super::put_img::put_meta(cmd,&file_data),
                _ => {}
            }
            let load_address: u16 = match (cmd.get_one::<String>("addr"),&typ) {
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
                        Ok(ItemType::Block) => {
                            let mut ptr = 0;
                            let blocks = super::parse_block_request(&dest_path)?;
                            for b in &blocks {
                                // read the block to get its length
                                let block_len = disk.read_block(&b.to_string())?.1.len();
                                if ptr + block_len > file_data.len() && block_len > 1 {
                                    error!("{}",RANGED_ACCESS);
                                    return Err(Box::new(CommandError::InvalidCommand));
                                }
                                if ptr >= file_data.len() {
                                    error!("{}",RANGED_ACCESS);
                                    return Err(Box::new(CommandError::InvalidCommand));
                                }
                                disk.write_block(&b.to_string(),&file_data[ptr..ptr+block_len])?;
                                ptr += block_len;
                            }
                            if blocks.len() > 1 && ptr != file_data.len() {
                                error!("{}",RANGED_ACCESS);
                                return Err(Box::new(CommandError::InvalidCommand));
                            }
                            Ok(ptr)
                        },
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

        // this pattern can be used for metadata only
        (Some(type_str),Some(_),None) => {
            match ItemType::from_str(type_str) {
                Ok(ItemType::Metadata) => return super::put_img::put_meta(cmd,&file_data),
                Ok(_) => {
                    error!("please narrow the item with `-f`");
                    Err(Box::new(CommandError::InvalidCommand))
                },
                Err(e) => Err(Box::new(e))
            }
        },

        // this pattern means we have a local file
        (None,None,Some(dest_path)) => {
            std::fs::write(&dest_path,&file_data).expect("could not write data to disk");
            return Ok(());
        },

        // arguments inconsistent
        _ => {
            match (maybe_typ,maybe_img) {
                (Some(_),None) => error!("please specify disk image with `-d`"),
                (Some(_),Some(_)) => error!("please narrow the item with `-f`"),
                (None,Some(_)) => error!("please narrow the type of item with `-t`"),
                (None,None) => error!("please provide arguments")
            }
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}