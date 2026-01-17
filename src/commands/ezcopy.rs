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
use std::path::PathBuf;

use super::CommandError;
use crate::fs::{DiskFS,FileImage,UnpackedData,dos3x,prodos};
use crate::img::tracks::Method;
use crate::{DYNERR,STDRESULT};
use crate::lang::{applesoft,integer,merlin,is_lang};

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

/// Use native path module to extract filename and error out if not UTF8,
/// we expect this should work for either host or image paths, including on Windows.
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

/// Parse a path the starts with a disk image and ends with a path inside the disk image.
/// Returns a tuple with (path_to_disk_image, path_inside_disk_image), where the second part can be an empty string.
/// Panics if `fused` does not match `dimg_patt`
fn parse_fused_path(fused: &str,dimg_patt: &Regex) -> Result<(String,String),DYNERR> {
    let mut locs = dimg_patt.capture_locations();
    dimg_patt.captures_read(&mut locs,fused);
    let (_,end) = locs.get(0).unwrap();
    let path_to_dimg = fused[0..end].to_owned();
    if fused.len() > end && &fused[end..end+1] != "/" {
        log::error!("{} is not formatted correctly",fused);
        return Err(Box::new(CommandError::InvalidCommand));
    }
    // We will always throw out the leading `/` from the path inside.
    // If ProDOS users want to specify the volume name (perhaps as a check) they can use `//`.
    let path_in_dimg = match fused.len() - end {
        0 => String::new(),
        1 => String::new(),
        _ => fused[end+1..].to_owned()
    };
    Ok((path_to_dimg,path_in_dimg))
}

/// Combine src_path and dst_path using logic that the user likely expects.
/// If there is 1 source and the destination is not null, and not an existing directory,
/// then the destination is used as is.  Otherwise the source filename is joined to the destination path.
fn revise_destination_path(src_path: &str, dst_path: &str, src_count: usize, dst_dir_exists: bool, dst_is_img: bool) -> Result<String,DYNERR> {
    match (src_count,dst_dir_exists,dst_path.is_empty()) {
        (1,false,false) => Ok(dst_path.to_owned()),
        _ => {
            let fname = extract_ordinary_filename(src_path)?;
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

/// In some cases we are given a new filename in the destination path, and that has to be detected
/// at the time when a file image is created, i.e., during the gather step, because when the
/// file image is created an invalid filename produces an error.  We also will automatically
/// strip any file extensions that are specified in `strip` which can help avoid length violations.
/// It is OK and necessary to later call `revise_destination_path` to handle an implied join.
fn revise_fimg_path(src_path: &str, dst_path: &str, src_count: usize, dst_dir_exists: bool, strip: Vec<&str>) -> String {
    let mut host_path = match (src_count, dst_dir_exists, dst_path.is_empty()) {
        (1,false,false) => dst_path.to_string(),
        _ => src_path.to_string()
    };
    let l = host_path.len();
    for x in strip {
        if l > x.len() && host_path.to_lowercase().ends_with(x) {
            host_path.truncate(l-x.len());
            return host_path;
        }
    }
    host_path
}

/// Pack up data into a file image after transforming to the emulated system's format.
/// The input slice may be fully parsed for identification purposes.
fn smart_pack(fimg: &mut FileImage, dat: &[u8], load_addr: Option<usize>) -> STDRESULT {
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
                fimg.pack_tok(&tok,super::ItemType::ApplesoftTokens,None)
            } else if is_lang(tree_sitter_integerbasic::LANGUAGE.into(), program) {
                log::info!("detected Integer BASIC");
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let tok = tokenizer.tokenize(program.to_string())?;
                fimg.pack_tok(&tok,super::ItemType::IntegerTokens,None)
            } else if is_lang(tree_sitter_merlin6502::LANGUAGE.into(), program) {
                log::info!("detected Merlin");
                let mut tokenizer = merlin::tokenizer::Tokenizer::new();
                let tok = tokenizer.tokenize(program.to_string())?;
                fimg.pack_raw(&tok)
            } else {
                // this will take care of either records, or the case where
                // the data is already a file image
                fimg.pack(&dat,load_addr)
            }
        },
        Err(_) => {
            fimg.pack(&dat,load_addr)
        }
    }
}

/// Unpack a file image and possibly transform the data to a string that is readable and invertible on the host system.
/// In this direction there is a chance file system hints can be used for identification, while in some
/// cases it is still necessary to parse the whole slice.
fn smart_unpack(fimg: &FileImage) -> Result<UnpackedData,DYNERR> {
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
    match maybe_file_type {
        Some(prodos::types::FileType::ApplesoftCode) => {
            log::info!("detected Applesoft");
            let toks = fimg.unpack_tok()?;
            let tokenizer = applesoft::tokenizer::Tokenizer::new();
            Ok(UnpackedData::Text(tokenizer.detokenize(&toks)?))
        },
        Some(prodos::types::FileType::IntegerCode) => {
            log::info!("detected Integer BASIC");
            let toks = fimg.unpack_tok()?;
            let tokenizer = integer::tokenizer::Tokenizer::new();
            Ok(UnpackedData::Text(tokenizer.detokenize(&toks)?))
        },
        Some(prodos::types::FileType::Text) => {
            // some processing to see if this is Merlin
            let merlin_code = fimg.unpack_raw(true)?;
            let mut tokenizer = merlin::tokenizer::Tokenizer::new();
            tokenizer.set_err_log(false);
            match tokenizer.detokenize(&merlin_code) {
                Ok(src) => {
                    match is_lang(tree_sitter_merlin6502::LANGUAGE.into(), &src) {
                        true => {
                            log::info!("detected Merlin");
                            Ok(UnpackedData::Text(src))
                        },
                        false => fimg.unpack()
                    }
                },
                Err(_) => fimg.unpack()
            }
        },
        _ => fimg.unpack()
    }
}

fn gather(src: Vec<String>,dst: &Destination,dst_dir_exists: bool,dimg_patt: &Regex,cmd: &clap::ArgMatches) -> Result<Vec<Source>,DYNERR> {
    let mut ans = Vec::new();
    let fmt = super::get_fmt(cmd)?;
    let load_addr: Option<usize> = match cmd.get_one::<String>("addr") {
        Some(a) => Some(usize::from_str(a)?),
        _ => None
    };
    let src_count = src.len();

    for fused_path in src {

        match dimg_patt.is_match(&fused_path) {
            true => {
                let (path_to,path_in) = parse_fused_path(&fused_path,dimg_patt)?;
                let mut src_disk = crate::create_fs_from_file(&path_to,fmt.as_ref())?;
                src_disk.get_img().change_method(Method::from_str(cmd.get_one::<String>("method").unwrap())?);
                match src_disk.glob(&path_in,false) {
                    Ok(vlist) => {
                        if vlist.len() == 0 {
                            log::error!("no matches to source path {}",path_in);
                            return Err(Box::new(CommandError::FileNotFound));
                        }
                        for v in vlist {
                            ans.push(Source {fimg: src_disk.get(&v)?, fused_path: [path_to.as_str(),v.as_str()].concat()});
                        }
                    },
                    Err(_) => ans.push(Source {fimg: src_disk.get(&path_in)?, fused_path})
                }
            },
            false => {
                match (dst,std::fs::read(&fused_path)) {
                    (Destination::Host(_),_) => {
                        log::error!("refusing host-to-host copy");
                        return Err(Box::new(CommandError::InvalidCommand))
                    },
                    (Destination::Dimg(dst_disk,_,raw_dst_path),Ok(dat)) => {
                        let fname = extract_ordinary_filename(&fused_path)?;
                        let dummy = dst_disk.new_fimg(None,true,"dummy")?;
                        let strip = match dummy.file_system.as_str() {
                            "prodos" | "a2 dos" => vec![".json",".txt",".bas",".abas",".ibas"],
                            _ => vec![".json"]
                        };
                        let dst_path = revise_fimg_path(&fname,raw_dst_path,src_count,dst_dir_exists,strip);
                        let mut fimg = dst_disk.new_fimg(None,true,&dst_path)?;
                        smart_pack(&mut fimg,&dat,load_addr)?;
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

    let dimg_patt = Regex::new(r"(?i)\.(2mg|d13|dsk|do|dsk|ima|imd|img|nib|po|td0|woz)").expect("failed to parse regex");
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
    let dst_dir_exists = match &mut dst {
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
    let mut src_list = gather(path_list,&dst,dst_dir_exists,&dimg_patt,cmd)?;

    // Second stage, write to destination

    let src_count = src_list.len();
    for src in &mut src_list {
        match &mut dst {
            Destination::Dimg(dst_disk, _, raw_dst_path) => {
                let dst_path = revise_destination_path(&src.fimg.full_path, &raw_dst_path, src_count, dst_dir_exists, true)?;
                log::info!("copy {} -> {}",src.fused_path,dst_path);
                dst_disk.put_at(&dst_path,&mut src.fimg)?;
            },
            Destination::Host(raw_dst_path) => {
                let dst_path = revise_destination_path(&src.fimg.full_path, raw_dst_path, src_count, dst_dir_exists, false)?;
                if PathBuf::from(dst_path.as_str()).is_file() {
                    log::error!("destination {} already exists as a file",dst_path);
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                log::info!("copy {} -> {}",src.fused_path,dst_path);
                match smart_unpack(&src.fimg)? {
                    UnpackedData::Binary(dat) => std::fs::write(&dst_path,&dat).expect("host file system error"),
                    UnpackedData::Text(s) => std::fs::write(&dst_path,s.as_bytes()).expect("host file system error"),
                    UnpackedData::Records(r) => {
                        let rec_str = r.to_json(None);
                        std::fs::write(&dst_path,rec_str.as_bytes()).expect("host file system error")
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
