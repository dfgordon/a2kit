//! ## put to image
//! 
//! Handles putting item types that do not require resolution of the file system.
//! This is invoked from the `put` module.

use clap;
use std::str::FromStr;
use log::{info,error};
use super::{ItemType,CommandError};
use crate::STDRESULT;

const RCH: &str = "unreachable was reached";
const RANGED_ACCESS: &str =
"Writing to multiple sectors is only allowed if the buffers match exactly";

pub fn put(cmd: &clap::ArgMatches,dat: &[u8]) -> STDRESULT {
    // presence of arguments should already be resolved
    let dest_path = cmd.get_one::<String>("file").expect(RCH);
    let typ = ItemType::from_str(&cmd.get_one::<String>("type").expect(RCH)).expect(RCH);
    let img_path = cmd.get_one::<String>("dimg").expect(RCH);
    let fmt = super::get_fmt(cmd)?;

    match crate::create_img_from_file(&img_path) {
        Ok(mut img) => {
            match typ {
                ItemType::Sector => {
                    if let Some(fmt) = fmt {
                        img.change_format(fmt)?;
                    }
                    let mut ptr = 0;
                    let sec_list = super::parse_sector_request(&dest_path,img.motor_steps_per_cyl())?;
                    let mut sec_len_list = Vec::new();
                    // Gather all the sector sizes, this must be done first so that
                    // we preserve angle-order during the write phase. An alternative would
                    // be to rely on the size as given by `get_chss`.
                    for (tkey,skey) in &sec_list {
                        let sec_len = img.read_pro_sector(tkey.clone(), skey.clone())?.len();
                        sec_len_list.push(sec_len);
                        ptr += sec_len;
                    }
                    // If multi-sector write, demand exact size match
                    if ptr != dat.len() && sec_list.len() > 1 {
                        error!("{}",RANGED_ACCESS);
                        return Err(Box::new(CommandError::InvalidCommand));
                    }
                    // Now write the sectors
                    ptr = 0;
                    for i in 0..sec_list.len() {
                        img.write_pro_sector(sec_list[i].0.clone(),sec_list[i].1.clone(),&dat[ptr..])?;
                        ptr += sec_len_list[i];
                    }
                },
                ItemType::RawTrack => {
                    img.set_pro_track_buf(super::request_one_track(&dest_path,img.motor_steps_per_cyl())?,dat)?
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