//! # CLI Subcommands
//! 
//! Contains modules that run the subcommands.

pub mod mkdsk;
pub mod put;
pub mod get;
pub mod get_img;
pub mod put_img;
pub mod stat;
pub mod modify;
pub mod langx;
pub mod completions;

use std::str::FromStr;
use std::io::Read;

use crate::img::tracks::{TrackKey,DiskFormat};
use crate::DYNERR;

/// process the `pro` argument, if it is a path retrieve the format from the file,
/// if it is a JSON string process it directly.
pub fn get_fmt(cmd: &clap::ArgMatches) -> Result<Option<DiskFormat>,Box<dyn std::error::Error>> {
    match cmd.get_one::<String>("pro") {
        Some(path_or_json) => {
            match json::parse(path_or_json) {
                Ok(_) => Ok(Some(DiskFormat::from_json(path_or_json)?)),
                Err(_) => {
                    let json_str = std::fs::read_to_string(path_or_json)?;
                    Ok(Some(DiskFormat::from_json(&json_str)?))
                }
            }
        },
        None => Ok(None)
    }
}

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

/// Items of information that a user might want to get-from or put-to a disk image.
/// The `ItemType` will affect how the CLI interprets the `--file` argument, i.e., as
/// an ordinary file system path, a disk address, or a metadata key path.
#[derive(PartialEq,Clone,Copy)]
pub enum ItemType {
    FileImage,
    Automatic,
    AppleSingle,
    Raw,
    Binary,
    Text,
    Records,
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
}

impl FromStr for ItemType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "any" => Ok(Self::FileImage),
            "auto" => Ok(Self::Automatic),
            "as" => Ok(Self::AppleSingle),
            "raw" => Ok(Self::Raw),
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "rec" => Ok(Self::Records),
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
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

const SEC_MESS: &str =
"sector specification should be <cyl>,<head>,<sec>` or a range";

const CYL_MESS: &str =
"cylinder specification should be a postive integer or quarter-decimal (e.g. 17.25)";

const TRK_MESS: &str =
"track specification should be `<cyl>,<head>` or a range";

/// parse an ordinary integer or a decimal that ends with
/// anything in the set ["0","00","25","5","50","75"], an error
/// is returned if the fraction is not compatible with `steps_per_cyl`.
fn parse_quarter_decimal(qdec: &str,head: usize,steps_per_cyl: usize) -> Result<TrackKey,DYNERR> {
    let cf: Vec<&str> = qdec.split('.').collect();
    if cf.len() < 1 || cf.len() > 2 {
        log::error!("{}",CYL_MESS);
        return Err(Box::new(CommandError::InvalidCommand))
    }
    let coarse = usize::from_str(cf[0])?;
    let mut fine = 0;
    if cf.len() == 2 {
        fine = match cf[1] {
            "0" | "00" => 0,
            "25" => 1,
            "5" | "50" => 2,
            "75" => 3,
            _ => {
                log::error!("{}",CYL_MESS);
                return Err(Box::new(CommandError::InvalidCommand))
            }
        };
    }
    match (steps_per_cyl,fine) {
        (1,0) => Ok(TrackKey::CH((coarse,head))),
        (2,f) if f==0 || f==2 =>  Ok(TrackKey::Motor((coarse*2 + f/2,head))),
        (4,f) => Ok(TrackKey::Motor((coarse*4+f,head))),
        _ => {
            log::error!("fractional track is incompatible with this image");
            Err(Box::new(CommandError::InvalidCommand))
        }

    }
}

/// parse something in the form n..m where n and m are integers
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
                    log::error!("end was <= start");
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
        log::error!("range specification should be in form `<beg>[..<end>]`");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    Ok(ans)
}

/// parse something in the form n..m, where n and m are integers or quarter decimals.
/// The resulting TrackKey structs will both have head = 0.
fn parse_track_range(range: &str,steps_per_cyl: usize) -> Result<[TrackKey;2],DYNERR> {
    let mut ans = [TrackKey::Track(0),TrackKey::Track(0)];
    let mut lims = range.split("..");
    for j in 0..2 {
        match (j,lims.next()) {
            (0,Some(lim)) => {
                ans[0] = parse_quarter_decimal(lim,0,steps_per_cyl)?;
            },
            (1,Some(lim)) => {
                ans[1] = parse_quarter_decimal(lim,0,steps_per_cyl)?;
                match ans[0].partial_cmp(&ans[1]) {
                    Some(std::cmp::Ordering::Equal) | Some(std::cmp::Ordering::Greater) => {
                        log::error!("end was <= start");
                        return Err(Box::new(CommandError::InvalidCommand));
                    },
                    None => {
                        log::error!("start and end must both be integers or both be quarter decimals");
                        return Err(Box::new(CommandError::InvalidCommand));
                    },
                    _ => {}
                }
            },
            (1,None) => {
                ans[1] = match ans[0] {
                    TrackKey::Motor((m,h)) => TrackKey::Motor((m+1,h)),
                    TrackKey::CH((c,h)) => TrackKey::CH((c+1,h)),
                    _ => return Err(Box::new(CommandError::InvalidCommand))
                };
            },
            _ => panic!("unexpected pattern parsing sector request")
        }
    }
    if lims.next().is_some() {
        log::error!("range specification should be in form `<beg>[..<end>]`");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    Ok(ans)
}

/// Parse a sector request in the form `c1[..c2],h1[..h2],s1[..s2][,,next_range]`.
/// The cylinder bounds can be quarter tracks, in which case the cylinder range
/// will be stepping by 4.
fn parse_sector_request(farg: &str,steps_per_cyl: usize) -> Result<Vec<(TrackKey,usize)>,DYNERR> {
    let mut ans: Vec<(TrackKey,usize)> = Vec::new();
    let mut contiguous_areas = farg.split(",,");
    while let Some(contig) = contiguous_areas.next() {
        let mut ranges = contig.split(',');
        // get track range
        let trk_rng = match ranges.next() {
            Some(range) => parse_track_range(range,steps_per_cyl)?,
            None => {
                log::error!("{}",SEC_MESS);
                return Err(Box::new(CommandError::InvalidCommand));
            }
        };
        // get head range
        let head_rng = match ranges.next() {
            Some(range) => parse_range(range)?,
            None => {
                log::error!("{}",SEC_MESS);
                return Err(Box::new(CommandError::InvalidCommand));
            }
        };
        // get sector range
        let sec_rng = match ranges.next() {
            Some(range) => parse_range(range)?,
            None => {
                log::error!("{}",SEC_MESS);
                return Err(Box::new(CommandError::InvalidCommand));
            }
        };
        if ranges.next().is_some() {
            log::error!("{}",SEC_MESS);
            return Err(Box::new(CommandError::InvalidCommand));
        }

        let mut cyl = trk_rng[0].clone();
        while cyl < trk_rng[1] {
            for head in head_rng[0]..head_rng[1] {
                for sec in sec_rng[0]..sec_rng[1] {
                    match cyl {
                        TrackKey::CH((c,_)) => ans.push((TrackKey::CH((c,head)),sec)),
                        TrackKey::Motor((m,_)) => ans.push((TrackKey::Motor((m,head)),sec)),
                        _ => panic!("unexpected track spec")
                    };
                    if ans.len()>4*(u16::MAX as usize) {
                        log::error!("sector request has too many sectors");
                        return Err(Box::new(CommandError::InvalidCommand));
                    }
                }
            }
            cyl.jump(1,None,steps_per_cyl)?;
        }
    }
    Ok(ans)
}

/// Parse a track request in the form `c1[..c2],h1[..h2][,,next_range]`.
/// The cylinder bounds can be quarter tracks, in which case the cylinder range
/// will be stepping by 4.
fn parse_track_request(farg: &str,steps_per_cyl: usize) -> Result<Vec<TrackKey>,DYNERR> {
    let mut ans: Vec<TrackKey> = Vec::new();
    let mut contiguous_areas = farg.split(",,");
    while let Some(contig) = contiguous_areas.next() {
        let mut ranges = contig.split(',');
        // get track range
        let trk_rng = match ranges.next() {
            Some(range) => parse_track_range(range,steps_per_cyl)?,
            None => {
                log::error!("{}",TRK_MESS);
                return Err(Box::new(CommandError::InvalidCommand));
            }
        };
        // get head range
        let head_rng = match ranges.next() {
            Some(range) => parse_range(range)?,
            None => {
                log::error!("{}",TRK_MESS);
                return Err(Box::new(CommandError::InvalidCommand));
            }
        };
        if ranges.next().is_some() {
            log::error!("{}",TRK_MESS);
            return Err(Box::new(CommandError::InvalidCommand));
        }

        let mut cyl = trk_rng[0].clone();
        while cyl < trk_rng[1] {
            for head in head_rng[0]..head_rng[1] {
                match cyl {
                    TrackKey::CH((c,_)) => ans.push(TrackKey::CH((c,head))),
                    TrackKey::Motor((m,_)) => ans.push(TrackKey::Motor((m,head))),
                    _ => panic!("unexpected track spec")
                };
                if ans.len()>4*(u16::MAX as usize) {
                    log::error!("track request has too many tracks");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
            }
            cyl.jump(1,None,steps_per_cyl)?;
        }
    }
    Ok(ans)
}

/// Calls parse_track_request while rejecting ranges, i.e., accept only one track
fn request_one_track(farg: &str,steps_per_cyl: usize) -> Result<TrackKey,DYNERR> {
    let v = parse_track_request(farg,steps_per_cyl)?;
    if v.len() != 1 {
        log::error!("expected exactly one track but got {}",v.len());
        return Err(Box::new(CommandError::InvalidCommand));
    }
    Ok(v[0].clone())
}

/// parse a block request in the form `b1[..b2][,,next_range]`
fn parse_block_request(farg: &str) -> Result<Vec<usize>,DYNERR> {
    let mut ans: Vec<usize> = Vec::new();
    let mut contiguous_areas = farg.split(",,");
    while let Some(contig) = contiguous_areas.next() {
        if contig.contains(",") {
            log::error!("unexpected single comma in block request");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let rng = parse_range(contig)?;
        for b in rng[0]..rng[1] {
            ans.push(b);
            if ans.len()>4*(u16::MAX as usize) {
                log::error!("block request has too many blocks");
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
    let unwrap_ts_keys = |keys: Vec<(TrackKey,usize)>| -> Vec<[usize;3]> {
        let mut ans = Vec::new();
        for k in keys {
            match k {
                (TrackKey::CH((c,h)),s) => ans.push([c,h,s]),
                _ => panic!("unhandled test scenario")
            }
        }
        ans
    };
    let single = "2,0,3";
    let contig = "2..4,0,3..5";
    let non_contig = "2..4,0,3..5,,32..34,0,0..2";
    let single_list = unwrap_ts_keys(parse_sector_request(single,1).expect("could not parse"));
    assert_eq!(single_list,vec![[2,0,3]]);
    let contig_list = unwrap_ts_keys(parse_sector_request(contig,1).expect("could not parse"));
    assert_eq!(contig_list,vec![[2,0,3],[2,0,4],[3,0,3],[3,0,4]]);
    let non_contig_list = unwrap_ts_keys(parse_sector_request(non_contig,1).expect("could not parse"));
    assert_eq!(non_contig_list,vec![[2,0,3],[2,0,4],[3,0,3],[3,0,4],[32,0,0],[32,0,1],[33,0,0],[33,0,1]]);
}

#[test]
fn test_parse_flux_req() {
    let unwrap_keys = |keys: Vec<TrackKey>| -> Vec<[usize;2]> {
        let mut ans = Vec::new();
        for k in keys {
            match k {
                TrackKey::CH((c,h)) => ans.push([c,h]),
                _ => panic!("unhandled test scenario")
            }
        }
        ans
    };
    let single = "2,0";
    let contig = "2..4,0";
    let non_contig = "2..4,0,,32..34,0";
    let single_list = unwrap_keys(parse_track_request(single,1).expect("could not parse"));
    assert_eq!(single_list,vec![[2,0]]);
    let contig_list = unwrap_keys(parse_track_request(contig,1).expect("could not parse"));
    assert_eq!(contig_list,vec![[2,0],[3,0]]);
    let non_contig_list = unwrap_keys(parse_track_request(non_contig,1).expect("could not parse"));
    assert_eq!(non_contig_list,vec![[2,0],[3,0],[32,0],[33,0]]);
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