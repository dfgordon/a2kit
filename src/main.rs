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
use a2kit::commands;
use a2kit::commands::{ItemType,CommandError};
use a2kit::lang;
use a2kit::lang::applesoft;
use a2kit::lang::integer;
use a2kit::lang::merlin;
use a2kit::lang::server::Analysis;
use colored::Colorize;

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
        let mut disk = a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg"))?;
        return if cmd.get_flag("generic") {
            let rows = disk.catalog_to_vec(&path_in_img)?;
            for row in rows {
                println!("{}",row);
            }
            Ok(())
        } else {
            disk.catalog_to_stdout(&path_in_img)
        }
    }
    
    // Output the directory tree as a JSON string

    if let Some(cmd) = matches.subcommand_matches("tree") {
        let mut disk = a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg"))?;
        println!("{}",disk.tree(cmd.get_flag("meta"), cmd.get_one::<u16>("indent").copied())?);
        return Ok(());
    }

    // Output the matches to the glob pattern

    if let Some(cmd) = matches.subcommand_matches("glob") {
        let mut disk = a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg"))?;
        let v = disk.glob(cmd.get_one::<String>("file").unwrap(),false)?;
        let mut obj = json::array![];
        for m in v {
            obj.push(m)?;
        }
        let s = match cmd.get_one::<u16>("indent") {
            Some(spaces) => json::stringify_pretty(obj, *spaces),
            None => json::stringify(obj)
        };
        println!("{}",s);
        return Ok(());
    }

    // Output the FS stats as a JSON string

    if let Some(cmd) = matches.subcommand_matches("stat") {
        let mut disk = a2kit::create_fs_from_file_or_stdin(cmd.get_one::<String>("dimg"))?;
        let stats = disk.stat()?;
        println!("{}",stats.to_json(cmd.get_one::<u16>("indent").copied()));
        return Ok(());
    }
    
    // Output the disk geometry as a JSON string

    if let Some(cmd) = matches.subcommand_matches("geometry") {
        let mut disk = a2kit::create_img_from_file_or_stdin(cmd.get_one::<String>("dimg"))?;
        println!("{}",disk.export_geometry(cmd.get_one::<u16>("indent").copied())?);
        return Ok(());
    }

    // Verify

    if let Some(cmd) = matches.subcommand_matches("verify") {
        let mut analyzer: Box<dyn Analysis> = match ItemType::from_str(cmd.get_one::<String>("type").expect(RCH)) {
            Ok(ItemType::ApplesoftText) => Box::new(lang::applesoft::diagnostics::Analyzer::new()),
            Ok(ItemType::IntegerText) => Box::new(lang::integer::diagnostics::Analyzer::new()),
            Ok(ItemType::MerlinText) => Box::new(lang::merlin::diagnostics::Analyzer::new()),
            _ => panic!("not handled")
        };
        if cmd.value_source("config").unwrap()==ValueSource::CommandLine {
            analyzer.update_config(cmd.get_one::<String>("config").unwrap())?;
        }
        let doc = lang::Document::from_string(analyzer.read_stdin(),0);
        if let Some(ws_path) = cmd.get_one::<String>("workspace") {
            match lsp_types::Url::from_directory_path(ws_path) {
                Ok(uri) => analyzer.init_workspace(vec![uri],vec![doc.clone()])?,
                Err(_) => return Err(Box::new(lang::Error::PathNotFound))
            }
        }
        if cmd.get_flag("sexpr") {
            analyzer.eprint_lines_sexpr(&doc.text);
        }
        analyzer.analyze(&doc)?;
        for diag in analyzer.get_diags(&doc) {
            lang::eprint_diagnostic(&diag,&doc.text);
        }
        let [err,warn,_info] = analyzer.err_warn_info_counts();
        if warn > 0 {
            eprintln!("! {} {}",warn.to_string().bright_yellow(),"warnings".bright_yellow());
        }
        if err==0 {
            eprintln!("\u{2713} {}","Passing".green());
            if !atty::is(atty::Stream::Stdout) {
                // if not the console, pipe the code to the next node
                println!("{}",doc.text);
            }
            return Ok(());
        } else {
            eprintln!("\u{2717} {} {}",err.to_string().red(),"errors".red());
            return Err(Box::new(lang::Error::Syntax));
        }
    }

    // Minify

    if let Some(cmd) = matches.subcommand_matches("minify") {
        if atty::is(atty::Stream::Stdin) {
            log::error!("line entry is not supported for `minify`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                log::error!("the file to minify could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
        if program.len()==0 {
            log::error!("minify did not receive any data from previous node");
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
            log::error!("line entry is not supported for `renumber`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let beg = usize::from_str_radix(cmd.get_one::<String>("beg").unwrap(),10)?;
        let end = usize::from_str_radix(cmd.get_one::<String>("end").unwrap(),10)?;
        let first = usize::from_str_radix(cmd.get_one::<String>("first").unwrap(),10)?;
        let step = usize::from_str_radix(cmd.get_one::<String>("step").unwrap(),10)?;
        let reorder = cmd.get_flag("reorder");
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                log::error!("the file to renumber could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
        if program.len()==0 {
            log::error!("renumber did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                let mut renumberer = applesoft::renumber::Renumberer::new();
                renumberer.set_flags(match reorder {true => 1, false => 0});
                let new_prog = renumberer.renumber(&program,beg,end,first,step)?;
                println!("{}",&new_prog);
                Ok(())
            },
            Ok(ItemType::IntegerText) => {
                lang::verify_str(tree_sitter_integerbasic::language(), &program)?;
                let mut renumberer = integer::renumber::Renumberer::new();
                renumberer.set_flags(match reorder {true => 1, false => 0});
                let new_prog = renumberer.renumber(&program,beg,end,first,step)?;
                log::warn!("line number expressions must be manually adjusted");
                println!("{}",&new_prog);
                Ok(())
            }
            _ => Err(Box::new(CommandError::UnsupportedItemType))
        };
    }
    
    // Tokenize BASIC or Encode Merlin

    if let Some(cmd) = matches.subcommand_matches("tokenize") {
        if atty::is(atty::Stream::Stdin) {
            log::error!("line entry is not supported for `tokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let addr_opt = cmd.get_one::<String>("addr");
        let mut program = String::new();
        match std::io::stdin().read_to_string(&mut program) {
            Ok(_) => {},
            Err(e) => {
                log::error!("the file to tokenize could not be interpreted as a string");
                return Err(Box::new(e));
            }
        }
        if program.len()==0 {
            log::error!("tokenize did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        return match typ
        {
            Ok(ItemType::ApplesoftText) => {
                lang::verify_str(tree_sitter_applesoft::language(),&program)?;
                if addr_opt==None {
                    log::error!("address needed to tokenize Applesoft");
                    return Err(Box::new(CommandError::InvalidCommand));
                }
                if let Ok(addr) = u16::from_str_radix(addr_opt.expect(RCH),10) {
                    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
                    let object = tokenizer.tokenize(&program,addr)?;
                    if atty::is(atty::Stream::Stdout) {
                        a2kit::display_block(addr as usize,&object);
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
                    log::error!("unnecessary address argument");
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
                    log::error!("unnecessary address argument");
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
            log::error!("line entry is not supported for `detokenize`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
        let mut tok: Vec<u8> = Vec::new();
        std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
        if tok.len()==0 {
            log::error!("detokenize did not receive any data from previous node");
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

    // Assemble source code

    if let Some(cmd) = matches.subcommand_matches("asm") {
        let mut config = merlin::settings::Settings::new();
        config.version = match cmd.get_one::<String>("assembler").expect(RCH).as_str() {
            "m8" => merlin::MerlinVersion::Merlin8,
            "m16" => merlin::MerlinVersion::Merlin16,
            "m16+" => merlin::MerlinVersion::Merlin16Plus,
            "m32" => merlin::MerlinVersion::Merlin32,
            _ => panic!("{}",RCH)
        };
        let mut analyzer = lang::merlin::diagnostics::Analyzer::new();
        analyzer.set_config(config.clone());
        // if cmd.value_source("config").unwrap()==ValueSource::CommandLine {
        //     analyzer.update_config(cmd.get_one::<String>("config").unwrap())?;
        // }
        let doc = lang::Document::from_string(analyzer.read_stdin(),0);
        if let Some(ws_path) = cmd.get_one::<String>("workspace") {
            match lsp_types::Url::from_directory_path(ws_path) {
                Ok(uri) => analyzer.init_workspace(vec![uri],vec![doc.clone()])?,
                Err(_) => return Err(Box::new(lang::Error::PathNotFound))
            }
        }
        analyzer.analyze(&doc)?;
        let symbols = analyzer.get_symbols();
        for diag in analyzer.get_diags(&doc) {
            lang::eprint_diagnostic(&diag,&doc.text);
        }
        let [err,_warn,_info] = analyzer.err_warn_info_counts();
        if err==0 {
            let mut asm = merlin::assembly::Assembler::new();
            asm.set_config(config);
            if cmd.get_flag("literals") {
                let dsyms = merlin::assembly::Assembler::dasm_symbols(std::sync::Arc::new(symbols));
                asm.use_shared_symbols(std::sync::Arc::new(dsyms));
            } else {
                asm.use_shared_symbols(std::sync::Arc::new(symbols));
            }
            let object = asm.spot_assemble(doc.text.clone(), 0, doc.text.len() as isize, None)?;
            if atty::is(atty::Stream::Stdout) {
                a2kit::display_block(0,&object);
            } else {
                std::io::stdout().write_all(&object).expect("could not write output stream");
            }
            return Ok(());
        } else {
            eprintln!("\u{2717} {} {}",err.to_string().red(),"errors".red());
            return Err(Box::new(lang::Error::Syntax));
        }
    }

    // Disassemble binary to Merlin source

    if let Some(cmd) = matches.subcommand_matches("dasm") {
        if atty::is(atty::Stream::Stdin) {
            log::error!("line entry is not supported for `dasm`, please pipe something in");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let proc = match cmd.get_one::<String>("proc").expect(RCH).as_str() {
            "6502" => merlin::ProcessorType::_6502,
            "65c02" => merlin::ProcessorType::_65c02,
            "65802" => merlin::ProcessorType::_65802,
            "65816" => merlin::ProcessorType::_65c816,
            _ => panic!("{}",RCH)
        };
        let (m8bit,x8bit) = match cmd.get_one::<String>("mx").expect(RCH).as_str() {
            "00" => (false,false),
            "01" => (false,true),
            "10" => (true,false),
            "11" => (true,true),
            _ => panic!("{}",RCH)
        };
        let org = match u16::from_str(cmd.get_one::<String>("org").expect(RCH)) {
            Ok(x) => x,
            Err(_) => {
                log::error!("origin did not parse as decimal unsigned 16 bit integer");
                return Err(Box::new(CommandError::OutOfRange))
            }
        };
        let mut tok: Vec<u8> = Vec::new();
        tok.append(&mut vec![0;org as usize]);
        std::io::stdin().read_to_end(&mut tok).expect("could not read input stream");
        if tok.len()==org as usize {
            log::error!("dasm did not receive any data from previous node");
            return Err(Box::new(CommandError::InvalidCommand));
        }
        let mut dasm = merlin::disassembly::Disassembler::new();
        dasm.set_mx(m8bit,x8bit);
        let rng =  merlin::disassembly::DasmRange::Range([org as usize,tok.len()]);
        let program = dasm.disassemble(&tok, rng, proc, "some")?;
        for line in program.lines() {
            println!("{}",line);
        }
        return Ok(());
    }

    // Create directory inside disk image
    if let Some(cmd) = matches.subcommand_matches("mkdir") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.create(&path_in_img)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Update password for a file
    if let Some(cmd) = matches.subcommand_matches("protect") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let password = cmd.get_one::<String>("password").expect(RCH);
        let read = cmd.get_flag("read");
        let write = cmd.get_flag("write");
        let delete = cmd.get_flag("delete");
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.protect(path_in_img,password,read,write,delete)?;
        return a2kit::save_img(&mut disk,path_to_img);
    }

    // Remove password from a file
    if let Some(cmd) = matches.subcommand_matches("unprotect") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.unprotect(path_in_img)?;
        return a2kit::save_img(&mut disk,path_to_img);
    }
    
    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.delete(&path_in_img)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Lock a file or directory
    if let Some(cmd) = matches.subcommand_matches("lock") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.lock(&path_in_img)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Unlock a file or directory
    if let Some(cmd) = matches.subcommand_matches("unlock") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.unlock(&path_in_img)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Rename a file or directory
    if let Some(cmd) = matches.subcommand_matches("rename") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let name = cmd.get_one::<String>("name").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.rename(&path_in_img,&name)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Retype a file
    if let Some(cmd) = matches.subcommand_matches("retype") {
        let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
        let path_in_img = cmd.get_one::<String>("file").expect(RCH);
        let typ = cmd.get_one::<String>("type").expect(RCH);
        let aux = cmd.get_one::<String>("aux").expect(RCH);
        let mut disk = a2kit::create_fs_from_file(&path_to_img)?;
        disk.retype(&path_in_img,&typ,&aux)?;
        return a2kit::save_img(&mut disk,&path_to_img);
    }

    // Put file inside disk image, or save to local
    if let Some(cmd) = matches.subcommand_matches("put") {
        return commands::put::put(cmd);
    }

    // Get file from local or from inside a disk image
    if let Some(cmd) = matches.subcommand_matches("get") {
        return commands::get::get(cmd);
    }
    
    // Put JSON list of file images inside a disk image
    if let Some(cmd) = matches.subcommand_matches("mput") {
        return commands::put::mput(cmd);
    }

    // Get JSON list of file images from inside a disk image
    if let Some(cmd) = matches.subcommand_matches("mget") {
        return commands::get::mget(cmd);
    }

    // Pack data into a file image
    if let Some(cmd) = matches.subcommand_matches("pack") {
        return commands::put::pack(cmd);
    }

    // Unpack data from a file image
    if let Some(cmd) = matches.subcommand_matches("unpack") {
        return commands::get::unpack(cmd);
    }
    
    log::error!("No subcommand was found, try `a2kit --help`");
    return Err(Box::new(CommandError::InvalidCommand));
}

