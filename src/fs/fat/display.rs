//! ### FAT Display Module
//! 
//! This module is concerned with displaying the directory.

use log::{debug,error};
use colored::Colorize;
use super::directory;
use super::types::Error;
use crate::{DYNERR,STDRESULT};

/// Extend a filename string by padding with
/// spaces or expanding asterisk wildcard.
fn extend_fragment(short: &str,len: usize) -> Result<String,DYNERR> {
    debug!("extending {}",short);
    let mut ans = String::new();
    let mut curs: usize = 0;
    if short.len() > 0 {
        loop {
            match &short[curs..curs+1] {
                "*" => {
                    if curs+1!=short.len() {
                        error!("wildcard in illegal position");
                        return Err(Box::new(Error::Syntax));
                    }
                    for _i in curs..len {
                        ans += "?";
                    }
                    return Ok(ans);
                }
                c => ans += c
            }
            curs += 1;
            if curs >= short.len() || curs >= len {
                break;
            }
        }
    }
    for _i in curs..len {
        ans += " ";
    }
    Ok(ans)
}

/// Test for a match to a CP/M wildcard pattern
fn match_wildcard_pattern(patt_raw: &str,base_raw: &str,typ_raw: &str) -> Result<bool,DYNERR> {
    if patt_raw.len()==0 {
        return Ok(true);
    }
    if base_raw=="." || base_raw==".." {
        return Ok(false);
    }
    let mut dot_iter = patt_raw.split('.');
    let patt_base = extend_fragment(dot_iter.next().unwrap(),8)?.to_uppercase();
    let patt_typ = match dot_iter.next() {
        Some(s) => extend_fragment(s,3)?.to_uppercase(),
        None => "   ".to_string()
    };
    let base = extend_fragment(base_raw,8)?.to_uppercase();
    let typ = extend_fragment(typ_raw,3)?.to_uppercase();
    for i in 0..8 {
        if &patt_base[i..i+1]!="?" && patt_base[i..i+1]!=base[i..i+1] {
            return Ok(false);
        }
    }
    for i in 0..3 {
        if &patt_typ[i..i+1]!="?" && patt_typ[i..i+1]!=typ[i..i+1] {
            return Ok(false);
        }
    }
    Ok(true)
}

fn format_name(finfo: &directory::FileInfo) {
    print!("{:8} {:3}",
        match (finfo.directory,finfo.system,finfo.read_only,finfo.hidden) {
            (true,_,_,_) => finfo.name.blue().bold(),
            (_,true,_,_) => finfo.name.bold(),
            (_,_,true,true) => finfo.name.red().dimmed(),
            (_,_,true,false) => finfo.name.red(),
            (_,_,false,true) => finfo.name.dimmed(),
            _ => finfo.name.normal()
        },
        match (finfo.directory,finfo.system,finfo.read_only,finfo.hidden) {
            (true,_,_,_) => finfo.typ.blue().bold(),
            (_,true,_,_) => finfo.typ.bold(),
            (_,_,true,true) => finfo.typ.red().dimmed(),
            (_,_,true,false) => finfo.typ.red(),
            (_,_,false,true) => finfo.typ.dimmed(),
            _ => finfo.typ.normal()
        },
    );
}

/// Display basic directory table cell.
fn dir_table(finfo: &directory::FileInfo,count: &mut usize,cols: usize) {
    if *count%cols>0 {
        print!("    ");
    }
    format_name(finfo);
    if *count%cols==cols-1 {
        println!();
    }
    *count += 1;
}

/// Display one line of directory listing
fn dir_line(finfo: &directory::FileInfo,count: &mut usize) {
    format_name(finfo);
    if finfo.directory {
        print!(" <DIR>     ");
    } else {
        print!(" {:8}  ",finfo.eof);
    }
    if let Some(t) = finfo.write_date {
        print!("{}   ",t.format("%m-%d-%y").to_string());
        if let Some(t) = finfo.write_time {
            print!("{}",t.format("%H:%M").to_string());
        }
    }
    println!();
    *count += 1;
}

fn is_displayed(finfo: &directory::FileInfo,pattern: &str) -> bool {
    let mut ans = true;
    let pattern_match = match_wildcard_pattern(pattern, &finfo.name, &finfo.typ).expect("bad wildcard pattern");
    ans &= pattern_match;
    ans &= !finfo.volume_id;
    ans
}

/// Display FAT directory, either in normal or `wide` mode.
/// This will behave like MS-DOS 3.3, except for color highlights.
pub fn dir(path: &str,vol_lab: &str, dir: &directory::Directory,pattern: &str,wide: bool,free: u64) -> STDRESULT {
    if vol_lab!="NO NAME" {
        println!();
        println!(" Volume in drive A is {}",vol_lab.blue().bold());
    } else {
        println!();
        println!(" Volume in drive A has no label")
    }
    let displ_path = if !path.starts_with("/") {
        "/".to_string() + path
    } else {
        path.to_string()
    }.replace("/","\\").to_uppercase();
    println!(" Directory of A:{}",&displ_path);
    if let Ok(sorted) = dir.build_files() {
        // `build_files` sorts on the name automatically, so we have to "re-sort" in order
        // to get the "unsorted" list.
        let unsorted = dir.sort_on_entry_index(&sorted);
        let mut count = 0;
        if wide {
            println!();
            for v in unsorted.values() {
                if is_displayed(v, pattern) {
                    dir_table(v,&mut count,5);
                }
            }
            if count % 5 > 0 {
                println!();
            }
        } else {
            println!();
            for v in unsorted.values() {
                if is_displayed(v, pattern) {
                    dir_line(v,&mut count);
                }
            }
        }
        if count==0 {
            println!("No File");
        }
        println!("{:9} File(s)   {} bytes free",count,free);
        println!();
    }
    Ok(())
}