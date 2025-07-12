//! ## Language Analysis and Transformations

use std::str::FromStr;
use std::io::{Read,Write};
use clap::parser::ValueSource;
use colored::Colorize;
use super::{ItemType,CommandError};
use crate::lang;
use crate::lang::applesoft;
use crate::lang::integer;
use crate::lang::merlin;
use crate::lang::server::Analysis;
use crate::STDRESULT;
const RCH: &str = "unreachable was reached";

pub fn verify(cmd: &clap::ArgMatches) -> STDRESULT {
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
    if doc.text.len()==0 {
        log::error!("verify was handed an empty string");
        return Err(Box::new(CommandError::InvalidCommand));
    }
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

pub fn minify(cmd: &clap::ArgMatches) -> STDRESULT {
    if atty::is(atty::Stream::Stdin) {
        log::error!("line entry is not supported for `minify`, please pipe something in");
        return Err(Box::new(CommandError::InvalidCommand));
    }
    let typ = ItemType::from_str(cmd.get_one::<String>("type").expect(RCH));
    let externals = match cmd.try_get_many::<i64>("extern") {
        Ok(Some(ans)) => ans.map(|x| *x as usize).collect::<Vec<_>>(),
        Ok(None) => vec![],
        Err(e) => {
            log::error!("{}",e);
            return Err(Box::new(e));
        }
    };
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
            minifier.set_external_refs(externals);
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

pub fn renumber(cmd: &clap::ArgMatches) -> STDRESULT {
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
    let externals = match cmd.try_get_many::<i64>("extern") {
        Ok(Some(ans)) => ans.map(|x| *x as usize).collect::<Vec<_>>(),
        Ok(None) => vec![],
        Err(e) => {
            log::error!("{}",e);
            return Err(Box::new(e));
        }
    };
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
            renumberer.set_external_refs(externals);
            renumberer.set_flags(match reorder {true => 1, false => 0});
            let new_prog = renumberer.renumber(&program,beg,end,first,step)?;
            println!("{}",&new_prog);
            Ok(())
        },
        Ok(ItemType::IntegerText) => {
            lang::verify_str(tree_sitter_integerbasic::language(), &program)?;
            let mut renumberer = integer::renumber::Renumberer::new();
            renumberer.set_external_refs(externals);
            renumberer.set_flags(match reorder {true => 1, false => 0});
            let new_prog = renumberer.renumber(&program,beg,end,first,step)?;
            log::warn!("line number expressions must be manually adjusted");
            println!("{}",&new_prog);
            Ok(())
        }
        _ => Err(Box::new(CommandError::UnsupportedItemType))
    };
}

pub fn tokenize(cmd: &clap::ArgMatches) -> STDRESULT {
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
                if atty::is(atty::Stream::Stdout) || cmd.get_flag("console") {
                    crate::display_block(addr as usize,&object);
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
            if atty::is(atty::Stream::Stdout) || cmd.get_flag("console") {
                crate::display_block(0,&object);
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
            if atty::is(atty::Stream::Stdout) || cmd.get_flag("console") {
                crate::display_block(0,&object);
            } else {
                std::io::stdout().write_all(&object).expect("could not write output stream");
            }
            Ok(())
        },
        _ => Err(Box::new(CommandError::UnsupportedItemType))
    };
}

pub fn detokenize(cmd: &clap::ArgMatches) -> STDRESULT {
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

pub fn asm(cmd: &clap::ArgMatches) -> STDRESULT {
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
        if atty::is(atty::Stream::Stdout) || cmd.get_flag("console") {
            crate::display_block(0,&object);
        } else {
            std::io::stdout().write_all(&object).expect("could not write output stream");
        }
        return Ok(());
    } else {
        eprintln!("\u{2717} {} {}",err.to_string().red(),"errors".red());
        return Err(Box::new(lang::Error::Syntax));
    }
}

pub fn dasm(cmd: &clap::ArgMatches) -> STDRESULT {
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

