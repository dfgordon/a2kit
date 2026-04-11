//! # Provide the `cp` subcommand
//! 
//! This subcommand is specially designed to provide *convenient* syntax and semantics for shuttling
//! files back and forth between disk images and the host file system.
//! It works for image-to-image copying if the file systems are the same.
//! It will reject a host-to-host copy request since that should be handled by native commands.
//! The get and put subcommands, while less convenient, are more precise, and might be a better
//! choice for mission critical scripts.

use clap;
use regex::Regex;
use std::str::FromStr;
use num_traits::FromPrimitive;
use std::path::PathBuf;
use std::sync::LazyLock;

use super::CommandError;
use crate::fs::{DiskFS,FileImage,UnpackedData,dos3x,prodos};
use crate::img::tracks::Method;
use crate::{DYNERR,STDRESULT};
use crate::lang::{applesoft,integer,merlin,is_lang};

static CPM_COLON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\/|\\)([0-9][0-9]?)(:)").unwrap());
static CPM_UNDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\/|\\)([0-9][0-9]?)(_)").unwrap());
static DRIVE_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z]:").unwrap());
static CIDERPRESS_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]$").unwrap());

struct Source {
    pub fimg: FileImage,
    pub fused_path: String
}

enum Destination {
    /// disk object, path to image, path inside image
    Dimg(Box<dyn DiskFS>,String,String),
    /// path on the host system
    Host(String)
}

/// Parse a CiderPress suffix to extract (type,aux). May panic if `suffix` format is wrong.
fn parse_cp_suffix(suffix: &str) -> (u8,usize) {
    let typ = hex::decode(&suffix[1..3]).expect("bad suffix");
    let aux = hex::decode(&suffix[3..7]).expect("bad suffix");
    (typ[0],u16::from_be_bytes([aux[0],aux[1]]) as usize)
}

/// Using the `old` pattern find the CP/M user prefix in `path`,
/// and replace the delimiter with a new one.  Return the updated `path`.
fn swap_cpm_delimiter(path: &str,old: &LazyLock<Regex>,new: &str) -> String {
    let rep = r"${1}${2}x".replace('x',new);
    old.replace(path, &rep).to_string()
}

/// Use native path module to extract filename and error out if not UTF8,
/// we expect this should work for either host or image paths, including on Windows.
/// However, spurious or colliding delimiters have to be cleaned prior to calling this.
fn extract_ordinary_filename(path: &str) -> Result<String,DYNERR> {
    let pbuf = PathBuf::from(path);
    let Some(os_fname) = pbuf.file_name() else {
        log::error!("could not extract filename from {}",path);
        return Err(Box::new(CommandError::FileNotFound));
    };
    let Some(fname) = os_fname.to_str() else {
        return Err(Box::new(CommandError::UnsupportedFormat));
    };
    Ok(fname.to_string())
}

/// Parse a path that starts with a disk image and ends with a path inside the disk image.
/// Returns a tuple with (path_to_disk_image, path_inside_disk_image), where the second part can be an empty string.
/// The leading `/` is removed from the inside path. Use `//` to require a match to a ProDOS volume.
/// Panics if `fused` does not match `dimg_patt`
fn parse_fused_path(fused: &str,dimg_patt: &Regex) -> Result<(String,String),DYNERR> {
    let mut locs = dimg_patt.capture_locations();
    dimg_patt.captures_read(&mut locs,fused);
    let (_,end) = locs.get(0).unwrap();
    match fused[end-1..end].as_ref() {
        "/" => Ok((fused[0..end-1].to_owned(),fused[end..].to_owned())),
        _ => Ok((fused.to_owned(),String::new()))
    }
}

/// Combine src_path and dst_path using logic that the user likely expects.
/// If there is 1 source and the destination is neither null nor a directory, use the destination as is.
/// Otherwise the source filename is joined to the destination path.
/// The CP/M user prefix will be adjusted based on `dst_is_img`.
fn finalize_destination_path(src_path: &str, dst_path: &str, src_count: usize, dst_is_dir: bool, dst_is_img: bool, cpm: bool) -> Result<String,DYNERR> {
    match (src_count,dst_is_dir,dst_path.is_empty()) {
        (1,false,false) => Ok(dst_path.to_owned()),
        _ => {
            let mut fname = src_path.to_string();
            if cpm {
                fname = swap_cpm_delimiter(&fname, &CPM_COLON, "_");
            }
            fname = extract_ordinary_filename(&fname)?;
            if cpm && dst_is_img {
                fname = swap_cpm_delimiter(&fname, &CPM_UNDER, ":");
            }
            match dst_is_img {
                true => match dst_path.len() {
                    0 => Ok(fname.to_owned()),
                    _ => Ok([dst_path,"/",fname.as_str()].concat())
                },
                false => match PathBuf::from(dst_path).join(fname).as_os_str().to_str() {
                    Some(s) => Ok(s.to_owned()),
                    None => Err(Box::new(CommandError::UnsupportedFormat))
                }
            }
        }
    }
}

/// Produce a partial destination path that is appropriate for insertion into a file image.
/// Sometimes the destination filename is actually in the source path. This will handle that logic.
/// If the source path is used, only the filename is taken, the CP/M user prefix is normalized, and
/// up to one filename extension is removed if it appears in `strip`.
/// Anything matching a CiderPress suffix is also removed.
/// If the destination path is used it is taken as is (we will return an error later if it is wrong).
fn create_fimg_path(src_path: &str, dst_path: &str, src_count: usize, dst_is_dir: bool, strip: Vec<&str>, cpm: bool) -> Result<String,DYNERR> {
    match (src_count, dst_is_dir, dst_path.is_empty()) {
        (1,false,false) => Ok(dst_path.to_string()),
        _ => {
            let mut ans = src_path.to_string();
            if cpm {
                ans = swap_cpm_delimiter(&ans, &CPM_COLON, "_");
            }
            ans = extract_ordinary_filename(&ans)?;
            if cpm {
                ans = swap_cpm_delimiter(&ans, &CPM_UNDER, ":");
            }
            let l = ans.len();
            for x in strip {
                if l > x.len() && ans.to_lowercase().ends_with(x) {
                    ans.truncate(l-x.len());
                    break;
                }
            }
            if ans.len() > 7 && CIDERPRESS_SUFFIX.is_match(&ans) {
                ans.truncate(l-7)
            }
            Ok(ans)
        }
    }
}

/// Pack up data into a file image after transforming to the emulated system's format.
/// The input slice may be fully parsed for identification purposes.
fn smart_pack(fimg: &mut FileImage, dat: &[u8], load_addr0: Option<usize>, cp_suffix: Option<String>) -> STDRESULT {
    let load_addr = match (load_addr0,&cp_suffix) {
        (Some(addr),_) => Some(addr),
        (None,Some(s)) => Some(parse_cp_suffix(&s).1),
        _ => None
    };
    let typ = match &cp_suffix {
        Some(s) => Some(parse_cp_suffix(&s).0),
        None => None
    };
    log::debug!("load={}, type={}",load_addr.unwrap_or(0),typ.unwrap_or(0));
    match str::from_utf8(dat) {
        Ok(program) => {
            if is_lang(tree_sitter_applesoft::LANGUAGE.into(),program) {
                log::info!("detected Applesoft");
                let start_addr = match load_addr {
                    Some(addr) => u16::try_from(addr)?,
                    None => 2049
                };
                let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                let tok = tokenizer.tokenize(&program,start_addr)?;
                fimg.pack_tok(&tok,super::ItemType::ApplesoftTokens,None)?;
            } else if is_lang(tree_sitter_integerbasic::LANGUAGE.into(), program) {
                log::info!("detected Integer BASIC");
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let tok = tokenizer.tokenize(program.to_string())?;
                fimg.pack_tok(&tok,super::ItemType::IntegerTokens,None)?;
            } else if is_lang(tree_sitter_merlin6502::LANGUAGE.into(), program) {
                log::info!("detected Merlin");
                let mut tokenizer = merlin::tokenizer::Tokenizer::new();
                let tok = tokenizer.tokenize(program.to_string())?;
                fimg.pack_raw(&tok)?;
            } else {
                // this will take care of either records, or the case where
                // the data is already a file image
                fimg.pack(&dat,load_addr)?;
            }
        },
        Err(_) => {
            fimg.pack(&dat,load_addr)?;
        }
    };
    // if there was a CiderPress suffix and this is an Apple FS override the file type
    if let Some(t) = typ {
        match fimg.file_system.as_str() {
            "prodos" => {
                fimg.fs_type = vec![t];
            },
            "a2 dos" => {
                match prodos::types::FileType::from_u8(t) {
                    Some(prodos::types::FileType::Text) => fimg.fs_type = vec![dos3x::types::FileType::Text as u8],
                    Some(prodos::types::FileType::ApplesoftCode) => fimg.fs_type = vec![dos3x::types::FileType::Applesoft as u8],
                    Some(prodos::types::FileType::IntegerCode) => fimg.fs_type = vec![dos3x::types::FileType::Integer as u8],
                    _ => fimg.fs_type = vec![dos3x::types::FileType::Binary as u8]
                }
            },
            _ => {}
        }
    }
    Ok(())
}

/// Unpack a file image and possibly transform the data to a string that is readable and invertible on the host system.
/// In this direction there is a chance file system hints can be used for identification, while in some
/// cases it is still necessary to parse the whole slice.
/// The returned tuple is the `UnpackedData` and the updated destination path (suffix might be added).
fn smart_unpack(fimg: &FileImage,dst_path: &str,add_suffix: bool) -> Result<(UnpackedData,String),DYNERR> {
    // Coerce DOS types to ProDOS types so we can handle all at once via packing trait.
    // For pascal, FAT, and CP/M we only have the generic unpacking.
    let maybe_file_type = match fimg.file_system.as_str() {
        "prodos" => prodos::Packer::get_prodos_type(fimg),
        "a2 dos" => match dos3x::Packer::get_dos3x_type(fimg) {
            Some(dos3x::types::FileType::Applesoft) => Some(prodos::types::FileType::ApplesoftCode),
            Some(dos3x::types::FileType::Integer) => Some(prodos::types::FileType::IntegerCode),
            Some(dos3x::types::FileType::Text) => Some(prodos::types::FileType::Text),
            _ => None
        },
        _ => None
    };
    let cp_suffix = match maybe_file_type {
        Some(_) => ["#".to_string(),hex::encode(&fimg.fs_type),hex::encode(&fimg.aux.iter().rev().map(|x| *x).collect::<Vec<u8>>())].concat(),
        None => "".to_string()
    };
    // closure to add a suffix to destination path
    let update_dst_path = |s: &str| -> String {
        match dst_path.to_lowercase().ends_with(s) || !add_suffix {
            true => dst_path.to_string(),
            false => [dst_path,s].concat()
        }
    };
    match maybe_file_type {
        Some(prodos::types::FileType::ApplesoftCode) => {
            log::info!("detected Applesoft");
            let toks = fimg.unpack_tok()?;
            let tokenizer = applesoft::tokenizer::Tokenizer::new();
            return Ok((UnpackedData::Text(tokenizer.detokenize(&toks)?),update_dst_path(".abas")))
        },
        Some(prodos::types::FileType::IntegerCode) => {
            log::info!("detected Integer BASIC");
            let toks = fimg.unpack_tok()?;
            let tokenizer = integer::tokenizer::Tokenizer::new();
            return Ok((UnpackedData::Text(tokenizer.detokenize(&toks)?),update_dst_path(".ibas")))
        },
        Some(prodos::types::FileType::Text) => {
            // some processing to see if this is Merlin
            let merlin_code = fimg.unpack_raw(true)?;
            let mut tokenizer = merlin::tokenizer::Tokenizer::new();
            tokenizer.set_err_log(false);
            if let Ok(src) = tokenizer.detokenize(&merlin_code) {
                if is_lang(tree_sitter_merlin6502::LANGUAGE.into(), &src) {
                    log::info!("detected Merlin");
                    return Ok((UnpackedData::Text(src),update_dst_path(".S")));
                }
            }
        },
        _ => {}
    }
    match fimg.unpack() {
        Ok(UnpackedData::Text(t)) => Ok((UnpackedData::Text(t),update_dst_path(".txt"))),
        Ok(ud) => Ok((ud,update_dst_path(&cp_suffix))),
        Err(e) => Err(e)
    }
}

fn gather(src: Vec<String>,dst: &Destination,dst_is_dir: bool,dimg_patt: &Regex,cmd: &clap::ArgMatches) -> Result<Vec<Source>,DYNERR> {
    let mut ans = Vec::new();
    let fmt = super::get_fmt(cmd)?;
    let load_addr: Option<usize> = match cmd.get_one::<String>("addr") {
        Some(a) => Some(usize::from_str(a)?),
        _ => None
    };
    let add_suffix = cmd.get_flag("suffix");
    let src_count = src.len();

    for fused_path in src {

        match dimg_patt.is_match(&fused_path) {

            // we are gathering things from a disk image
            true => {
                let (path_to,path_in) = parse_fused_path(&fused_path,dimg_patt)?;
                let mut src_disk = crate::create_fs_from_file(&path_to,fmt.as_ref())?;
                src_disk.get_img().change_method(Method::from_str(cmd.get_one::<String>("method").unwrap())?);
                let flat = match src_disk.stat() {
                    Ok(stat) if ["cpm","a2 dos","a2 pascal"].contains(&stat.fs_name.as_str()) => true,
                    _ => false
                };
                match src_disk.glob(&path_in,false) {
                    Ok(glob_matches) => {
                        if glob_matches.len() == 0 {
                            log::error!("no matches to source path {}",path_in);
                            return Err(Box::new(CommandError::FileNotFound));
                        }
                        for raw_match in glob_matches {
                            if flat && (raw_match.contains("/") || raw_match.contains("\\")) {
                                log::warn!("skipping {} due to path delimiter in filename",raw_match);
                                break;
                            }
                            // for flat FS the slash has to be put back in the fused path,
                            // as of this writing Source.fused_path is used only for logging
                            let m = match flat {
                                true => ["/",&raw_match].concat(),
                                false => raw_match.clone()
                            };
                            ans.push(Source {fimg: src_disk.get(&raw_match)?, fused_path: [path_to.as_str(),m.as_str()].concat()});
                        }
                    },
                    Err(_) => ans.push(Source {fimg: src_disk.get(&path_in)?, fused_path})
                }
            },

            // we are gathing things from the host
            false => {
                match (dst,std::fs::read(&fused_path)) {
                    (Destination::Host(_),_) => {
                        log::error!("refusing host-to-host copy");
                        return Err(Box::new(CommandError::InvalidCommand))
                    },
                    (Destination::Dimg(dst_disk,_,raw_dst_path),Ok(dat)) => {
                        let dummy = dst_disk.new_fimg(None,true,"dummy")?;
                        let cpm = dummy.file_system.as_str()=="cpm";
                        let strip = match dummy.file_system.as_str() {
                            "prodos" | "a2 dos" => vec![".json",".txt",".bas",".abas",".ibas"],
                            _ => vec![".json"]
                        };
                        let cp_suffix = match (add_suffix,CIDERPRESS_SUFFIX.is_match(&fused_path)) {
                            (true,true) => Some(fused_path[fused_path.len()-7..].to_string()),
                            _ => None
                        };
                        let dst_path = create_fimg_path(&fused_path,raw_dst_path,src_count,dst_is_dir,strip,cpm)?;
                        let mut fimg = dst_disk.new_fimg(None,true,&dst_path)?;
                        smart_pack(&mut fimg,&dat,load_addr,cp_suffix)?;
                        ans.push(Source {fimg,fused_path});
                    },
                    (_,Err(e)) => return Err(Box::new(e))
                }
            }
        }
    }
    Ok(ans)
}

pub fn ezcopy(cmd: &clap::ArgMatches) -> STDRESULT {

    // let rec_len = match cmd.get_one::<String>("len") {
    //     Some(s) => Some(usize::from_str(s)?),
    //     None => None
    // };

    // First stage, setup and gather sources

    let add_suffix = cmd.get_flag("suffix");
    let dimg_patt = Regex::new(r"(?i)\.(2mg|d13|dsk|do|dsk|ima|imd|img|nib|po|td0|woz)($|/)").expect("failed to parse regex");
    let mut path_list: Vec<String> = cmd.get_many::<String>("paths").expect("no paths").map(|x| x.to_owned()).collect();
    let fused = path_list.pop().unwrap();
    let fmt = super::get_fmt(cmd)?;
    let mut dst = match dimg_patt.is_match(&fused) {
        true => {
            let (path_to,path_inside) = parse_fused_path(&fused,&dimg_patt)?;
            let mut dimg = crate::create_fs_from_file(&path_to,fmt.as_ref())?;
            dimg.get_img().change_method(Method::from_str(cmd.get_one::<String>("method").unwrap())?);
            Destination::Dimg(dimg,path_to,path_inside)
        },
        false => {
            Destination::Host(fused.clone())
        }
    };
    let dst_is_dir = match &mut dst {
        Destination::Dimg(disk, path_to, path_inside) => {
            log::info!("image destination {}",path_to);
            match disk.catalog_to_vec(path_inside) {
                Ok(_) => true,
                Err(_) => false
            }
        },
        Destination::Host(target_path) => {
            log::info!("host destination {}",target_path);
            if PathBuf::from(target_path.as_str()).is_file() {
                log::error!("destination already exists as a file");
                return Err(Box::new(CommandError::InvalidCommand));
            }
            PathBuf::from(target_path.as_str()).is_dir()
        }
    };
    let mut src_list = gather(path_list,&dst,dst_is_dir,&dimg_patt,cmd)?;

    // Second stage, write to destination

    let src_count = src_list.len();
    for src in &mut src_list {
        let cpm = src.fimg.file_system.as_str() == "cpm";
        match &mut dst {
            Destination::Dimg(dst_disk, _, raw_dst_path) => {
                let dst_path = finalize_destination_path(&src.fimg.full_path, &raw_dst_path, src_count, dst_is_dir, true, cpm)?;
                log::info!("copy {} -> {}",src.fused_path,dst_path);
                dst_disk.put_at(&dst_path,&mut src.fimg)?;
            },
            Destination::Host(raw_dst_path) => {
                let dst_path = finalize_destination_path(&src.fimg.full_path, raw_dst_path, src_count, dst_is_dir, false, cpm)?;
                if cfg!(windows) {
                    let start = match DRIVE_PREFIX.is_match(&dst_path) {
                        true => 2,
                        false => 0
                    };
                    if dst_path[start..].contains(':') {
                        log::warn!("skipping {} due to colon",dst_path);
                        break;
                    }
                }
                if PathBuf::from(dst_path.as_str()).is_file() {
                    log::error!("destination {} already exists as a file",dst_path);
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                log::info!("copy {} -> {}",src.fused_path,dst_path);
                match smart_unpack(&src.fimg,&dst_path,add_suffix) {
                    Ok((UnpackedData::Binary(dat),final_dst)) => std::fs::write(&final_dst,&dat).expect("host file system error"),
                    Ok((UnpackedData::Text(s),final_dst)) => std::fs::write(&final_dst,s.as_bytes()).expect("host file system error"),
                    Ok((UnpackedData::Records(r),final_dst)) => {
                        let rec_str = r.to_json(None);
                        std::fs::write(&final_dst,rec_str.as_bytes()).expect("host file system error")
                    },
                    _ => {
                        log::info!("not unpacking {}",dst_path);
                        let fimg_str = src.fimg.to_json(Some(2));
                        std::fs::write(&[&dst_path,".json"].concat(),fimg_str.as_bytes()).expect("host file system error")
                    }
                }
            }
        }
    }
    match &mut dst {
        Destination::Dimg(dst_disk,dimg_path,_) => crate::save_img(dst_disk,&dimg_path),
        Destination::Host(_) => Ok(())
    }
}
