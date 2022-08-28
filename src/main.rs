use clap::{arg,Command};
use thiserror::Error;
use std::str::FromStr;
use std::io::{Read,Write};
mod walker;
mod applesoft;
mod integer;
mod dos33;

const RCH: &str = "unreachable was reached";

#[derive(PartialEq)]
enum DiskImageType {
    DO,
    PO,
    WOZ
}

#[derive(PartialEq)]
enum ItemType {
    Binary,
    Text,
    ApplesoftText,
    IntegerText,
    ApplesoftTokens,
    IntegerTokens,
    ApplesoftVars,
    IntegerVars,
}

#[derive(Error,Debug)]
pub enum CommandError {
    #[error("Item type is not yet supported")]
    UnsupportedItemType,
    #[error("Item type is unknown")]
    UnknownItemType,
    #[error("Command could not be interpreted")]
    InvalidCommand,
    #[error("One of the parameters was out of range")]
    OutOfRange
}

impl FromStr for DiskImageType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "do" => Ok(Self::DO),
            "po" => Ok(Self::PO),
            "woz" => Ok(Self::WOZ),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

impl FromStr for ItemType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "atxt" => Ok(Self::ApplesoftText),
            "itxt" => Ok(Self::IntegerText),
            "atok" => Ok(Self::ApplesoftTokens),
            "itok" => Ok(Self::IntegerTokens),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}


fn main() -> Result<(),Box<dyn std::error::Error>>
{
    let long_help =
"This tool is intended to be used with redirection and pipes.

Examples:
Applesoft line entry checker: `a2kit verify -t atxt`
Applesoft error checker: `a2kit verify -t atxt < myprog.bas`
create disk image: `a2kit create -v 254 -t do > myimg.do`
Tokenize to file: `a2kit tokenize -a 2049 -t atxt < prog.bas > prog.atok
Tokenize to image: `a2kit tokenize -a 2049 -t atxt < prog.bas \\
                    | a2kit put -f PROG -t atok -d myimg.do`";

    let matches = Command::new("a2kit")
        .about("Manipulates Apple II files and disk images, with language comprehension.")
    .after_long_help(long_help)
    .subcommand(Command::new("create")
        .arg(arg!(-v --volume <VOLUME> "volume name or number"))
        .arg(arg!(-t --type <TYPE> "type of disk image to create").possible_values(["do"]))
        .about("create a blank disk image"))
    .subcommand(Command::new("verify")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt"]))
        .about("error check source files"))
    .subcommand(Command::new("get")
        .arg(arg!(-f --file <PATH> "path inside disk image of file to get"))
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atok","itok","bin","txt"]))
        .about("extract file from disk image"))
    .subcommand(Command::new("put")
        .arg(arg!(-f --file <PATH> "path inside disk image of file to put"))
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atok","itok","bin","txt"]))
        .arg(arg!(-d --dimg <PATH> "path to disk image itself"))
        .arg(arg!(-a --addr <ADDRESS> "address of binary file").required(false))
        .about("add file to disk image, n.b. disk image is not stdout"))
    .subcommand(Command::new("catalog")
        .about("display disk image catalog"))
    .subcommand(Command::new("tokenize")
        .arg(arg!(-a --addr <ADDRESS> "address of tokenized code (Applesoft only)").required(false))
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atxt","itxt"]))
        .about("tokenize BASIC source file or line entry"))
    .subcommand(Command::new("detokenize")
        .arg(arg!(-t --type <TYPE> "type of the file").possible_values(["atok","itok"]))
        .about("detokenize a BASIC file"))
    .get_matches();
    
    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("create") {
        match DiskImageType::from_str(cmd.value_of("type").expect(RCH)).unwrap() {
            DiskImageType::DO => match u8::from_str(cmd.value_of("volume").expect(RCH)) {
                Ok(vol) if vol>=1 && vol<=254 => {
                    let mut disk = dos33::Disk::new();
                    disk.format(254,true);
                    let buf = disk.to_do_img();
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
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
            DiskImageType::WOZ => {
                return Err(Box::new(CommandError::UnsupportedItemType));
            }
        };
    }

    // Catalog a disk image
    if let Some(cmd) = matches.subcommand_matches("catalog") {
        if atty::is(atty::Stream::Stdin) {
            panic!("redirect a disk image to stdin");
        }
        //let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk_img_data = Vec::new();
        std::io::stdin().read_to_end(&mut disk_img_data).expect("failed to read input stream");
        let disk = dos33::Disk::from_do_img(&disk_img_data);
        disk.catalog_to_stdout();
        return Ok(())
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
                Ok(source) => {
                    println!("{}",source);
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
            panic!("line entry is not supported for tokenize, redirect a file to stdin");
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
            panic!("line entry is not supported for detokenize, redirect a file to stdin");
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

    // Put file inside disk image
    if let Some(cmd) = matches.subcommand_matches("put") {
        if atty::is(atty::Stream::Stdin) {
            panic!("line entry is not supported for put, redirect a file to stdin");
        }
        if !atty::is(atty::Stream::Stdout) {
            panic!("output redirection is not supported for put, use -d to specify disk image");
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let path_to_img = String::from(cmd.value_of("dimg").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let load_address: u16 = match (cmd.value_of("addr"),&typ) {
            (Some(a),_) => u16::from_str(a).expect("bad address"),
            (_ ,Ok(ItemType::Binary)) => panic!("binary file requires an address"),
            _ => 768 as u16
        };
        let mut file_data = Vec::new();
        std::io::stdin().read_to_end(&mut file_data).expect("failed to read input stream");
        let disk_img_data = std::fs::read(&path_to_img);
        let mut disk = dos33::Disk::new();
        match disk_img_data {
            Ok(v) => {
                disk = dos33::Disk::from_do_img(&v);
            },
            Err(_e) => {
                eprintln!("disk image not found, creating blank");
                disk.format(254,true);
            }
        }
        match typ {
            Ok(ItemType::ApplesoftTokens) => {
                disk.save(&path_in_img,&file_data,dos33::Type::Applesoft).expect("disk image error");
            }
            Ok(ItemType::IntegerTokens) => {
                disk.save(&path_in_img,&file_data,dos33::Type::Integer).expect("disk image error");
            }
            Ok(ItemType::Binary) => {
                disk.bsave(&path_in_img,&file_data,load_address).expect("disk image error");
            }
            Ok(ItemType::Text) => {
                disk.write_text(&path_in_img,&file_data).expect("disk image error");
            }
            _ => {
                panic!("not supported");
            }
        }
        let updated_img_data = disk.to_do_img();
        std::fs::write(&path_to_img,updated_img_data).expect("could not write disk image to disk");
        return Ok(())
    }

    // Get file from inside disk image
    if let Some(cmd) = matches.subcommand_matches("get") {
        if atty::is(atty::Stream::Stdin) {
            panic!("line entry is not supported for put, redirect a file to stdin");
        }
        let typ = ItemType::from_str(cmd.value_of("type").expect(RCH));
        let path_in_img = String::from(cmd.value_of("file").expect(RCH));
        let mut disk_img_data = Vec::new();
        std::io::stdin().read_to_end(&mut disk_img_data).expect("failed to read input stream");
        let disk = dos33::Disk::from_do_img(&disk_img_data);
        let object = match typ {
            Ok(ItemType::ApplesoftTokens) => disk.load(&path_in_img).expect("disk image error"),
            Ok(ItemType::IntegerTokens) => disk.load(&path_in_img).expect("disk image error"),
            Ok(ItemType::Binary) => disk.bload(&path_in_img).expect("disk image error").1,
            Ok(ItemType::Text) => disk.read_text(&path_in_img).expect("disk image error"),
            _ => panic!("not supported")
        };
        if atty::is(atty::Stream::Stdout) {
            match typ {
                Ok(ItemType::Text) => {
                    println!("{}",dos33::types::SequentialText::pack(&object));
                },
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
            std::io::stdout().write_all(&object).expect("could not write output stream");
        }
        return Ok(())
    }
    
    eprintln!("No subcommand was found");
    return Err(Box::new(CommandError::InvalidCommand));

}
