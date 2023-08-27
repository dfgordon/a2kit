//! ## put to image
//! 
//! Handles putting item types that do not require resolution of the file system.
//! This is invoked from the `put` module.

use clap;
use std::str::FromStr;
use log::{debug,info,error};
use super::{ItemType,CommandError};
use crate::{STDRESULT,DYNERR};

const RCH: &str = "unreachable was reached";

fn parse_sector(farg: &str) -> Result<[usize;3],DYNERR> {
    let fcopy = String::from(farg);
    let it: Vec<&str> = fcopy.split(',').collect();
    if it.len()!=3 {
        error!("sector specification should be in form `cylinder,head,sector`");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let cyl = usize::from_str(it[0])?;
    let head = usize::from_str(it[1])?;
    let sec = usize::from_str(it[2])?;
    debug!("user requested cyl {} head {} sec {}",cyl,head,sec);
    Ok([cyl,head,sec])
}

fn parse_track(farg: &str) -> Result<[usize;2],DYNERR> {
    let fcopy = String::from(farg);
    let it: Vec<&str> = fcopy.split(',').collect();
    if it.len()!=2 {
        error!("track specification should be in form `cylinder,head`");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let cyl = usize::from_str(it[0])?;
    let head = usize::from_str(it[1])?;
    debug!("user requested cyl {} head {}",cyl,head);
    Ok([cyl,head])
}

pub fn put(cmd: &clap::ArgMatches,dat: &[u8]) -> STDRESULT {
    // presence of arguments should already be resolved
    let dest_path = cmd.get_one::<String>("file").expect(RCH);
    let typ = ItemType::from_str(&cmd.get_one::<String>("type").expect(RCH)).expect(RCH);
    let img_path = cmd.get_one::<String>("dimg").expect(RCH);

    match crate::create_img_from_file(&img_path) {
        Ok(mut img) => {
            match typ {
                ItemType::Sector => {
                    let [cyl,head,sec] = parse_sector(&dest_path)?;
                    img.write_sector(cyl, head, sec, dat)?
                },
                ItemType::RawTrack => {
                    let [cyl,head] = parse_track(&dest_path)?;
                    img.set_track_buf(cyl, head, dat)?
                },
                ItemType::Track => {
                    error!("cannot copy nibbles, try using the raw track");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                _ => panic!("{}",RCH)
            };
            std::fs::write(img_path,img.to_bytes())?;
            return Ok(());
        },
        Err(e) => return Err(e)
    }
}

pub fn put_meta(cmd: &clap::ArgMatches,dat: &[u8]) -> STDRESULT {
    // presence of arguments should already be resolved
    let maybe_selection = cmd.get_one::<String>("file");
    let img_path = cmd.get_one::<String>("dimg").expect(RCH);

    match crate::create_img_from_file(&img_path) {
        Ok(mut img) => {
            let json_string = String::from_utf8(dat.to_vec())?;
            let parsed = json::parse(&json_string)?;
            let mut curs = crate::JsonCursor::new();
            while let Some((key,leaf)) = curs.next(&parsed) {
                if key=="_pretty" {
                    match curs.parent(&parsed) {
                        Some(parent) => {
                            if !parent.has_key("_raw") {
                                error!("found _pretty key but no _raw key for {}",parent.to_string());
                                return Err(Box::new(CommandError::KeyNotFound));
                            }
                        },
                        None => {
                            error!("found _pretty key without a parent");
                            return Err(Box::new(CommandError::UnknownFormat));
                        }
                    }
                    continue;
                }
                if let Some(selection) = maybe_selection {
                    if selection.chars().next()!=Some('/') {
                        error!("selection string should start with `/`");
                        return Err(Box::new(CommandError::KeyNotFound));
                    }
                    if selection.ends_with("/") && curs.key_path_string().contains(selection) {
                        img.put_metadata(&curs.key_path(), leaf)?;
                    } else if &curs.key_path_string()==selection {
                        img.put_metadata(&curs.key_path(), leaf)?;
                    } else {
                        info!("skipping `{}` based on selection string `{}`",curs.key_path_string(),selection);
                    }
                } else {
                    img.put_metadata(&curs.key_path(), leaf)?;
                }
            }
            std::fs::write(img_path,img.to_bytes())?;
            Ok(())
        },
        Err(e) => return Err(e)
    }
}