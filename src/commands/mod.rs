//! # CLI Subcommands
//! 
//! Contains modules that run the subcommands.

pub mod mkdsk;
pub mod put;
pub mod get;
pub mod get_img;
pub mod put_img;

use std::str::FromStr;

#[derive(thiserror::Error,Debug)]
pub enum CommandError {
    #[error("Item type is not yet supported")]
    UnsupportedItemType,
    #[error("Item type is unknown")]
    UnknownItemType,
    #[error("Command could not be interpreted")]
    InvalidCommand,
    #[error("One of the parameters was out of range")]
    OutOfRange,
    #[error("Input source is not supported")]
    UnsupportedFormat,
    #[error("Input source could not be interpreted")]
    UnknownFormat,
    #[error("File not found")]
    FileNotFound
}

/// Types of files that may be distinguished by the file system or a2kit.
/// This will have to be mapped to a similar enumeration at lower levels
/// in order to obtain the binary type code.
#[derive(PartialEq,Clone,Copy)]
pub enum ItemType {
    Raw,
    Binary,
    Text,
    Records,
    FileImage,
    ApplesoftText,
    IntegerText,
    MerlinText,
    ApplesoftTokens,
    IntegerTokens,
    MerlinTokens,
    ApplesoftVars,
    IntegerVars,
    Block,
    Track,
    Sector,
    RawTrack,
    System
}

impl FromStr for ItemType {
    type Err = CommandError;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "raw" => Ok(Self::Raw),
            "bin" => Ok(Self::Binary),
            "txt" => Ok(Self::Text),
            "rec" => Ok(Self::Records),
            "any" => Ok(Self::FileImage),
            "atxt" => Ok(Self::ApplesoftText),
            "itxt" => Ok(Self::IntegerText),
            "mtxt" => Ok(Self::MerlinText),
            "atok" => Ok(Self::ApplesoftTokens),
            "itok" => Ok(Self::IntegerTokens),
            "mtok" => Ok(Self::MerlinTokens),
            "avar" => Ok(Self::ApplesoftVars),
            "ivar" => Ok(Self::IntegerVars),
            "block" => Ok(Self::Block),
            "track" => Ok(Self::Track),
            "raw_track" => Ok(Self::RawTrack),
            "sec" => Ok(Self::Sector),
            "sys" => Ok(Self::System),
            _ => Err(CommandError::UnknownItemType)
        }
    }
}

