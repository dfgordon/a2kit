//! ## get from image
//! 
//! Handles getting item types that do not require resolution of the file system.
//! This is invoked from the `get` module.

use clap;
use std::io::Write;
use std::str::FromStr;
use log::error;
use super::{ItemType,CommandError};
use crate::img::DiskImage;
use crate::STDRESULT;

const RCH: &str = "unreachable was reached";

fn output_get(dat: Vec<u8>,typ: ItemType,img: Box<dyn DiskImage>,console_fmt: bool) {
    if atty::is(atty::Stream::Stdout) || console_fmt {
        match typ {
            ItemType::Track => println!("{}",img.display_track(&dat)),
            _ => crate::display_block(0,&dat)
        };
    } else {
        std::io::stdout().write_all(&dat).expect("could not write stdout")
    }
}

pub fn get(cmd: &clap::ArgMatches) -> STDRESULT {
    // presence of arguments should already be resolved
    let src_path = cmd.get_one::<String>("file").expect(RCH);
    let typ = ItemType::from_str(&cmd.get_one::<String>("type").expect(RCH)).expect(RCH);
    let maybe_img_path = cmd.get_one::<String>("dimg");
    let fmt = super::get_fmt(cmd)?;
    let method = crate::img::tracks::Method::from_str(cmd.get_one::<String>("method").unwrap())?;

    match crate::create_img_from_file_or_stdin(maybe_img_path) {
        Ok(mut img) => {
            if let Some(fmt) = fmt {
                img.change_format(fmt)?;
            }
            img.change_method(method);
            let bytes = match typ {
                ItemType::Sector => {
                    let mut cum: Vec<u8> = Vec::new();
                    let sector_list = super::parse_sector_request(&src_path,img.motor_steps_per_cyl())?;
                    for (tkey,skey) in sector_list {
                        cum.append(&mut img.read_pro_sector(tkey,skey)?);
                    }
                    cum
                },
                ItemType::Track => {
                    img.get_pro_track_nibbles(super::parse_track_request(&src_path,img.motor_steps_per_cyl())?)?
                },
                ItemType::RawTrack => {
                    img.get_pro_track_buf(super::parse_track_request(&src_path,img.motor_steps_per_cyl())?)?
                },
                _ => panic!("{}",RCH)
            };
            return Ok(output_get(bytes,typ,img,cmd.get_flag("console")));
        },
        Err(e) => return Err(e)
    }
}

pub fn get_meta(cmd: &clap::ArgMatches) -> STDRESULT {
    // presence of arguments should already be resolved
    let maybe_selection = cmd.get_one::<String>("file");
    let maybe_img_path = cmd.get_one::<String>("dimg");

    match crate::create_img_from_file_or_stdin(maybe_img_path) {
        Ok(img) => {
            match maybe_selection {
                None => {
                    println!("{}",img.get_metadata(Some(4)));
                    Ok(())
                }
                Some(selection) => {
                    if selection.chars().next()!=Some('/') {
                        error!("selection string should start with `/`");
                        return Err(Box::new(CommandError::KeyNotFound));
                    }
                    match json::parse(&img.get_metadata(None)) {
                        Ok(parsed) => {
                            let mut keys = selection.split('/');
                            let mut obj = parsed;
                            keys.next();
                            while let Some(key) = keys.next() {
                                if key=="" {
                                    break;
                                }
                                match obj[key].clone() {
                                    json::JsonValue::Null => return Err(Box::new(CommandError::KeyNotFound)),
                                    x => { obj = x }
                                };
                            }
                            println!("{}",json::stringify_pretty(obj, 4));
                            Ok(())
                        },
                        Err(e) => Err(Box::new(e))
                    }
                }
            }
        },
        Err(e) => return Err(e)
    }
}