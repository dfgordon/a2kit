//! # Command Line Interface
//! 
//! Simple subcommands are directly in `main.rs`.
//! More elaborate subcommands are in the `commands` module.

use clap::parser::ValueSource;
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

mod cli;

const RCH: &str = "unreachable was reached";

fn main() -> Result<(),Box<dyn std::error::Error>>
{
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let main_cmd = cli::build_cli();
    let matches = main_cmd.get_matches();
    
    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        return commands::mkdsk::mkdsk(cmd);
    }

    // Catalog a disk image

    if let Some(cmd) = matches.subcommand_matches("catalog") {
        let path_in_img = match cmd.get_one::<String>("file") {
            Some(path) => path,
            _ => "/"
        };
        return match a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg")) {
            Ok(mut disk) => disk.catalog_to_stdout(&path_in_img),
            Err(e) => Err(e)
        };
    }
    
    // Output the directory tree as a JSON string

    if let Some(cmd) = matches.subcommand_matches("tree") {
        return match a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg")) {
            Ok(mut disk) => {
                println!("{}",disk.tree(cmd.get_flag("meta"))?);
                Ok(())
            },
            Err(e) => Err(e)
        };
    }

    // Output the FS stats as a JSON string

    if let Some(cmd) = matches.subcommand_matches("stat") {
        return match a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg")) {
            Ok(mut dimg) => {
                let stats = dimg.stat()?;
                println!("{}",stats.to_json(2));
                Ok(())
            },
            Err(e) => Err(e)
        };
    }
    
    // Output the disk geometry as a JSON string

    if let Some(cmd) = matches.subcommand_matches("geometry") {
        return match a2kit::create_img_from_file_or_stdin(cmd.get_one::<String>("dimg")) {
            Ok(mut dimg) => {
                println!("{}",dimg.export_geometry(2)?);
                Ok(())
            },
            Err(e) => Err(e)
        };
    }

    // Verify

    if let Some(cmd) = matches.subcommand_matches("verify") {
        if let Ok(typ) = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH)) {
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
                    return Err(e);
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
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                error!("the file to minify could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
        if program.len()==0 {
            error!("minify did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                let mut minifier = applesoft::minifier::Minifier::new();
                if cmd.value_source("level").unwrap()==ValueSource::CommandLine {
                    minifier.set_level(usize::from_str_radix(cmd.get_one::<String>("level").unwrap(),10)?);
                }
                if cmd.value_source("flags").unwrap()==ValueSource::CommandLine {
                    minifier.set_flags(u64::from_str_radix(cmd.get_one::<String>("flags").unwrap(),10)?);
                }
                let object = minifier.minify(&program)?;
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
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let beg = usize::from_str_radix(cmd.get_one::<String>("beg").unwrap(),10)?;
        let end = usize::from_str_radix(cmd.get_one::<String>("end").unwrap(),10)?;
        let first = usize::from_str_radix(cmd.get_one::<String>("first").unwrap(),10)?;
        let step = usize::from_str_radix(cmd.get_one::<String>("step").unwrap(),10)?;
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                error!("the file to renumber could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
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
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let addr_opt = cmd.get_one::<String>("addr");
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                error!("the file to tokenize could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
        if program.len()==0 {
            error!("tokenize did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                if addr_opt==None {
                    error!("address needed to tokenize Applesoft");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                if let Ok(addr) = u16::from_str_radix(addr_opt.expect(RCH),10) {
                    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                    let object = tokenizer.tokenize(&program,addr)?;
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
                lang::verify_str(tree_sitter_integerbasic::language(),&program)?;
                if let Some(_addr) = addr_opt {
                    error!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut tokenizer = integer::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program))?;
                if atty::is(atty::Stream::Stdout) {
                    a2kit::display_block(0,&object);
                } else {
                    std::io::stdout().write_all(&object).expect("could not write output stream");
                }
                Ok(())
            },
            Ok(ItemType::MerlinText) => {
                lang::verify_str(tree_sitter_merlin6502::language(),&program)?;
                if let Some(_addr) = addr_opt {
                    error!("unnecessary address argument");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                let mut tokenizer = merlin::tokenizer::Tokenizer::new();
                let object = tokenizer.tokenize(String::from(&program))?;
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
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
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
                let program = tokenizer.detokenize(&tok)?;
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::IntegerTokens) => {
                let tokenizer = integer::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok)?;
                for line in program.lines() {
                    println!("{}",line);
                }
                Ok(())
            },
            Ok(ItemType::MerlinTokens) => {
                let tokenizer = merlin::tokenizer::Tokenizer::new();
                let program = tokenizer.detokenize(&tok)?;
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
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.create(&path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,&path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        }
    }

    // Update password for a file
    if let Some(cmd) = matches.subcommand_matches("protect") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let password = cmd.get_one::<String>("password").expect(RCH);
        let read = cmd.get_flag("read");
        let write = cmd.get_flag("write");
        let delete = cmd.get_flag("delete");
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.protect(path_in_img,password,read,write,delete) {
                Ok(()) => a2kit::save_img(&mut disk,path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }

    // Remove password from a file
    if let Some(cmd) = matches.subcommand_matches("unprotect") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        return match a2kit::create_fs_from_file(&path_to_img) {
            Ok(mut disk) => match disk.unprotect(path_in_img) {
                Ok(()) => a2kit::save_img(&mut disk,path_to_img),
                Err(e) => Err(e)
            },
            Err(e) => Err(e)
        };
    }
    
    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
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
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
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
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
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
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let name = cmd.get_one::<String>("name").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
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
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let typ = cmd.get_one::<String>("type").expect(RCH);
        let aux = cmd.get_one::<String>("aux").expect(RCH);
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

