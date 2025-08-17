//! # Command Line Interface
//! 
//! Dispatch commands to `commands` module.

use env_logger;
#[cfg(windows)]
use colored;
use a2kit::commands;
use a2kit::commands::CommandError;
mod cli;

fn main() -> Result<(),Box<dyn std::error::Error>>
{
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let main_cmd = cli::build_cli();
    let matches = main_cmd.get_matches();

    // Completions

    if let Some(cmd) = matches.subcommand_matches("completions") {
        return commands::completions::generate(cli::build_cli(),cmd);
    }

    // Create a disk image

    if let Some(cmd) = matches.subcommand_matches("mkdsk") {
        return commands::mkdsk::mkdsk(cmd);
    }

    // Catalog a disk image

    if let Some(cmd) = matches.subcommand_matches("catalog") {
        return commands::stat::catalog(cmd);
    }
    
    // Output the directory tree as a JSON string

    if let Some(cmd) = matches.subcommand_matches("tree") {
        return commands::stat::tree(cmd);
    }

    // Output the matches to the glob pattern

    if let Some(cmd) = matches.subcommand_matches("glob") {
        return commands::stat::glob(cmd);
    }

    // Output the FS stats as a JSON string

    if let Some(cmd) = matches.subcommand_matches("stat") {
        return commands::stat::stat(cmd);
    }
    
    // Output the disk geometry as a JSON string

    if let Some(cmd) = matches.subcommand_matches("geometry") {
        return commands::stat::geometry(cmd);
    }

    // Verify

    if let Some(cmd) = matches.subcommand_matches("verify") {
        return commands::langx::verify(cmd);
    }

    // Minify

    if let Some(cmd) = matches.subcommand_matches("minify") {
        return commands::langx::minify(cmd);
    }
    
    // Renumber

    if let Some(cmd) = matches.subcommand_matches("renumber") {
        return commands::langx::renumber(cmd);
    }
    
    // Tokenize BASIC or Encode Merlin

    if let Some(cmd) = matches.subcommand_matches("tokenize") {
        return commands::langx::tokenize(cmd);
    }

    // Detokenize BASIC or decode Merlin

    if let Some(cmd) = matches.subcommand_matches("detokenize") {
        return commands::langx::detokenize(cmd);
    }

    // Assemble source code

    if let Some(cmd) = matches.subcommand_matches("asm") {
        return commands::langx::asm(cmd);
    }

    // Disassemble binary to Merlin source

    if let Some(cmd) = matches.subcommand_matches("dasm") {
        return commands::langx::dasm(cmd);
    }

    // Create directory inside disk image
    if let Some(cmd) = matches.subcommand_matches("mkdir") {
        return commands::modify::mkdir(cmd);
    }

    // Change permissions for a file
    if let Some(cmd) = matches.subcommand_matches("access") {
        return commands::modify::access(cmd);
    }
    
    // Delete a file or directory
    if let Some(cmd) = matches.subcommand_matches("delete") {
        return commands::modify::delete(cmd);
    }

    // Rename a file or directory
    if let Some(cmd) = matches.subcommand_matches("rename") {
        return commands::modify::rename(cmd);
    }

    // Retype a file
    if let Some(cmd) = matches.subcommand_matches("retype") {
        return commands::modify::retype(cmd);
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

