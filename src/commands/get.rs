use clap;
use std::io::Write;
use std::str::FromStr;
use std::error::Error;
use super::{ItemType,CommandError};
use crate::fs::DiskFS;

const RCH: &str = "unreachable was reached";

// TODO: somehow fold FileImage and Records into the pattern
fn output_get(
    maybe_object: Result<(u16,Vec<u8>),Box<dyn Error>>,
    maybe_typ: Result<ItemType,CommandError>,
    maybe_disk: Option<Box<dyn DiskFS>>,
) -> Result<(),Box<dyn Error>> {
    match maybe_object {
        Ok(tuple) => {
            let object = tuple.1;
            if atty::is(atty::Stream::Stdout) {
                match (maybe_typ,maybe_disk) {
                    (Ok(ItemType::Text),Some(disk)) => println!("{}",disk.decode_text(&object)),
                    (Ok(ItemType::Track),None) => crate::display_track(tuple.0,&object),
                    _ => crate::display_chunk(tuple.0,&object)
                };
            } else {
                match (maybe_typ,maybe_disk) {
                    (Ok(ItemType::Text),Some(disk)) => println!("{}",disk.decode_text(&object)),
                    _ => std::io::stdout().write_all(&object).expect("could not write stdout")
                };
            }
            return Ok(())
        },
        Err(e) => return Err(e)
    }
}

pub fn get(cmd: &clap::ArgMatches) -> Result<(),Box<dyn Error>> {
    if !atty::is(atty::Stream::Stdin) {
        eprintln!("input is redirected, but `get` must start the pipeline");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let src_path = String::from(cmd.value_of("file").expect(RCH));
    let maybe_typ = cmd.value_of("type");
    let maybe_img = cmd.value_of("dimg");

    match (maybe_typ,maybe_img) {

        // we are getting from a disk image
        (Some(typ_str),Some(img_path)) => {
            let typ = ItemType::from_str(typ_str);
            // If getting a track, no need to resolve file system, handle differently
            match typ {
                Ok(ItemType::Track) | Ok(ItemType::RawTrack) => {
                    match crate::create_img_from_file(img_path) {
                        Ok(img) => {
                            let maybe_object = match typ {
                                Ok(ItemType::Track) => img.get_track_bytes(&src_path),
                                Ok(ItemType::RawTrack) => img.get_track_buf(&src_path),
                                _ => panic!("{}",RCH)
                            };
                            return output_get(maybe_object,typ,None);
                        },
                        Err(e) => return Err(e)
                    }
                },
                _ => {}
            }
            match crate::create_fs_from_file(img_path) {
                Ok(disk) => {
                    // special handling for sparse data
                    if let Ok(ItemType::FileImage) = typ {
                        return match disk.read_any(&src_path) {
                            Ok(chunks) => {
                                println!("{}",chunks.to_json(4));
                                Ok(())
                            },
                            Err(e) => Err(e)
                        }
                    }
                    // special handling for random access text
                    if let Ok(ItemType::Records) = typ {
                        let record_length = match cmd.value_of("len") {
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
                        Ok(ItemType::Raw) => disk.read_text(&src_path),
                        Ok(ItemType::Chunk) => disk.read_chunk(&src_path),
                        _ => {
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        }
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
            eprintln!("for `get` provide either `-f` alone, or all of `-f`, `-d`, and `-t`");
            return Err(Box::new(CommandError::InvalidCommand))
        }
    }
}