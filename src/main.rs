//! # Command Line Interface
//! 
//! This is a standalone main module.
//! All sub-modules are in the library crate.

use clap::{arg,Command};
use env_logger;
use std::io::{Read,Write};
use std::str::FromStr;
use std::error::Error;
#[cfg(windows)]
use colored;
use a2kit::disk_base::*;
use a2kit::dos33;
use a2kit::prodos;
use a2kit::walker;
use a2kit::applesoft;
use a2kit::integer;
use a2kit::merlin;
use a2kit::img_po;
use a2kit::img_do;
use a2kit::img_woz1;
use a2kit::img_woz2;

const RCH: &str = "unreachable was reached";

fn main() -> Result<(),Box<dyn std::error::Error>>
{
    env_logger::init();
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
    let long_help =
"a2kit is always invoked with exactly one of several subcommands.
The subcommands are generally designed to function as nodes in a pipeline.
PowerShell users may need to wrap the pipeline in a native shell.
Set RUST_LOG environment variable to control logging level.

Examples:
---------
create DOS image:      `a2kit mkdsk -v 254 -t woz1 > myimg.woz`
create ProDOS image:   `a2kit mkdsk -v disk.new -t po > myimg.po`
Language line entry:   `a2kit verify -t atxt`
Language file check:   `a2kit get -f myprog.bas | a2kit verify -t atxt`
Tokenize to file:      `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt > prog.atok
Tokenize to image:     `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt \\
                           | a2kit put -f prog -t atok -d myimg.dsk`
Detokenize from image: `a2kit get -f prog -t atok -d myimg.dsk | a2kit detokenize -t atok";

    let matches = Command::new("a2kit")
        .about("Manipulates Apple II files and disk images, with language comprehension.")
    .after_long_help(long_help)
    .subcommand(Command::new("mkdsk")
        .arg(arg!(-v --volume <VOLUME> "volume name or number"))
        .arg(arg!(-t --type <TYPE> "type of disk image to create").possible_values(["do","po","woz1","woz2"]))
        .arg(arg!(-k --kind <SIZE> "kind of disk").possible_values([
            "5.25in",
            "3.5in",
            "hdmax"]).required(false)
            .default_value("5.25in"))
        .about("write a blank disk image to stdout"))
    .subcommand(Command::new("reorder")
        .arg(arg!(-d --dimg <PATH> "path to disk image"))
        .about("Put a disk image into its natural order"))
    .subcommand(Command::new("reimage")
        .arg(arg!(-d --dimg <PATH> "path to old disk image"))
        .arg(arg!(-t --type <TYPE> "type of new disk image").possible_values(["do","po","woz1","woz2"]))
        .about("Transform an image into another type of image"))
    .subcommand(Command::new("mkdir")
        .arg(arg!(-f --file <PATH> "path inside disk image of new directory"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("create a new directory inside a disk image"))
    .subcommand(Command::new("delete")
        .arg(arg!(-f --file <PATH> "path inside disk image to delete"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("delete a file or directory inside a disk image"))
    .subcommand(Command::new("lock")
        .arg(arg!(-f --file <PATH> "path inside disk image to lock"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("write protect a file or directory inside a disk image"))
    .subcommand(Command::new("unlock")
        .arg(arg!(-f --file <PATH> "path inside disk image to unlock"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("remove write protection from a file or directory inside a disk image"))
    .subcommand(Command::new("rename")
        .arg(arg!(-f --file <PATH> "path inside disk image to rename"))
        .arg(arg!(-n --name <NAME> "new name"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("rename a file or directory inside a disk image"))
    .subcommand(Command::new("retype")
        .arg(arg!(-f --file <PATH> "path inside disk image to retype"))
        .arg(arg!(-t --type <TYPE> "file system type, code or mnemonic"))
        .arg(arg!(-a --aux <AUX> "file system auxiliary metadata"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("change file type inside a disk image"))
    .subcommand(Command::new("verify")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt","mtxt"]))
        .about("read from stdin and error check"))
    .subcommand(Command::new("get")
        .arg(arg!(-f --file <PATH> "source path or chunk index, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","mtok","bin","txt","raw","chunk","track","raw_track","rec","any"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .arg(arg!(-l --len <LENGTH> "length of record in DOS 3.3 random access text file").required(false))
        .about("read from local or disk image, write to stdout"))
    .subcommand(Command::new("put")
        .arg(arg!(-f --file <PATH> "destination path or chunk index, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","mtok","bin","txt","raw","chunk","rec","any"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .arg(arg!(-a --addr <ADDRESS> "address of binary file").required(false))
        .about("read from stdin, write to local or disk image"))
    .subcommand(Command::new("catalog")
        .arg(arg!(-f --file <PATH> "path of directory inside disk image").required(false))
        .arg(arg!(-d --dimg <PATH> "path to disk image"))
        .about("write disk image catalog to stdout"))
    .subcommand(Command::new("tokenize")
        .arg(arg!(-a --addr <ADDRESS> "address of tokenized code (Applesoft only)").required(false))
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt","mtxt"]))
        .about("read from stdin, tokenize, write to stdout"))
    .subcommand(Command::new("detokenize")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atok","itok","mtok"]))
        .about("read from stdin, detokenize, write to stdout"))
    .get_matches();
    
    // Put a disk image into its natural ordering

    if let Some(cmd) = matches.subcommand_matches("reorder") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        if let Some((img,disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            if img.is_do_or_po() {
                std::io::stdout().write_all(&disk.to_img()).expect("write to stdout failed");
                return Ok(());
            } else {
                eprintln!("cannot reorder this type of disk image");
                return Err(Box::new(CommandError::UnknownFormat));
            }
        }
        return Err(Box::new(CommandError::UnknownFormat));
    }

    // Transform an image into another type of image

    if let Some(cmd) = matches.subcommand_matches("reimage") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        // we need to get the file system also in order to determine the ordering
        if let Some((img,_disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            let maybe_bytes = match DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap() {
                DiskImageType::DO => img.to_do(),
                DiskImageType::PO => img.to_po(),
                _ => panic!("not supported")
            };
            if let Ok(bytestream) = maybe_bytes {
                std::io::stdout().write_all(&bytestream).expect("write to stdout failed");
                return Ok(());
            }
        }
        return Err(Box::new(CommandError::UnknownFormat));
    }    

    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        let dos_vol = u8::from_str(cmd.value_of("volume").expect(RCH));
        let kind = cmd.value_of("kind").expect(RCH);
        let (blocks,floppy) = match kind {
            "5.25in" => (280,true),
            "3.5in" => (1600,true),
            "hdmax" => (65535,false),
            _ => (280,true)
        };
        match u8::from_str(cmd.value_of("volume").expect(RCH)) {
            Ok(vol) if vol<1 || vol>254 => {
                eprintln!("volume must be from 1 to 254");
                return Err(Box::new(CommandError::OutOfRange));
            },
            _ => {}
        }
        let mut bytestream: Option<Vec<u8>> = None;
        match DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap() {
            DiskImageType::DO => match dos_vol {
                Ok(vol) => {
                    // DOS ordered DOS disk
                    let mut disk = dos33::Disk::new();
                    disk.format(vol,true,17);
                    bytestream = Some(disk.to_img());
                },
                _ => {
                    // DOS ordered ProDOS disk
                    let mut disk = prodos::Disk::new(blocks);
                    disk.format(&cmd.value_of("volume").expect(RCH).to_string(),floppy,None);
                    if let Some(img) = img_po::PO::from_bytes(&disk.to_img()) {
                        match img.to_do() {
                            Ok(bytes) => bytestream = Some(bytes),
                            Err(e) => return Err(e)
                        }
                    }
                }
            },
            DiskImageType::PO => match dos_vol {
                Ok(vol) => {
                    // ProDOS ordered DOS disk
                    let mut disk = dos33::Disk::new();
                    disk.format(vol,true,17);
                    if let Some(img) = img_do::DO::from_bytes(&disk.to_img()) {
                        match img.to_po() {
                            Ok(bytes) => bytestream = Some(bytes),
                            Err(e) => return Err(e)
                        }
                    }
                },
                _ => {
                    // ProDOS ordered ProDOS disk
                    let mut disk = prodos::Disk::new(blocks);
                    disk.format(&cmd.value_of("volume").expect(RCH).to_string(),floppy,None);
                    bytestream = Some(disk.to_img());
                }
            },
            DiskImageType::WOZ => match dos_vol {
                Ok(vol) => {
                    // DOS disk
                    match DiskKind::from_str(kind) {
                        Ok(DiskKind::A2_525_16) => {
                            let mut disk = dos33::Disk::new();
                            let mut woz = img_woz1::Woz1::create(DiskKind::A2_525_16);
                            disk.format(vol,true,17);
                            if let Ok(()) = woz.update_from_do(&disk.to_img()) {
                                bytestream = Some(woz.to_bytes());
                            }
                        },
                        Ok(_) => {
                            eprintln!("Only 5.25 inch disks are supported with WOZ images");
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        },
                        Err(e) => {
                            return Err(Box::new(e))
                        }
                    }
                },
                _ => {
                    // ProDOS disk
                    match DiskKind::from_str(kind) {
                        Ok(DiskKind::A2_525_16) => {
                            let mut disk = prodos::Disk::new(blocks);
                            let mut woz = img_woz1::Woz1::create(DiskKind::A2_525_16);
                            disk.format(&cmd.value_of("volume").expect(RCH).to_string(),floppy,None);
                            if let Ok(()) = woz.update_from_po(&disk.to_img()) {
                                bytestream = Some(woz.to_bytes());
                            }
                        },
                        Ok(_) => {
                            eprintln!("Only 5.25 inch disks are supported with WOZ images");
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        },
                        Err(e) => {
                            return Err(Box::new(e))
                        }
                    }
                }
            },
            DiskImageType::WOZ2 => match dos_vol {
                Ok(vol) => {
                    // DOS disk
                    match DiskKind::from_str(kind) {
                        Ok(DiskKind::A2_525_16) => {
                            let mut disk = dos33::Disk::new();
                            let mut woz = img_woz2::Woz2::create(DiskKind::A2_525_16);
                            disk.format(vol,true,17);
                            if let Ok(()) = woz.update_from_do(&disk.to_img()) {
                                bytestream = Some(woz.to_bytes());
                            }
                        },
                        Ok(_) => {
                            eprintln!("Only 5.25 inch disks are supported with WOZ images");
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        },
                        Err(e) => {
                            return Err(Box::new(e))
                        }
                    }
                },
                _ => {
                    // ProDOS disk
                    match DiskKind::from_str(kind) {
                        Ok(DiskKind::A2_525_16) => {
                            let mut disk = prodos::Disk::new(blocks);
                            let mut woz = img_woz2::Woz2::create(DiskKind::A2_525_16);
                            disk.format(&cmd.value_of("volume").expect(RCH).to_string(),floppy,None);
                            if let Ok(()) = woz.update_from_po(&disk.to_img()) {
                                bytestream = Some(woz.to_bytes());
                            }
                        },
                        Ok(_) => {
                            eprintln!("Only 5.25 inch disks are supported with WOZ images");
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        },
                        Err(e) => {
                            return Err(Box::new(e))
                        }
                    }
                }
            }
        };
        if let Some(buf) = bytestream {
            eprintln!("writing {} bytes",buf.len());
            std::io::stdout().write_all(&buf).expect("write to stdout failed");
            return Ok(());
        }
        return Err(Box::new(CommandError::UnsupportedItemType));
    }

    // Catalog a disk image
    if let Some(cmd) = matches.subcommand_matches("catalog") {
        let path_in_img = match cmd.value_of("file") {
            Some(path) => path,
            _ => "/"
        };
        if let Some(path_to_img) = cmd.value_of("dimg") {
            let disk = a2kit::create_disk_from_file(path_to_img);
            return disk.catalog_to_stdout(&path_in_img);
        }
        panic!("{}",RCH);
    }
    
    // Verify

    if let Some(cmd) = matches.subcommand_matches("verify") {
        if let Ok(typ) = ItemType::from_str(cmd.value_of("type").expect(RCH)) {
            let res = match typ
            {
                ItemType::ApplesoftText => walker::verify_stdin(tree_sitter_applesoft::language(),"]"),
                ItemType::IntegerText => walker::verify_stdin(tree_sitter_integerbasic::language(),">"),
                ItemType::MerlinText => walker::verify_stdin(tree_sitter_merlin6502::language(),":"),
                _ => return Err(Box::new(CommandError::UnsupportedItemType))
            };
            match res {
                Ok(res) => {
                    println!("{}",res.0);
                    eprintln!("{}",res.1);
                    return Ok(());
                },
                Err(e) => {
                    return Err(Box::new(e));
                }
            }

        }
    }

    // Tokenize BASIC or Encode Merlin

    if let Some(cmd) = matches.subcommand_matches("tokenize") {
        if atty::is(atty::Stream::Stdin) {
            eprintln!("line entry is not supported for `tokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let addr_opt = cmd.value_of("addr");
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                if addr_opt==None {
                    eprintln!("address needed to tokenize Applesoft");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                if let Ok(addr) = u16::from_str_radix(addr_opt.expect(RCH),10) {
                    let mut program = String::new();
                    std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
                    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                    let object = tokenizer.tokenize(&program,addr);
                    if atty::is(atty::Stream::Stdout) {
                        a2kit::display_chunk(addr,&object);
                    } else {
                        std::io::stdout().write_all(&object).expect("could not write output stream");
                    }
                    return Ok(());
                }
                Err(Box::new(CommandError::OutOfRange))
            },
            Ok(ItemType::IntegerText) => {
                if let Some(_addr) = addr_opt {
                    eprintln!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut program = String::new();
                std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program));
                if atty::is(atty::Stream::Stdout) {
                    a2kit::display_chunk(0,&object);
                } else {
                    std::io::stdout().write_all(&object).expect("could not write output stream");
                }
                Ok(())
            },
            Ok(ItemType::MerlinText) => {
                if let Some(_addr) = addr_opt {
                    eprintln!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut program = String::new();
                std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
                let mut tokenizer = merlin::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program));
                if atty::is(atty::Stream::Stdout) {
                    a2kit::display_chunk(0,&object);
                } else {
                    std::io::stdout().write_all(&object).expect("could not write output stream");
                }
                Ok(())
            },
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }

    // Detokenize BASIC or decode Merlin

    if let Some(cmd) = matches.subcommand_matches("detokenize") {
        if atty::is(atty::Stream::Stdin) {
            eprintln!("line entry is not supported for `detokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        return match typ
        {
            Ok(ItemType::ApplesoftTokens) => {
                let mut tok: Vec<u8> = Vec::new();
                std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
                let tokenizer = applesoft::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok);
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::IntegerTokens) => {
                let mut tok: Vec<u8> = Vec::new();
                std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
                let tokenizer = integer::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok);
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::MerlinTokens) => {
                let mut tok: Vec<u8> = Vec::new();
                std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
                let tokenizer = merlin::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok);
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }

    // Create directory inside disk image
    if let Some(cmd) = matches.subcommand_matches("mkdir") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.create(&path_in_img) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.delete(&path_in_img) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Lock a file or directory
    if let Some(cmd) = matches.subcommand_matches("lock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.lock(&path_in_img) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Unlock a file or directory
    if let Some(cmd) = matches.subcommand_matches("unlock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.unlock(&path_in_img) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Rename a file or directory
    if let Some(cmd) = matches.subcommand_matches("rename") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let name = String::from(cmd.value_of("name").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.rename(&path_in_img,&name) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Retype a file
    if let Some(cmd) = matches.subcommand_matches("retype") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let typ = String::from(cmd.value_of("type").expect(RCH));
        let aux = String::from(cmd.value_of("aux").expect(RCH));
        if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(&path_to_img) {
            return match disk.retype(&path_in_img,&typ,&aux) {
                Ok(()) => a2kit::update_img_and_save(&mut img,&disk,&path_to_img),
                Err(e) => Err(e)
            };
        } else {
            return Err(Box::new(CommandError::UnknownFormat));
        }
    }

    // Put file inside disk image, or save to local
    if let Some(cmd) = matches.subcommand_matches("put") {
        if atty::is(atty::Stream::Stdin) {
            eprintln!("cannot use `put` with console input, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        if !atty::is(atty::Stream::Stdout) {
            eprintln!("output is redirected, but `put` must end the pipeline");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let dest_path = String::from(cmd.value_of("file").expect(RCH));
        let maybe_typ = cmd.value_of("type");
        let maybe_img = cmd.value_of("dimg");
        let mut file_data = Vec::new();
        std::io::stdin().read_to_end(&mut file_data).expect("failed to read input stream");

        match (maybe_typ,maybe_img) {
            
            // we are writing to a disk image
            (Some(typ_str),Some(img_path)) => {
                let typ = ItemType::from_str(typ_str);
                let load_address: u16 = match (cmd.value_of("addr"),&typ) {
                    (Some(a),_) => u16::from_str(a).expect("bad address"),
                    (_ ,Ok(ItemType::Binary)) => {
                        eprintln!("binary file requires an address");
                        return Err(Box::new(CommandError::InvalidCommand));
                    },
                    _ => 768 as u16
                };
                if let Some((mut img,mut disk)) = a2kit::create_img_and_disk_from_file(img_path) {
                    let result = match typ {
                        Ok(ItemType::ApplesoftTokens) => disk.save(&dest_path,&file_data,ItemType::ApplesoftTokens,None),
                        Ok(ItemType::IntegerTokens) => disk.save(&dest_path,&file_data,ItemType::IntegerTokens,None),
                        Ok(ItemType::MerlinTokens) => disk.write_text(&dest_path,&file_data),
                        Ok(ItemType::Binary) => disk.bsave(&dest_path,&file_data,load_address,None),
                        Ok(ItemType::Text) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match disk.encode_text(s) {
                                Ok(encoded) => disk.write_text(&dest_path,&encoded),
                                Err(e) => Err(e)
                            },
                            _ => {
                                eprintln!("could not encode data as UTF8");
                                return Err(Box::new(CommandError::UnknownFormat));
                            }
                        },
                        Ok(ItemType::Raw) => disk.write_text(&dest_path,&file_data),
                        Ok(ItemType::Chunk) => disk.write_chunk(&dest_path,&file_data),
                        Ok(ItemType::Records) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match Records::from_json(s) {
                                Ok(recs) => disk.write_records(&dest_path,&recs),
                                Err(e) => Err(e)
                            },
                            _ => {
                                eprintln!("could not encode data as UTF8");
                                return Err(Box::new(CommandError::UnknownFormat));
                            }
                        },
                        Ok(ItemType::SparseData) => match std::str::from_utf8(&file_data) {
                            Ok(s) => match SparseData::from_json(s) {
                                Ok(chunks) => disk.write_any(&dest_path,&chunks),
                                Err(e) => Err(e)
                            },
                            _ => {
                                eprintln!("could not encode data as UTF8");
                                return Err(Box::new(CommandError::UnknownFormat));
                            }
                        },
                        _ => {
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        }
                    };
                    return match result {
                        Ok(_len) => a2kit::update_img_and_save(&mut img,&disk,img_path),
                        Err(e) => Err(e)
                    };
                } else {
                    eprintln!("destination file could not be interpreted");
                    return Err(Box::new(CommandError::UnknownFormat));
                }
            },

            // we are writing to a local file
            (None,None) => {
                std::fs::write(&dest_path,&file_data).expect("could not write data to disk");
                return Ok(());
            },

            // arguments inconsistent
            _ => {
                eprintln!("for `put` provide either `-f` alone, or all of `-f`, `-d`, and `-t`");
                return Err(Box::new(CommandError::InvalidCommand))
            }
        }
    }

    // Get file from local or from inside a disk image
    if let Some(cmd) = matches.subcommand_matches("get") {
        if !atty::is(atty::Stream::Stdin) {
            eprintln!("input is redirected, but `get` must start the pipeline");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let src_path = String::from(cmd.value_of("file").expect(RCH));
        let maybe_typ = cmd.value_of("type");
        let maybe_img = cmd.value_of("dimg");

        match (maybe_typ,maybe_img) {

            // we are getting from a disk image
            (Some(typ_str),Some(img_path)) => {
                let typ = ItemType::from_str(typ_str);
                // If getting a track, no need to resolve file system, handle differently
                match typ {
                    Ok(ItemType::Track) | Ok(ItemType::RawTrack) => {
                        if let Some(img) = a2kit::create_img_from_file(img_path) {
                            let maybe_object = match typ {
                                Ok(ItemType::Track) => img.get_track_bytes(&src_path),
                                Ok(ItemType::RawTrack) => img.get_track_buf(&src_path),
                                _ => panic!("{}",RCH)
                            };
                            return output_get(maybe_object,typ,None);
                        } else {
                            return Err(Box::new(CommandError::UnknownFormat));
                        }
                    }
                    _ => {}
                }
                if let Some((img,disk)) = a2kit::create_img_and_disk_from_file(img_path) {
                    // special handling for sparse data
                    if let Ok(ItemType::SparseData) = typ {
                        return match disk.read_any(&src_path) {
                            Ok(chunks) => {
                                println!("{}",chunks.to_json(4));
                                Ok(())
                            },
                            Err(e) => Err(e)
                        }
                    }
                    // special handling for random access text
                    if let Ok(ItemType::Records) = typ {
                        let record_length = match cmd.value_of("len") {
                            Some(s) => {
                                if let Ok(l) = usize::from_str(s) {
                                    l
                                } else {
                                    0 as usize
                                }
                            },
                            _ => 0 as usize
                        };
                        return match disk.read_records(&src_path,record_length) {
                            Ok(recs) => {
                                println!("{}",recs.to_json(4));
                                Ok(())
                            },
                            Err(e) => Err(e)
                        }
                    }
                    // other file types
                    let maybe_object = match typ {
                        Ok(ItemType::ApplesoftTokens) => disk.load(&src_path),
                        Ok(ItemType::IntegerTokens) => disk.load(&src_path),
                        Ok(ItemType::MerlinTokens) => disk.read_text(&src_path),
                        Ok(ItemType::Binary) => disk.bload(&src_path),
                        Ok(ItemType::Text) => disk.read_text(&src_path),
                        Ok(ItemType::Raw) => disk.read_text(&src_path),
                        Ok(ItemType::Chunk) => disk.read_chunk(&src_path),
                        _ => {
                            return Err(Box::new(CommandError::UnsupportedItemType));
                        }
                    };
                    return output_get(maybe_object,typ,Some(disk));
                } else {
                    return Err(Box::new(CommandError::UnknownFormat));
                }
            },

            // we are getting a local file
            (None,None) => {
                let object = std::fs::read(&src_path).expect("could not read file");
                std::io::stdout().write_all(&object).expect("could not write stdout");
                return Ok(())
            },

            // arguments inconsistent
            _ => {
                eprintln!("for `get` provide either `-f` alone, or all of `-f`, `-d`, and `-t`");
                return Err(Box::new(CommandError::InvalidCommand))
            }
        }
    }
    
    eprintln!("No subcommand was found, try `a2kit --help`");
    return Err(Box::new(CommandError::InvalidCommand));

}

// TODO: somehow fold SparseData and Records into the pattern
fn output_get(
    maybe_object: Result<(u16,Vec<u8>),Box<dyn Error>>,
    maybe_typ: Result<ItemType,CommandError>,
    maybe_disk: Option<Box<dyn A2Disk>>,
) -> Result<(),Box<dyn Error>> {
    match maybe_object {
        Ok(tuple) => {
            let object = tuple.1;
            if atty::is(atty::Stream::Stdout) {
                match (maybe_typ,maybe_disk) {
                    (Ok(ItemType::Text),Some(disk)) => println!("{}",disk.decode_text(&object)),
                    (Ok(ItemType::Track),None) => a2kit::display_track(tuple.0,&object),
                    _ => a2kit::display_chunk(tuple.0,&object)
                };
            } else {
                match (maybe_typ,maybe_disk) {
                    (Ok(ItemType::Text),Some(disk)) => println!("{}",disk.decode_text(&object)),
                    _ => std::io::stdout().write_all(&object).expect("could not write stdout")
                };
            }
            return Ok(())
        },
        Err(e) => return Err(e)
    }
}