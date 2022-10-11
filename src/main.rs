use clap::{arg,Command};
use std::io::{Read,Write};
use std::str::FromStr;
#[cfg(windows)]
use colored;
use a2kit::disk_base::{DiskImageType,ItemType,CommandError,A2Disk,Records,SparseData};
use a2kit::dos33;
use a2kit::prodos;
use a2kit::walker;
use a2kit::applesoft;
use a2kit::integer;

const RCH: &str = "unreachable was reached";

fn main() -> Result<(),Box<dyn std::error::Error>>
{
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
    let long_help =
"This tool is intended to be used with redirection and pipes.
PowerShell may require you to wrap the pipeline in the native shell.

Examples:
create DOS image: `a2kit mkdsk -v 254 -t do > myimg.do`
create ProDOS image: `a2kit mkdsk -v disk.new -t po > myimg.po`
Applesoft line entry checker: `a2kit verify -t atxt`
Applesoft error checker: `a2kit get -f myprog.bas | a2kit verify -t atxt`
Tokenize to file: `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt > prog.atok
Tokenize to image: `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt \\
                    | a2kit put -f prog -t atok -d myimg.do`
Detokenize from image: `a2kit get -f prog -t atok -d myimg.do | a2kit detokenize -t atok";

    let matches = Command::new("a2kit")
        .about("Manipulates Apple II files and disk images, with language comprehension.")
    .after_long_help(long_help)
    .subcommand(Command::new("mkdsk")
        .arg(arg!(-v --volume <VOLUME> "volume name or number"))
        .arg(arg!(-t --type <TYPE> "type of disk image to create").possible_values(["do","po"]))
        .arg(arg!(-k --kind <SIZE> "kind of disk").possible_values([
            "5.25in",
            "3.5in",
            "hdmax"]).required(false)
            .default_value("5.25in"))
        .about("write a blank disk image to stdout"))
    .subcommand(Command::new("reorder")
        .arg(arg!(-d --dimg <PATH> "path to disk image"))
        .about("Put a disk image into its natural order"))
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
    .subcommand(Command::new("verify")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt"]))
        .about("read from stdin and error check"))
    .subcommand(Command::new("get")
        .arg(arg!(-f --file <PATH> "source path or chunk index, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","bin","txt","raw","chunk","rec","any"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .arg(arg!(-l --len <LENGTH> "length of record in DOS 3.3 random access text file").required(false))
        .about("read from local or disk image, write to stdout"))
    .subcommand(Command::new("put")
        .arg(arg!(-f --file <PATH> "destination path or chunk index, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","bin","txt","raw","chunk","rec","any"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .arg(arg!(-a --addr <ADDRESS> "address of binary file").required(false))
        .about("read from stdin, write to local or disk image"))
    .subcommand(Command::new("catalog")
        .arg(arg!(-f --file <PATH> "path of directory inside disk image").required(false))
        .arg(arg!(-d --dimg <PATH> "path to disk image"))
        .about("write disk image catalog to stdout"))
    .subcommand(Command::new("tokenize")
        .arg(arg!(-a --addr <ADDRESS> "address of tokenized code (Applesoft only)").required(false))
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt"]))
        .about("read from stdin, tokenize, write to stdout"))
    .subcommand(Command::new("detokenize")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atok","itok"]))
        .about("read from stdin, detokenize, write to stdout"))
    .get_matches();
    
    // Put a disk image into its natural ordering
    if let Some(cmd) = matches.subcommand_matches("reorder") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let disk = a2kit::create_disk_from_file(&path_to_img);
        std::io::stdout().write_all(&disk.to_img()).expect("write to stdout failed");
        return Ok(());
    }

    // Create a disk image
    // TODO: allow creation of DOS ordered ProDOS

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        match DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap() {
            DiskImageType::DO => match u8::from_str(cmd.value_of("volume").expect(RCH)) {
                Ok(vol) if vol>=1 && vol<=254 => {
                    let mut disk = dos33::Disk::new();
                    disk.format(254,true,17);
                    let buf = disk.to_img();
                    eprintln!("writing {} bytes",buf.len());
                    std::io::stdout().write_all(&buf).expect("write to stdout failed");
                    return Ok(());
                },
                _ => {
                    eprintln!("volume must be from 1 to 254");
                    return Err(Box::new(CommandError::OutOfRange));
                }
            },
            DiskImageType::PO => {
                let kind = cmd.value_of("kind").expect(RCH);
                let (blocks,floppy) = match kind {
                    "5.25in" => (280,true),
                    "3.5in" => (1600,true),
                    "hdmax" => (65535,false),
                    _ => (280,true)
                };
                let mut disk = prodos::Disk::new(blocks);
                disk.format(&cmd.value_of("volume").expect(RCH).to_string(),floppy,None);
                let buf = disk.to_img();
                eprintln!("Writing {} bytes",buf.len());
                std::io::stdout().write_all(&buf).expect("write to stdout failed");
                return Ok(());
            },
            DiskImageType::WOZ => {
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
        };
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

    // Tokenize BASIC

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
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }

    // Detokenize BASIC

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
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }

    // Create directory inside disk image
    if let Some(cmd) = matches.subcommand_matches("mkdir") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk = a2kit::create_disk_from_file(&path_to_img);
        match disk.create(&path_in_img) {
            Ok(()) => {
                let updated_img_data = disk.to_img();
                std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
                return Ok(())
            },
            Err(e) => return Err(e)
        };
    }

    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk = a2kit::create_disk_from_file(&path_to_img);
        match disk.delete(&path_in_img) {
            Ok(()) => {
                let updated_img_data = disk.to_img();
                std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
                return Ok(())
            },
            Err(e) => return Err(e)
        };
    }

    // Lock a file or directory
    if let Some(cmd) = matches.subcommand_matches("lock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk = a2kit::create_disk_from_file(&path_to_img);
        match disk.lock(&path_in_img) {
            Ok(()) => {
                let updated_img_data = disk.to_img();
                std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
                return Ok(())
            },
            Err(e) => return Err(e)
        };
    }

    // Unlock a file or directory
    if let Some(cmd) = matches.subcommand_matches("unlock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk = a2kit::create_disk_from_file(&path_to_img);
        match disk.unlock(&path_in_img) {
            Ok(()) => {
                let updated_img_data = disk.to_img();
                std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
                return Ok(())
            },
            Err(e) => return Err(e)
        };
    }

    // Rename a file or directory
    if let Some(cmd) = matches.subcommand_matches("rename") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let name = String::from(cmd.value_of("name").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk = a2kit::create_disk_from_file(&path_to_img);
        match disk.rename(&path_in_img,&name) {
            Ok(()) => {
                let updated_img_data = disk.to_img();
                std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
                return Ok(())
            },
            Err(e) => return Err(e)
        };
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
                let mut disk = a2kit::create_disk_from_file(img_path);
                let result = match typ {
                    Ok(ItemType::ApplesoftTokens) => disk.save(&dest_path,&file_data,ItemType::ApplesoftTokens,None),
                    Ok(ItemType::IntegerTokens) => disk.save(&dest_path,&file_data,ItemType::IntegerTokens,None),
                    Ok(ItemType::Binary) => disk.bsave(&dest_path,&file_data,load_address,None),
                    Ok(ItemType::Text) => match std::str::from_utf8(&file_data) {
                        Ok(s) => match disk.encode_text(s) {
                            Ok(encoded) => disk.write_text(&dest_path,&encoded),
                            Err(e) => Err(e)
                        },
                        _ => {
                            eprintln!("could not encode data as UTF8");
                            return Err(Box::new(CommandError::InputFormatBad));
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
                            return Err(Box::new(CommandError::InputFormatBad));
                        }
                    },
                    Ok(ItemType::SparseData) => match std::str::from_utf8(&file_data) {
                        Ok(s) => match SparseData::from_json(s) {
                            Ok(chunks) => disk.write_any(&dest_path,&chunks),
                            Err(e) => Err(e)
                        },
                        _ => {
                            eprintln!("could not encode data as UTF8");
                            return Err(Box::new(CommandError::InputFormatBad));
                        }
                    },
                    _ => {
                        return Err(Box::new(CommandError::UnsupportedItemType));
                    }
                };
                return match result {
                    Ok(_len) => {
                        let updated_img_data = disk.to_img();
                        std::fs::write(img_path,updated_img_data).expect("could not write disk image to disk");
                        Ok(())
                    }
                    Err(e) => Err(e)
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
                let disk = a2kit::create_disk_from_file(img_path);
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
                    Ok(ItemType::Binary) => disk.bload(&src_path),
                    Ok(ItemType::Text) => disk.read_text(&src_path),
                    Ok(ItemType::Raw) => disk.read_text(&src_path),
                    Ok(ItemType::Chunk) => disk.read_chunk(&src_path),
                    _ => {
                        return Err(Box::new(CommandError::UnsupportedItemType));
                    }
                };
                match maybe_object {
                    Ok(tuple) => {
                        let object = tuple.1;
                        if atty::is(atty::Stream::Stdout) {
                            match typ {
                                Ok(ItemType::Text) => println!("{}",disk.decode_text(&object)),
                                _ => a2kit::display_chunk(tuple.0,&object)
                            };
                        } else {
                            match typ {
                                Ok(ItemType::Text) => println!("{}",disk.decode_text(&object)),
                                _ => std::io::stdout().write_all(&object).expect("could not write stdout")
                            };
                        }
                        return Ok(())
                    },
                    Err(e) => return Err(e)
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
