//! # Command Line Interface
//! 
//! Simple subcommands are directly in `main.rs`.
//! More elaborate subcommands are in the `commands` module.

use clap::{arg,Command,ArgAction};
use env_logger;
use std::io::{Read,Write};
use std::str::FromStr;
#[cfg(windows)]
use colored;
use log::error;
use a2kit::commands;
use a2kit::commands::{ItemType,CommandError};
use a2kit::lang;
use a2kit::lang::applesoft;
use a2kit::lang::integer;
use a2kit::lang::merlin;

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
  levels: trace,debug,info,warn,error

Examples:
---------
create DOS image:      `a2kit mkdsk -o dos33 -v 254 -t woz1 > myimg.woz`
create ProDOS image:   `a2kit mkdsk -o prodos -v disk.new -t po > myimg.po`
Language line entry:   `a2kit verify -t atxt`
Language file check:   `a2kit get -f myprog.bas | a2kit verify -t atxt`
Tokenize to file:      `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt > prog.atok
Tokenize to image:     `a2kit get -f prog.bas | a2kit tokenize -a 2049 -t atxt \\
                           | a2kit put -f prog -t atok -d myimg.dsk`
Detokenize from image: `a2kit get -f prog -t atok -d myimg.dsk | a2kit detokenize -t atok";

    let img_types = ["d13","do","po","woz1","woz2","imd"];
    let os_names = ["cpm2","dos32","dos33","prodos","pascal"];
    let disk_kinds = [
        "8in",
        "8in-trs80",
        "8in-nabu",
        "5.25in",
        "5.25in-kayii",
        "5.25in-kay4",
        "5.25in-osb-sd",
        "5.25in-osb-dd",
        "3.5in",
        "3.5in-ss",
        "3.5in-ds",
        "hdmax"
    ];
    let get_put_types = [
        "atok","itok","mtok","bin","txt","raw","block","sec","track","raw_track","rec","any"     
    ];

    let matches = Command::new("a2kit")
        .about("Manipulates Apple II files and disk images, with language comprehension.")
    .after_long_help(long_help)
    .subcommand(Command::new("mkdsk")
        .arg(arg!(-v --volume <VOLUME> "volume name or number").required(false))
        .arg(arg!(-t --type <TYPE> "type of disk image to create").possible_values(img_types))
        .arg(arg!(-o --os <OS> "operating system format").possible_values(os_names))
        .arg(arg!(-b --bootable "make disk bootable").action(ArgAction::SetTrue))
        .arg(arg!(-k --kind <SIZE> "kind of disk").possible_values(disk_kinds)
            .required(false)
            .default_value("5.25in"))
        .arg(arg!(-d --dimg <PATH> "disk image path to create"))
        .about("write a blank disk image to the given path"))
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
    .subcommand(Command::new("minify")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt"]))
        .about("reduce program size"))
    .subcommand(Command::new("renumber")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt"]))
        .arg(arg!(-b --beg <NUM> "lowest number to renumber"))
        .arg(arg!(-e --end <NUM> "highest number to renumber plus 1"))
        .arg(arg!(-f --first <NUM> "first number"))
        .arg(arg!(-s --step <NUM> "step between numbers"))
        .about("renumber BASIC program lines"))
    .subcommand(Command::new("get")
        .arg(arg!(-f --file <PATH> "source path or address, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(get_put_types))
        .arg(arg!(-d --dimg <PATH> "path to disk image").required(false))
        .arg(arg!(-l --len <LENGTH> "length of record in DOS 3.3 random access text file").required(false))
        .about("read from local or disk image, write to stdout"))
    .subcommand(Command::new("put")
        .arg(arg!(-f --file <PATH> "destination path or address, maybe inside disk image"))
        .arg(arg!(-t --type <TYPE> "type of the file").required(false).possible_values(get_put_types))
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
    
    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        return commands::mkdsk::mkdsk(cmd);
    }

    // Catalog a disk image
    if let Some(cmd) = matches.subcommand_matches("catalog") {
        let path_in_img = match cmd.value_of("file") {
            Some(path) => path,
            _ => "/"
        };
        if let Some(path_to_img) = cmd.value_of("dimg") {
            return match a2kit::create_fs_from_file(path_to_img) {
                Ok(mut disk) => disk.catalog_to_stdout(&path_in_img),
                Err(e) => Err(e)
            };
        }
        panic!("{}",RCH);
    }
    
    // Verify

    if let Some(cmd) = matches.subcommand_matches("verify") {
        if let Ok(typ) = ItemType::from_str(cmd.value_of("type").expect(RCH)) {
            let res = match typ
            {
                ItemType::ApplesoftText => lang::verify_stdin(tree_sitter_applesoft::language(),"]"),
                ItemType::IntegerText => lang::verify_stdin(tree_sitter_integerbasic::language(),">"),
                ItemType::MerlinText => lang::verify_stdin(tree_sitter_merlin6502::language(),":"),
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

    // Minify

    if let Some(cmd) = matches.subcommand_matches("minify") {
        if atty::is(atty::Stream::Stdin) {
            error!("line entry is not supported for `minify`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let mut program = String::new();
        std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
        if program.len()==0 {
            error!("minify did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                let mut minifier = applesoft::minifier::Minifier::new();
                let object = minifier.minify(&program);
                println!("{}",&object);
                Ok(())
            },
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }
    
    // Renumber

    if let Some(cmd) = matches.subcommand_matches("renumber") {
        if atty::is(atty::Stream::Stdin) {
            error!("line entry is not supported for `renumber`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let beg = usize::from_str_radix(cmd.value_of("beg").unwrap(),10)?;
        let end = usize::from_str_radix(cmd.value_of("end").unwrap(),10)?;
        let first = usize::from_str_radix(cmd.value_of("first").unwrap(),10)?;
        let step = usize::from_str_radix(cmd.value_of("step").unwrap(),10)?;
        let mut program = String::new();
        std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
        if program.len()==0 {
            error!("renumber did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                let mut renumberer = applesoft::renumber::Renumberer::new();
                let object = renumberer.renumber(&program,beg,end,first,step)?;
                println!("{}",&object);
                Ok(())
            },
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }
    
    // Tokenize BASIC or Encode Merlin

    if let Some(cmd) = matches.subcommand_matches("tokenize") {
        if atty::is(atty::Stream::Stdin) {
            error!("line entry is not supported for `tokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let addr_opt = cmd.value_of("addr");
        let mut program = String::new();
        std::io::stdin().read_to_string(&mut program).expect("could not read input stream");
        if program.len()==0 {
            error!("tokenize did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                if addr_opt==None {
                    error!("address needed to tokenize Applesoft");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                if let Ok(addr) = u16::from_str_radix(addr_opt.expect(RCH),10) {
                    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                    let object = tokenizer.tokenize(&program,addr);
                    if atty::is(atty::Stream::Stdout) {
                        a2kit::display_block(addr,&object);
                    } else {
                        std::io::stdout().write_all(&object).expect("could not write output stream");
                    }
                    return Ok(());
                }
                Err(Box::new(CommandError::OutOfRange))
            },
            Ok(ItemType::IntegerText) => {
                if let Some(_addr) = addr_opt {
                    error!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program));
                if atty::is(atty::Stream::Stdout) {
                    a2kit::display_block(0,&object);
                } else {
                    std::io::stdout().write_all(&object).expect("could not write output stream");
                }
                Ok(())
            },
            Ok(ItemType::MerlinText) => {
                if let Some(_addr) = addr_opt {
                    error!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut tokenizer = merlin::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program));
                if atty::is(atty::Stream::Stdout) {
                    a2kit::display_block(0,&object);
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
            error!("line entry is not supported for `detokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let mut tok: Vec<u8> = Vec::new();
        std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
        if tok.len()==0 {
            error!("detokenize did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftTokens) => {
                let tokenizer = applesoft::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok);
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::IntegerTokens) => {
                let tokenizer = integer::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok);
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::MerlinTokens) => {
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
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.create(&path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        }
    }

    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.delete(&path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Lock a file or directory
    if let Some(cmd) = matches.subcommand_matches("lock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.lock(&path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Unlock a file or directory
    if let Some(cmd) = matches.subcommand_matches("unlock") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.unlock(&path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Rename a file or directory
    if let Some(cmd) = matches.subcommand_matches("rename") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let name = String::from(cmd.value_of("name").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.rename(&path_in_img,&name) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Retype a file
    if let Some(cmd) = matches.subcommand_matches("retype") {
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let typ = String::from(cmd.value_of("type").expect(RCH));
        let aux = String::from(cmd.value_of("aux").expect(RCH));
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.retype(&path_in_img,&typ,&aux) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Put file inside disk image, or save to local
    if let Some(cmd) = matches.subcommand_matches("put") {
        return commands::put::put(cmd);
    }

    // Get file from local or from inside a disk image
    if let Some(cmd) = matches.subcommand_matches("get") {
        return commands::get::get(cmd);
    }
    
    error!("No subcommand was found, try `a2kit --help`");
    return Err(Box::new(CommandError::InvalidCommand));

}

