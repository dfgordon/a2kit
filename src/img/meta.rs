//! # Disk Image Metadata Handling
//! 
//! Some disk images wrap the track data in a metadata structure.
//! This module contains machinery for exposing this metadata to the user.
//! One specific goal is to provide an object representing the metadata that
//! can be passed through the CLI pipeline, e.g., we may want to get some
//! subset of the metadata from one image and put it in another.
//! 
//! The main structural concept is that of a tree.  The tree is exposed
//! through the CLI as a JSON string.  A specific value is referenced using the
//! native JSON, e.g., `{"woz2":{"info":{"sides":"01"}}}`, while a location is
//! referenced as a list of `&str`, e.g., `["woz2","info","sides"]`.
//! A path notation is supported for user interaction, e.g., `/woz2/info/sides`.
//!
//! Binary is encoded as hex strings, however, there is an option to break a
//! value out into `_raw` and `_pretty` values, where the `_pretty` value can
//! be put as decimal, given units, etc..
//! 
//! This module exports several `macro_rules` to streamline metadata handling code.
//! These macros will take an arbitrary identifier chain, such as `self.info.sides`,
//! and resolve it into the appropriate path or JSON fragment.

use log::error;
use crate::img::{Error,DiskImageType};
use crate::STDRESULT;

/// Get a byte value from the image into a JSON object as a hex string.
/// ```rs
/// getByte!(root:JsonValue,image_type:String,self.path.to.byte:u8)
/// ```
#[macro_export]
macro_rules! getByte {
    ($root:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        $root[&$typ]$([stringify!($x)])+ = json::JsonValue::String(hex::ToHex::encode_hex(&vec![$slf.$($x).+]))
    };
}

/// Get a byte value from the image into a JSON object, adding the `_raw` terminus.
/// The `_pretty` terminus has to be added by hand.
/// ```rs
/// getByteEx!(root:JsonValue,image_type:String,self.path.to.byte:u8)
/// ```
#[macro_export]
macro_rules! getByteEx {
    ($root:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        $root[&$typ]$([stringify!($x)])+ = json::JsonValue::new_object();
        $root[&$typ]$([stringify!($x)])+["_raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&vec![$slf.$($x).+]))
    };
}

/// get a multi-byte value from the image into a JSON object as hex string
/// ```rs
/// getHex!(root:JsonValue,image_type:String,self.path.to.bytes:[u8])
/// ```
#[macro_export]
macro_rules! getHex {
    ($root:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        $root[&$typ]$([stringify!($x)])+ = json::JsonValue::String(hex::ToHex::encode_hex(&$slf.$($x).+))
    };
}

/// Get a multi-byte value from the image into a JSON object, adding the `_raw` terminus.
/// The `_pretty` terminus has to be added by hand.
/// ```rs
/// getHexEx!(root:JsonValue,image_type:String,self.path.to.bytes:[u8])
/// ```
#[macro_export]
macro_rules! getHexEx {
    ($root:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        $root[&$typ]$([stringify!($x)])+ = json::JsonValue::new_object();
        $root[&$typ]$([stringify!($x)])+["_raw"] = json::JsonValue::String(hex::ToHex::encode_hex(&$slf.$($x).+))
    };
}

/// Parse a hex string containing one byte and put value into the image using the given key path.
/// ```rs
/// putByte!(val:&str,key_path:&Vec<String>,image_type:String,self.path.to.byte:u8)
/// ```
#[macro_export]
macro_rules! putByte {
    ($val:ident,$key:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        if meta::match_key($key,&[&$typ,$(stringify!($x)),+]) {
            return meta::set_metadata_byte($val, &mut $slf.$($x).+);
        }
    };
}

/// Parse a hex string containing multiple bytes and put value into the image using the given key path.
/// ```rs
/// putHex!(val:&str,key_path:&Vec<String>,image_type:String,self.path.to.bytes:[u8])
/// ```
#[macro_export]
macro_rules! putHex {
    ($val:ident,$key:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        if meta::match_key($key,&[&$typ,$(stringify!($x)),+]) {
            return meta::set_metadata_hex($val, &mut $slf.$($x).+);
        }
    };
}

/// Put a variable length UTF8 string into the image using the given key path.
/// ```rs
/// putString!(val:&str,key_path:&Vec<String>,image_type:String,self.path.to.string:String)
/// ```
#[macro_export]
macro_rules! putString {
    ($val:ident,$key:ident,$typ:ident,$slf:ident.$($x:ident).+) => {
        if meta::match_key($key,&[&$typ,$(stringify!($x)),+]) {
            $slf.$($x).+ = $val.to_string();
            return Ok(());
        }
    };
}

/// Put a fixed length UTF8 string into the image using the given key path.
/// ```rs
/// putString!(val:&str,key_path:&Vec<String>,image_type:String,self.path.to.buf:[u8],pad:u8)
/// ```
#[macro_export]
macro_rules! putStringBuf {
    ($val:ident,$key:ident,$typ:ident,$slf:ident.$($x:ident).+,$pad:expr) => {
        if meta::match_key($key,&[&$typ,$(stringify!($x)),+]) {
            return meta::set_metadata_utf8($val,&mut $slf.$($x).+,$pad);
        }
    };
}

/// Test a key against a slice of &str.  The `test_path` does not need to
/// and should not include the optional `_raw` key.
pub fn match_key(key_path: &[String],test_path: &[&str]) -> bool {
    let pad = match key_path.last() {
        Some(last) if last=="_raw" => 1,
        _ => 0
    };
    if key_path.len()!=test_path.len()+pad {
        return false;
    }
    for i in 0..test_path.len() {
        if key_path[i]!=test_path[i] {
            return false;
        }
    }
    true
}

/// Test the key for match to the image type.  This relies on the protocol that
/// all metadata has a root key corresponding to the string representation of
/// the `DiskImageType`, e.g., every WOZ v1 key starts with `woz1`.
pub fn test_metadata(key_path: &[String], typ: DiskImageType) -> STDRESULT {
    let mut node = key_path.iter();
    match node.next() {
        Some(key) if key==&typ.to_string() => Ok(()),
        _ => {
            error!("metadata root did not match `{}`",typ.to_string());
            Err(Box::new(Error::MetadataMismatch))
        }
    }
}

/// Set a binary metadata value using a hex string
pub fn set_metadata_hex(hex_val: &str, buf: &mut [u8]) -> STDRESULT {
    match hex::decode_to_slice(hex_val, buf) {
        Ok(()) => Ok(()),
        Err(e) => Err(Box::new(e))
    }
}

/// Set a byte metadata value using a hex string
pub fn set_metadata_byte(hex_val: &str, buf: &mut u8) -> STDRESULT {
    let mut slice: [u8;1] = [0];
    match hex::decode_to_slice(hex_val, &mut slice) {
        Ok(()) => { *buf = slice[0]; Ok(()) },
        Err(e) => Err(Box::new(e))
    }
}

/// Fill a fixed length metadata buffer with a UTF8 string.
/// Pad with `pad` when `buf` is longer than `utf8_val`.
/// Return error if `buf` cannot hold the string.
pub fn set_metadata_utf8(utf8_val: &str, buf: &mut [u8], pad: u8) -> STDRESULT {
    let bytes = utf8_val.as_bytes();
    if bytes.len()<=buf.len() {
        for i in 0..bytes.len() {
            buf[i] = bytes[i];
        }
        for i in bytes.len()..buf.len() {
            buf[i] = pad;
        }
        Ok(())
    } else {
        Err(Box::new(Error::MetadataMismatch))
    }
}

