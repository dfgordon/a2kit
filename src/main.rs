use clap::{arg,Command};
use std::io::{Read,Write};
use std::str::FromStr;
mod walker;
mod disk_base;
mod applesoft;
mod integer;
mod dos33;
mod prodos;
use a2kit::disk_base::{DiskImageType,ItemType,CommandError};
use crate::disk_base::A2Disk;

const RCH: &str = "unreachable was reached";

fn main() -> Result<(),Box<dyn std::error::Error>>
{
    let long_help =
"This tool is intended to be used with redirection and pipes.
On Windows please use PowerShell.

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
    .subcommand(Command::new("mkdir")
        .arg(arg!(-f --file <PATH> "path inside disk image of new directory"))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .about("create a new directory inside a disk image"))
    .subcommand(Command::new("verify")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt"]))
        .about("read from stdin and error check"))
    .subcommand(Command::new("get")
        .arg(arg!(-f --file <PATH> "source path, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","bin","txt","raw"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .about("read from local or disk image, write to stdout"))
    .subcommand(Command::new("put")
        .arg(arg!(-f --file <PATH> "destination path, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(["atok","itok","bin","txt","raw"]))
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
    
    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        match DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap() {
            DiskImageType::DO => match u8::from_str(cmd.value_of("volume").expect(RCH)) {
                Ok(vol) if vol>=1 && vol<=254 => {
                    let mut disk = dos33::Disk::new();
                    disk.format(254,true);
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
            }
            DiskImageType::WOZ => {
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
        };
    }

    // Catalog a disk image
    if let Some(cmd) = matches.subcommand_matches("catalog") {
        let path_in_img = match cmd.value_of("file") {
            Some(path) => path.to_string(),
            _ => "/".to_string()
        };
        if let Some(path_to_img) = cmd.value_of("dimg") {
            let disk = a2kit::create_disk_from_file(&path_to_img);
            disk.catalog_to_stdout(&path_in_img);
            return Ok(());
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
                _ => panic!("unexpected language")
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
            panic!("line entry is not supported for `tokenize`, please pipe something in");
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let addr_opt = cmd.value_of("addr");
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                if let Ok(addr) = u16::from_str_radix(addr_opt.expect("address needed to tokenize Applesoft"),10) {
                    let mut program = String::new();
                    std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
                    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                    let object = tokenizer.tokenize(String::from(&program),addr);
                    if atty::is(atty::Stream::Stdout) {
                        let disp_lines = object.len()/8;
                        let remainder = object.len()%8;
                        for i in 0..disp_lines {
                            println!("{:02X?}",&object[i*8..i*8+8]);
                        }
                        if remainder>0 {
                            println!("{:02X?}",&object[disp_lines*8..disp_lines*8+remainder]);
                        }
                    } else {
                        std::io::stdout().write_all(&object).expect("could not write output stream");
                    }
                    return Ok(());
                }
                Err(Box::new(CommandError::OutOfRange))
            },
            Ok(ItemType::IntegerText) => {
                if let Some(_addr) = addr_opt {
                    panic!("unnecessary address argument");
                }
                let mut program = String::new();
                std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program));
                if atty::is(atty::Stream::Stdout) {
                    let disp_lines = object.len()/8;
                    let remainder = object.len()%8;
                    for i in 0..disp_lines {
                        println!("{:02X?}",&object[i*8..i*8+8]);
                    }
                    if remainder>0 {
                        println!("{:02X?}",&object[disp_lines*8..disp_lines*8+remainder]);
                    }
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
            panic!("line entry is not supported for `detokenize`, please pipe something in");
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
        match disk.create(&path_in_img,None) {
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
            panic!("cannot use `put` with console input, please pipe something in");
        }
        if !atty::is(atty::Stream::Stdout) {
            panic!("output is redirected, but `put` must end the pipeline");
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
                    (_ ,Ok(ItemType::Binary)) => panic!("binary file requires an address"),
                    _ => 768 as u16
                };
                let mut disk = a2kit::create_disk_from_file(img_path);
                let result = match typ {
                    Ok(ItemType::ApplesoftTokens) => disk.save(&dest_path,&file_data,ItemType::ApplesoftTokens),
                    Ok(ItemType::IntegerTokens) => disk.save(&dest_path,&file_data,ItemType::IntegerTokens),
                    Ok(ItemType::Binary) => disk.bsave(&dest_path,&file_data,load_address),
                    Ok(ItemType::Text) => match std::str::from_utf8(&file_data) {
                        Ok(s) => match disk.encode_text(&s.to_string()) {
                            Ok(encoded) => disk.write_text(&dest_path,&encoded),
                            Err(e) => Err(e)
                        },
                        _ => panic!("problem with utf8 while writing text file")
                    },
                    Ok(ItemType::Raw) => disk.write_text(&dest_path,&file_data),
                    _ => panic!("not supported")
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
            panic!("input is redirected, but `get` must start the pipeline");
        }
        let dest_path = String::from(cmd.value_of("file").expect(RCH));
        let maybe_typ = cmd.value_of("type");
        let maybe_img = cmd.value_of("dimg");

        match (maybe_typ,maybe_img) {

            // we are getting from a disk image
            (Some(typ_str),Some(img_path)) => {
                let typ = ItemType::from_str(typ_str);
                let disk = a2kit::create_disk_from_file(img_path);
                let maybe_object = match typ {
                    Ok(ItemType::ApplesoftTokens) => disk.load(&dest_path),
                    Ok(ItemType::IntegerTokens) => disk.load(&dest_path),
                    Ok(ItemType::Binary) => disk.bload(&dest_path),
                    Ok(ItemType::Text) => disk.read_text(&dest_path),
                    Ok(ItemType::Raw) => disk.read_text(&dest_path),
                    _ => panic!("not supported")
                };
                match maybe_object {
                    Ok(tuple) => {
                        let object = tuple.1;
                        if atty::is(atty::Stream::Stdout) {
                            match typ {
                                Ok(ItemType::Text) => println!("{}",disk.decode_text(&object)),
                                _ => {
                                    let disp_lines = object.len()/8;
                                    let remainder = object.len()%8;
                                    for i in 0..disp_lines {
                                        println!("{:02X?}",&object[i*8..i*8+8]);
                                    }
                                    if remainder>0 {
                                        println!("{:02X?}",&object[disp_lines*8..disp_lines*8+remainder]);
                                    }        
                                }
                            }
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
                let object = std::fs::read(&dest_path).expect("could not read file");
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
    
    eprintln!("No subcommand was found");
    return Err(Box::new(CommandError::InvalidCommand));

}
