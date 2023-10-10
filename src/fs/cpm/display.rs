//! ### CP/M Display Module
//! 
//! This module is concerned with displaying the directory.  Most of the
//! machinery has to do with emulating the many CP/M v3 display options.

use log::{trace,debug,error};
use colored::Colorize;
use std::collections::BTreeSet;
use regex::Regex;
use super::directory;
use super::pack::*;
use crate::bios::dpb::DiskParameterBlock;
use super::types;
use crate::{DYNERR,STDRESULT};

/// Advance cursor through string slice, return error when end is reached.
/// In honor of Applesoft CHRGET.
fn chrget(curs: &mut usize,vals: &str) -> STDRESULT {
    *curs += 1;
    match *curs < vals.len() {
        true => Ok(()),
        false => Err(Box::new(types::Error::BadFormat))
    }
}

/// Parse string slice of form `=(a1,a2,...)` or `=a1` where a1,a2,... are alphanumeric strings.
/// Advances the cursor and returns values in a Vec.
fn parse_opt_vals(curs: &mut usize,opt: &str) -> Result<Vec<String>,DYNERR> {
    if opt.len()<=*curs || &opt[*curs..*curs+1]!="=" {
        return Err(Box::new(types::Error::BadFormat));
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
            Err(Box::new(types::Error::BadFormat))
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
                    return Err(Box::new(types::Error::BadFormat));
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
    user: BTreeSet<u8>, // e.g. `USER=ALL`, `USER=0`, `USER=(0,1,2)`
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
            sys: false,
            user: BTreeSet::from([0])
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
            return Some(Err(Box::new(types::Error::BadFormat)));
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
                        "USER" => {
                            ans.user = BTreeSet::new();
                            curs += 4;
                            let vals = match parse_opt_vals(&mut curs, opt) {
                                Ok(v) => v,
                                Err(e) => return Some(Err(e))
                            };
                            if vals.len()==1 && vals[0].as_str()=="ALL" {
                                ans.user = BTreeSet::from([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
                            } else {
                                for v in vals {
                                    debug!("user asked for USER={}",v);
                                    match str::parse::<u8>(&v) {
                                        Ok(user) if user < types::USER_END => {
                                            ans.user.insert(user);
                                        },
                                        Ok(user) => {
                                            error!("invalid user number {}",user);
                                            return Some(Err(Box::new(types::Error::BadFormat)));
                                        },
                                        Err(e) => {
                                            return Some(Err(Box::new(e)));
                                        }
                                    }
                                }
                            }
                        },
                        _ => {
                            error!("unrecognized option `{}`",&name);
                            return Some(Err(Box::new(types::Error::BadFormat)));
                        }
                    }
                },
                None => {
                    error!("syntax error in CP/M options `{}`",opt);
                    return Some(Err(Box::new(types::Error::BadFormat)));
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
                    return Some(Err(Box::new(types::Error::BadFormat)));
                }
            }
        }
    }
}

/// Display basic directory table.
/// Intrinsic v2 table: cols=4, sizes=false.
/// Intrinsic v3 table: cols=5, sizes=false.
/// If SIZE and not FULL: cols=3, sizes=true.
fn dir_table(dir: &directory::Directory,finfo: &directory::FileInfo,stats: &mut UserStats,cols: usize,sizes: bool) {
    let bytes = match finfo.entries.last_key_value() {
        Some((_k,v))=> {
            let fx = dir.get_entry::<directory::Extent>(v).unwrap();
            fx.get_eof()
        }
        None => 0
    };
    let kbytes = match bytes { b if b>0 => 1+(b-1)/1024, _ => 0 };
    let records = match bytes { b if b>0 => 1+(b-1)/128, _ => 0 };
    if stats.file_count%cols==0 {
        print!("A: ");
    } else {
        print!(" : ");
    }
    print!("{:8} {:3}",
        match (finfo.read_only,finfo.system) {
            (true,true) => finfo.name.red().dimmed(),
            (true,false) => finfo.name.red(),
            (false,true) => finfo.name.dimmed(),
            (false,false) => finfo.name.normal()
        },
        match (finfo.read_only,finfo.system) {
            (true,true) => finfo.typ.red().dimmed(),
            (true,false) => finfo.typ.red(),
            (false,true) => finfo.typ.dimmed(),
            (false,false) => finfo.typ.normal()
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
    stats.total_blocks += finfo.blocks_allocated;
}

/// Display first 40 columns of a long directory entry
fn dir_first40(dir: &directory::Directory,finfo: &directory::FileInfo,stats: &mut UserStats,show_att: bool) {
    let mut attr = String::new();
    attr += match finfo.system { true => "Sys ", false => "Dir " };
    attr += match finfo.read_only { true => "RO  ", false => "RW  " };
    attr += match show_att && finfo.f1 { true => "1", false => " " };
    attr += match show_att && finfo.f2 { true => "2", false => " " };
    attr += match show_att && finfo.f3 { true => "3", false => " " };
    attr += match show_att && finfo.f4 { true => "4", false => " " };
    let bytes = match finfo.entries.last_key_value() {
        Some((_k,v))=> {
            let fx = dir.get_entry::<directory::Extent>(v).unwrap();
            fx.get_eof()
        }
        None => 0
    };
    let kbytes = match bytes { b if b>0 => 1+(b-1)/1024, _ => 0 };
    let records = match bytes { b if b>0 => 1+(b-1)/128, _ => 0 };
    print!("{:8} {:3} {:5}k {:6} {}",
        finfo.name,
        finfo.typ,
        kbytes,
        records,
        attr
    );
    stats.file_count += 1;
    stats.total_kbytes += kbytes;
    stats.total_recs += records;
    stats.total_blocks += finfo.blocks_allocated;
}

/// Display last 40 columns of a long directory entry
fn dir_last40(finfo: &directory::FileInfo) {
    let mut access_create_data = String::new();
    access_create_data += &match finfo.access_time {
        Some([0,0,0,0]) => "".to_string(),
        Some(t) => unpack_date(t).format("%m/%d/%y %H:%M").to_string(),
        None => "".to_string()
    };
    access_create_data += &match finfo.create_time {
        Some([0,0,0,0]) => "".to_string(),
        Some(t) => unpack_date(t).format("%m/%d/%y %H:%M").to_string(),
        None => "".to_string()
    };
    let mut prot = String::new();
    prot += match finfo.read_pass { true => "R", false => "" };
    prot += match finfo.write_pass { true => "W", false => "" };
    prot += match finfo.del_pass { true => "D", false => "" };
    print!("{:6} {:14}  {:14}",
        match prot.len() {
            0 => "None",
            _ => &prot
        },
        match finfo.update_time {
            Some([0,0,0,0]) => "".to_string(),
            Some(t) => unpack_date(t).format("%m/%d/%y %H:%M").to_string(),
            None => "".to_string()
        },
        access_create_data
    );
}

/// SHOW [LABEL] as in CP/M v3
pub fn show_label(lab: &directory::Label) {
    let (base,typ) = lab.get_split_string();
    let access_create = match lab.is_timestamped_access() { true => "Access", false => "Create" };
    println!("{:12}  {:7}  {:6}  {:6}","Directory","Passwds","Stamp","Stamp");
    println!("{:12}  {:7}  {:6}  {:6}  {:14}  {:14}","Label","Reqd",access_create,"Update","Label Created","Label Updated");
    println!("------------  -------  ------  ------  --------------  --------------");
    println!("{:8}.{:3}  {:7}  {:6}  {:6}  {:14}  {:14}",
        base,
        typ,
        match lab.is_protected() { true => "on", false => "off" },
        match lab.is_timestamped_creation() || lab.is_timestamped_access() { true => "on", false => "off" },
        match lab.is_timestamped_update() { true => "on", false => "off" },
        unpack_date(lab.get_create_time()).format("%m/%d/%y %H:%M"),
        unpack_date(lab.get_update_time()).format("%m/%d/%y %H:%M")
    );
}

fn is_displayed(user: u8,finfo: &directory::FileInfo,opt: &DirOptions) -> bool {
    let mut ans = true;
    let pattern_match = match_wildcard_pattern(&opt.pattern, &finfo.name, &finfo.typ).expect("bad wildcard pattern");
    ans &= pattern_match && !opt.exclude || !pattern_match && opt.exclude;
    ans &= user==finfo.user;
    ans &= !(opt.dir && !opt.sys && finfo.system);
    ans &= !(opt.sys && !opt.dir && !finfo.system);
    ans &= !(opt.ro && !opt.rw && !finfo.read_only);
    ans &= !(opt.rw && !opt.ro && finfo.read_only);
    ans
}

/// Display CP/M directory in style determined by options.
/// This will behave like CP/M v3, including how it will list
/// the files on a CP/M v2 disk.
pub fn dir(dir: &directory::Directory,dpb: &DiskParameterBlock,opt: &str) -> STDRESULT {
    let maybe_lab = dir.find_label();
    let access_create = match &maybe_lab {
        Some(lab) => {
            println!();
            println!("Label for drive A:");
            println!();
            show_label(&lab);
            match lab.is_timestamped_access() { true => "Access", false => "Create" }
        },
        None => {
            "Create"
        }
    };
    let (protected,timestamped) = match &maybe_lab {
        Some(lab) => (lab.is_protected(),lab.is_timestamped()),
        None => (false,false)
    };
    if let Ok(sorted) = dir.build_files(dpb,[3,1,0]) {
        // `build_files` sorts on the name automatically, so we have to "re-sort" in order
        // to get the "unsorted" list.
        let unsorted = dir.sort_on_entry_index(&sorted);
        let first40_heading = "    Name     Bytes   Recs   Attributes ";
        let first40_sep =     "------------ ------ ------ ------------";
        let last40_heading = String::from(" Prot      Update          ") + access_create;
        let last40_sep =                    "------ --------------  --------------";
        let options = match DirOptions::parse(opt) {
            None => DirOptions::new(),
            Some(Ok(opt)) => opt,
            Some(Err(e)) => return Err(e)
        };
        for user in &options.user {
            let mut user_stats = UserStats {
                total_blocks: 0,
                total_kbytes: 0,
                total_recs: 0,
                file_count: 0
            };
            if !options.full && !options.size {
                println!();
                for v in unsorted.values() {
                    if is_displayed(*user, v, &options) {
                        match maybe_lab {
                            Some(_) => dir_table(dir,v,&mut user_stats,5,false),
                            None => dir_table(dir,v,&mut user_stats,4,false)
                        };
                    }
                }
                if user_stats.file_count % 4 > 0 && maybe_lab.is_none() {
                    println!();
                }
                if user_stats.file_count % 5 > 0 && maybe_lab.is_some() {
                    println!();
                }
                if user_stats.file_count==0 {
                    println!("No File");
                }
                println!();
                continue;
            }
            println!();
            println!("Directory for Drive A: User {}",user);
            println!();
            let files: Vec<&directory::FileInfo> = match options.nosort {
                true => unsorted.values().collect(),
                false => sorted.values().collect()
            };
            match (options.full,options.size,protected || timestamped) {
                (false,true,_) => {
                    // asked for size, but not full, 3 files per row
                    for v in files {
                        if is_displayed(*user,v,&options) {
                            dir_table(dir, v, &mut user_stats,3,true);
                        }
                    }
                    if user_stats.file_count % 3 > 0 {
                        println!();
                    }
                }
                (_,_,true) => {
                    // full record with one file per row
                    for v in files {
                        if is_displayed(*user, v, &options) {
                            if user_stats.file_count==0 {
                                println!("{} {}",first40_heading,last40_heading);
                                println!("{} {}",first40_sep,last40_sep);
                                println!();
                            }
                            dir_first40(dir,v,&mut user_stats,options.att);
                            print!(" ");
                            dir_last40(v);
                            println!();
                        }
                    }
                },
                (_,_,false) => {
                    // No timestamps or protection, 2 files per row
                    for v in files.iter() {
                        if is_displayed(*user, v, &options) {
                            if user_stats.file_count==0 {
                                println!("{} {}",first40_heading,first40_heading);
                                println!("{} {}",first40_sep,first40_sep);
                                println!();
                            }
                            dir_first40(dir,v,&mut user_stats,options.att);
                            if user_stats.file_count%2==1 {
                                print!(" ");
                            } else {
                                println!();
                            }
                        }
                    }
                    if user_stats.file_count % 2 > 0 {
                        println!();
                    }
                }
            }
            // Count entries used.
            // This is not what CP/M does, we are treating every timestamp extent as used.
            // CP/M treats some timestamp extents as unused, it is not clear how or why.
            // We use different words in the output to help call this out.
            let mut used_entries = 0;
            for i in 0..dir.num_entries() {
                used_entries += match dir.get_type(&types::Ptr::ExtentEntry(i)) {
                    types::ExtentType::Deleted => 0,
                    _ => 1
                };
            }
            // Display the cumulative stats
            if user_stats.file_count==0 {
                println!("No File");
                println!();
            } else {
                println!();
                println!("Total Bytes     = {:6}k  Total Records = {:7}  Files Found = {:4}",
                    user_stats.total_kbytes,user_stats.total_recs,user_stats.file_count);
                println!("Total {}k Blocks = {:6}   Occupied/Tot Entries For Drive A: {:4}/{:4}",
                    dpb.block_size()/1024,user_stats.total_blocks,used_entries,dir.num_entries());
                println!();
            }
        }
    }
    Ok(())
}