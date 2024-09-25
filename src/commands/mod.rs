//! # CLI Subcommands
//! 
//! Contains modules that run the subcommands.

pub mod mkdsk;
pub mod put;
pub mod get;
pub mod get_img;
pub mod put_img;

use std::str::FromStr;
use std::io::Read;
use log::{debug,error};

use crate::DYNERR;

#[derive(thiserror::Error,Debug)]
pub enum CommandError {
    #[error("Item type is not yet supported")]
    UnsupportedItemType,
    #[error("Item type is unknown")]
    UnknownItemType,
    #[error("Command could not be interpreted")]
    InvalidCommand,
    #[error("One of the parameters was out of range")]
    OutOfRange,
    #[error("Input source is not supported")]
    UnsupportedFormat,
    #[error("Input source could not be interpreted")]
    UnknownFormat,
    #[error("File not found")]
    FileNotFound,
    #[error("Key not found")]
    KeyNotFound,
}

/// Types of files that may be distinguished by the file system or a2kit.
/// This will have to be mapped to a similar enumeration at lower levels
/// in order to obtain the binary type code.
#[derive(PartialEq,Clone,Copy)]
pub enum ItemType {
    Raw,
    Binary,
    Text,
    Records,
    FileImage,
    ApplesoftText,
    IntegerText,
    MerlinText,
    ApplesoftTokens,
    IntegerTokens,
    MerlinTokens,
    ApplesoftVars,
    IntegerVars,
    Block,
    Track,
    Sector,
    RawTrack,
    System,
    Metadata,
    Automatic
}

impl FromStr for ItemType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "raw" => Ok(Self::Raw),
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "rec" => Ok(Self::Records),
            "any" => Ok(Self::FileImage),
            "atxt" => Ok(Self::ApplesoftText),
            "itxt" => Ok(Self::IntegerText),
            "mtxt" => Ok(Self::MerlinText),
            "atok" => Ok(Self::ApplesoftTokens),
            "itok" => Ok(Self::IntegerTokens),
            "mtok" => Ok(Self::MerlinTokens),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
            "block" => Ok(Self::Block),
            "track" => Ok(Self::Track),
            "raw_track" => Ok(Self::RawTrack),
            "sec" => Ok(Self::Sector),
            "sys" => Ok(Self::System),
            "meta" => Ok(Self::Metadata),
            "auto" => Ok(Self::Automatic),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

const SEC_MESS: &str =
"sector specification should be <cyl>,<head>,<sec>` or a range";

fn parse_range(range: &str) -> Result<[usize;2],DYNERR> {
    let mut ans = [0,1];
    let mut lims = range.split("..");
    for j in 0..2 {
        match (j,lims.next()) {
            (0,Some(lim)) => {
                ans[0] = usize::from_str(lim)?;
            },
            (1,Some(lim)) => {
                ans[1] = usize::from_str(lim)?;
                if ans[1] <= ans[0] {
                    error!("sector range end was <= start");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
            },
            (1,None) => {
                ans[1] = ans[0] + 1;
            },
            _ => panic!("unexpected pattern parsing sector request")
        }
    }
    if lims.next().is_some() {
        error!("range specification should be in form `<beg>[..<end>]`");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    Ok(ans)
}

/// parse a sector request in the form `c1[..c2],h1[..h2],s1[..s2][,,next_range]`
fn parse_sector_request(farg: &str) -> Result<Vec<[usize;3]>,DYNERR> {
    let mut ans: Vec<[usize;3]> = Vec::new();
    let mut contiguous_areas = farg.split(",,");
    while let Some(contig) = contiguous_areas.next() {
        let mut ranges = contig.split(',');
        let mut bounds_set: Vec<[usize;2]> = Vec::new();
        for _i in 0..3 {
            match ranges.next() {
                Some(range) => {
                    let rng = parse_range(range)?;
                    bounds_set.push(rng);
                },
                None => {
                    error!("{}",SEC_MESS);
                    return Err(Box::new(CommandError::InvalidCommand));
                }
            }
        }
        if ranges.next().is_some() {
            error!("{}",SEC_MESS);
            return Err(Box::new(CommandError::InvalidCommand));
        }
        for cyl in bounds_set[0][0]..bounds_set[0][1] {
            for head in bounds_set[1][0]..bounds_set[1][1] {
                for sec in bounds_set[2][0]..bounds_set[2][1] {
                    ans.push([cyl,head,sec]);
                    if ans.len()>4*(u16::MAX as usize) {
                        error!("sector request has too many sectors");
                        return Err(Box::new(CommandError::InvalidCommand));
                    }
                }
            }
        }
    }
    Ok(ans)
}

/// parse a sector request in the form `cyl,head` (ranges not allowed)
fn parse_track_request(farg: &str) -> Result<[usize;2],DYNERR> {
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

/// parse a sector request in the form `b1[..b2][,,next_range]`
fn parse_block_request(farg: &str) -> Result<Vec<usize>,DYNERR> {
    let mut ans: Vec<usize> = Vec::new();
    let mut contiguous_areas = farg.split(",,");
    while let Some(contig) = contiguous_areas.next() {
        if contig.contains(",") {
            error!("unexpected single comma in block request");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let rng = parse_range(contig)?;
        for b in rng[0]..rng[1] {
            ans.push(b);
            if ans.len()>4*(u16::MAX as usize) {
                error!("block request has too many blocks");
                return Err(Box::new(CommandError::InvalidCommand));
            }
        }
    }
    Ok(ans)
}

/// get a JSON object presumed to be a list and log any errors
fn get_json_list_from_stdin() -> Result<json::JsonValue,DYNERR> {
    let mut raw_list = Vec::new();
    std::io::stdin().read_to_end(&mut raw_list)?;
    let json_list = match json::parse(&String::from_utf8(raw_list)?) {
        Ok(s) => s,
        Err(_) => {
            log::error!("input to mget was not valid JSON");
            return Err(Box::new(CommandError::InvalidCommand));
        }
    };
    if !json_list.is_array() {
        log::error!("input to mget was not a JSON list");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    Ok(json_list)
}

#[test]
fn test_parse_sec_req() {
    let single = "2,0,3";
    let contig = "2..4,0,3..5";
    let non_contig = "2..4,0,3..5,,32..34,0,0..2";
    let single_list = parse_sector_request(single).expect("could not parse");
    assert_eq!(single_list,vec![[2,0,3]]);
    let contig_list = parse_sector_request(contig).expect("could not parse");
    assert_eq!(contig_list,vec![[2,0,3],[2,0,4],[3,0,3],[3,0,4]]);
    let non_contig_list = parse_sector_request(non_contig).expect("could not parse");
    assert_eq!(non_contig_list,vec![[2,0,3],[2,0,4],[3,0,3],[3,0,4],[32,0,0],[32,0,1],[33,0,0],[33,0,1]]);
}

#[test]
fn test_parse_block_req() {
    let single = "1";
    let contig = "1..4";
    let non_contig = "1..4,,6,,8..10";
    let single_list = parse_block_request(single).expect("could not parse");
    assert_eq!(single_list,vec![1]);
    let contig_list = parse_block_request(contig).expect("could not parse");
    assert_eq!(contig_list,vec![1,2,3]);
    let non_contig_list = parse_block_request(non_contig).expect("could not parse");
    assert_eq!(non_contig_list,vec![1,2,3,6,8,9]);
}