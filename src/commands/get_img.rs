//! ## get from image
//! 
//! Handles getting item types that do not require resolution of the file system.
//! This is invoked from the `get` module.

use clap;
use std::io::Write;
use std::str::FromStr;
use std::error::Error;
use log::{debug,error};
use super::{ItemType,CommandError};
use crate::img::DiskImage;

const RCH: &str = "unreachable was reached";

fn parse_sector(farg: &str) -> Result<[usize;3],Box<dyn Error>> {
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

fn parse_track(farg: &str) -> Result<[usize;2],Box<dyn Error>> {
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

fn output_get(dat: Vec<u8>,typ: ItemType,img: Box<dyn DiskImage>) {
    if atty::is(atty::Stream::Stdout) {
        match typ {
            ItemType::Track => println!("{}",img.display_track(&dat)),
            _ => crate::display_chunk(0,&dat)
        };
    } else {
        std::io::stdout().write_all(&dat).expect("could not write stdout")
    }
}

pub fn get(cmd: &clap::ArgMatches) -> Result<(),Box<dyn Error>> {
    if !atty::is(atty::Stream::Stdin) {
        error!("input is redirected, but `get` must start the pipeline");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    // presence of arguments should already be resolved
    let src_path = String::from(cmd.value_of("file").expect(RCH));
    let typ = ItemType::from_str(&String::from(cmd.value_of("type").expect(RCH))).expect(RCH);
    let img_path = String::from(cmd.value_of("dimg").expect(RCH));

    match crate::create_img_from_file(&img_path) {
        Ok(img) => {
            let bytes = match typ {
                ItemType::Sector => {
                    let [cyl,head,sec] = parse_sector(&src_path)?;
                    img.read_sector(cyl, head, sec)?
                },
                ItemType::Track => {
                    let [cyl,head] = parse_track(&src_path)?;
                    img.get_track_nibbles(cyl, head)?
                },
                ItemType::RawTrack => {
                    let [cyl,head] = parse_track(&src_path)?;
                    img.get_track_buf(cyl, head)?
                },
                _ => panic!("{}",RCH)
            };
            return Ok(output_get(bytes,typ,img));
        },
        Err(e) => return Err(e)
    }
}