//! ### FAT Display Module
//! 
//! This module is concerned with displaying the directory.

use log::{trace,debug,error};
use colored::Colorize;
use std::collections::BTreeSet;
use regex::Regex;
use super::directory;
use super::pack::*;
use crate::bios::bpb::BootSector;
use super::types::Error;
use crate::{DYNERR,STDRESULT};

/// Advance cursor through string slice, return error when end is reached.
/// In honor of Applesoft CHRGET.
fn chrget(curs: &mut usize,vals: &str) -> STDRESULT {
    *curs += 1;
    match *curs < vals.len() {
        true => Ok(()),
        false => Err(Box::new(Error::Syntax))
    }
}

/// Parse string slice of form `=(a1,a2,...)` or `=a1` where a1,a2,... are alphanumeric strings.
/// Advances the cursor and returns values in a Vec.
fn parse_opt_vals(curs: &mut usize,opt: &str) -> Result<Vec<String>,DYNERR> {
    if opt.len()<=*curs || &opt[*curs..*curs+1]!="=" {
        return Err(Box::new(Error::Syntax));
    }
    chrget(curs,opt)?;
    let (is_list,patt) = match &opt[*curs..*curs+1] {
        "(" => (true,Regex::new(r"^[0-9a-zA-Z]+(,[0-9a-zA-Z]+)*\)").expect("failed to parse regex")),
        _ => (false,Regex::new(r"^[0-9a-zA-Z]+").expect("failed to parse regex"))
    };
    if is_list {
        chrget(curs,opt)?;
    }
    match patt.find(&opt[*curs..]) {
        Some(m) => {
            *curs += m.as_str().len();
            match is_list {
                true => {
                    let s = m.as_str();
                    Ok(s[0..s.len()-1].split(",").map(|x| x.to_uppercase()).collect())
                },
                false => Ok(vec![m.as_str().to_uppercase()])
            }
        },
        _ => {
            Err(Box::new(Error::Syntax))
        }
    }
}

/// Extend a filename string by padding with
/// spaces or expanding asterisk wildcard.
fn extend_fragment(short: &str,len: usize) -> Result<String,DYNERR> {
    let mut ans = String::new();
    let mut curs: usize = 0;
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

struct UserStats {
    /// this is really total_blocks normalized to 1k blocks (TODO: are holes counted?)
    total_kbytes: usize,
    total_recs: usize,
    file_count: usize,
    total_blocks: usize,
}

/// CP/M v3 directory options.
struct DirOptions {
    pattern: String, // glob pattern
    att: bool, // show attributes F1 to F4
    date: bool, // show time stamps
    dir: bool, // show DIR files (implicitly true unless SYS was given)
    _drive: BTreeSet<u8>, // specified by letter, e.g., `DRIVE=A` , `DRIVE=(A,B)` , `DRIVE=ALL`
    exclude: bool, // display negative pattern matches
    _ff: bool, // send form feed to printer
    full: bool, // show everything (seems to not include `att` flags)
    _length: u16, // lines of output before waiting for keypress
    message: bool, // show names of drives and user numbers searched (even if no file)
    _nopage: bool, // scroll without waiting for keypress
    nosort: bool, // do not sort alphabetically
    ro: bool, // show only read-only files
    rw: bool, // show only read-write files
    size: bool, // if full is not specified, show only names and sizes in 3 columns
    sys: bool, // show SYS files (only show SYS unless DIR explicitly given)
}

impl DirOptions {
    fn new() -> Self {
        Self {
            pattern: String::new(),
            att: false,
            date: false,
            dir: false,
            _drive: BTreeSet::from([0]),
            exclude: false,
            _ff: false,
            full: false,
            _length: u16::MAX,
            message: false,
            _nopage: false,
            nosort: false,
            ro: false,
            rw: false,
            size: false,
            sys: false
        }
    }
    /// Parse CP/M v3 directory options
    fn parse(opt: &str) -> Option<Result<Self,DYNERR>> {
        if opt=="" {
            return None;
        }
        let mut curs = 0;
        let mut ans = Self::new();
        let word_patt = Regex::new(r"^\w+").expect("failed to parse regex");
        loop {
            match &opt[curs..curs+1] {
                "[" => break,
                s => ans.pattern += s
            }
            curs += 1;
            if curs >= opt.len() {
                break;
            }
        }
        if curs >= opt.len() {
            return Some(Ok(ans));
        }
        curs += 1;
        if curs >= opt.len() {
            return Some(Err(Box::new(Error::Syntax)));
        }
        trace!("wildcard pattern `{}`",ans.pattern);
        // there is an interaction between `full` and `size` that depends on order
        ans.full = true;
        loop {
            match word_patt.find(&opt[curs..]) {
                Some(m) => {
                    trace!("matching argument `{}`",m.as_str());
                    let name = m.as_str().to_uppercase();
                    match name.as_str() {
                        "FULL" => {
                            ans.full = true;
                            curs += 4;
                        },
                        "DATE" => {
                            ans.date = true;
                            curs += 4;
                        },
                        "SYS" => {
                            ans.sys = true;
                            curs += 3;
                        },
                        "DIR" => {
                            ans.dir = true;
                            curs += 3;
                        },
                        "ATT" => {
                            ans.att = true;
                            curs += 3;
                        },
                        "EXCLUDE" => {
                            ans.exclude = true;
                            curs += 7;
                        },
                        "MESSAGE" => {
                            ans.message = true;
                            curs += 7;
                        },
                        "NOSORT" => {
                            ans.nosort = true;
                            curs += 6;
                        },
                        "RO" => {
                            ans.ro = true;
                            curs += 2;
                        },
                        "RW" => {
                            ans.rw = true;
                            curs += 2;
                        },
                        "SIZE" => {
                            ans.size = true;
                            ans.full = false;
                            curs += 4;
                        }
                        _ => {
                            error!("unrecognized option `{}`",&name);
                            return Some(Err(Box::new(Error::Syntax)));
                        }
                    }
                },
                None => {
                    error!("syntax error in CP/M options `{}`",opt);
                    return Some(Err(Box::new(Error::Syntax)));
                }
            }
            match (curs+1<opt.len(),opt.get(curs..curs+1)) {
                (true,Some(",")) => {
                    curs += 1;
                },
                (false,Some("]")) => {
                    return Some(Ok(ans));
                },
                (_,None) => {
                    return Some(Ok(ans));
                },
                _ => {
                    error!("syntax error in CP/M options `{}`",opt);
                    return Some(Err(Box::new(Error::Syntax)));
                }
            }
        }
    }
}

/// Display basic directory table.
fn dir_table(dir: &directory::Directory,finfo: &directory::FileInfo,stats: &mut UserStats,cols: usize,sizes: bool) {
    let kbytes = match finfo.eof { b if b>0 => 1+(b-1)/1024, _ => 0 };
    let records = match finfo.eof { b if b>0 => 1+(b-1)/128, _ => 0 };
    if stats.file_count%cols==0 {
        print!("A: ");
    } else {
        print!(" : ");
    }
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
            (true,_,_,_) => finfo.name.blue().bold(),
            (_,true,_,_) => finfo.typ.bold(),
            (_,_,true,true) => finfo.typ.red().dimmed(),
            (_,_,true,false) => finfo.typ.red(),
            (_,_,false,true) => finfo.typ.dimmed(),
            _ => finfo.typ.normal()
        },
    );
    if sizes==true {
        print!(" {:4}k",kbytes);
    }
    if stats.file_count%cols==cols-1 {
        println!();
    }
    stats.file_count += 1;
    stats.total_kbytes += kbytes;
    stats.total_recs += records;
}

fn is_displayed(finfo: &directory::FileInfo,opt: &DirOptions) -> bool {
    let mut ans = true;
    let pattern_match = match_wildcard_pattern(&opt.pattern, &finfo.name, &finfo.typ).expect("bad wildcard pattern");
    ans &= pattern_match && !opt.exclude || !pattern_match && opt.exclude;
    ans &= !(opt.dir && !opt.sys && finfo.system);
    ans &= !(opt.sys && !opt.dir && !finfo.system);
    ans &= !(opt.ro && !opt.rw && !finfo.read_only);
    ans &= !(opt.rw && !opt.ro && finfo.read_only);
    ans &= !finfo.volume_id;
    ans
}

/// Display CP/M directory in style determined by options.
/// This will behave like CP/M v3, including how it will list
/// the files on a CP/M v2 disk.
pub fn dir(dir: &directory::Directory,boot: &BootSector,opt: &str) -> STDRESULT {
    if let Some(label) = dir.find_label() {
        println!();
        println!("{}",label.name(true).blue().bold());
    }
    if let Ok(sorted) = dir.build_files() {
        // `build_files` sorts on the name automatically, so we have to "re-sort" in order
        // to get the "unsorted" list.
        let unsorted = dir.sort_on_entry_index(&sorted);
        let options = match DirOptions::parse(opt) {
            None => DirOptions::new(),
            Some(Ok(opt)) => opt,
            Some(Err(e)) => return Err(e)
        };
        let mut user_stats = UserStats {
            total_blocks: 0,
            total_kbytes: 0,
            total_recs: 0,
            file_count: 0
        };
        if !options.full && !options.size {
            println!();
            for v in unsorted.values() {
                if is_displayed(v, &options) {
                    dir_table(dir,v,&mut user_stats,5,false);
                }
            }
            if user_stats.file_count % 5 > 0 {
                println!();
            }
            if user_stats.file_count==0 {
                println!("No File");
            }
            println!();
        }
    }
    Ok(())
}