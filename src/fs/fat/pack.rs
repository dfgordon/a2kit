//! ### FAT Packing Module
//! 
//! Functions to help pack or unpack dates, filenames, and passwords.

use chrono::{Timelike,Datelike};
use log::{debug,info,warn};
/// Characters forbidden from file names
pub const INVALID_CHARS: &str = "\"*+,./:;<=>?[\\]|";
pub const DOT: ([u8;8],[u8;3]) = ([b'.',32,32,32,32,32,32,32],[32,32,32]);
pub const DOTDOT: ([u8;8],[u8;3]) = ([b'.',b'.',32,32,32,32,32,32],[32,32,32]);
// TODO: how do we support old style extended character sets?
// For Kanji, if first name byte is 0x05 replace with extended character 0xe5.
//const KANJI: u8 = 0x05;

/// pack the date into the FAT format, if the year is not between 1980
/// and 2107 it will be pegged to the nearest representable date.
pub fn pack_date(time: Option<chrono::NaiveDateTime>) -> [u8;2] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    let year = match now.year() {
        y if y < 1980 => {
            warn!("date prior to reference date, pegging to reference date");
            1980
        },
        y if y > 2107 => {
            warn!("date is pegged to maximum of 2107");
            2107
        },
        y => y
    };

    let ans16 = now.day() as u16 + ((now.month() as u16) << 5) + ((year as u16 - 1980) << 9);
    return u16::to_le_bytes(ans16);
}

pub fn pack_time(time: Option<chrono::NaiveDateTime>) -> [u8;2] {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };

    let ans16 = (now.second() as u16) / 2 + ((now.minute() as u16) << 5) + ((now.hour() as u16) << 11);
    return u16::to_le_bytes(ans16);
}

pub fn pack_tenths(time: Option<chrono::NaiveDateTime>) -> u8 {
    let now = match time {
        Some(t) => t,
        _ => chrono::Local::now().naive_local()
    };
    (now.timestamp_subsec_millis() / 100) as u8 + 10*(now.second() % 2) as u8
}

pub fn unpack_date(fat_date: [u8;2]) -> Option<chrono::NaiveDate> {
    if fat_date==[0,0] {
        return None;
    }
    let date16 = u16::from_le_bytes(fat_date);
    let year = 1980 + (date16 >> 9) as i32;
    let month = ((date16 & 0b0000_0001_1110_0000) >> 5) as u32;
    let day = (date16 & 0b1_1111) as u32;
    chrono::NaiveDate::from_ymd_opt(year, month, day)
}

pub fn unpack_time(fat_time: [u8;2],tenths: u8) -> Option<chrono::NaiveTime> {
    let time16 = u16::from_le_bytes(fat_time);
    let hour = (time16 >> 11) as u32;
    let min = ((time16 & 0b0000_0111_1110_0000) >> 5) as u32;
    let sec2 = (time16 & 0b1_1111) as u32;
    chrono::NaiveTime::from_hms_opt(hour, min, sec2*2 + tenths as u32/10)
}

/// Accepts lower case, case is raised by string_to_file_name.
/// "." and ".." are not accepted here.
pub fn is_name_valid(s: &str) -> bool {
    let it: Vec<&str> = s.split('.').collect();
    if it.len()>2 {
        return false;
    }
    let base = it[0];
    let ext = match it.len() {
        1 => "",
        _ => it[1]
    };
    // TODO: handle extended chars like KANJI
    for char in [base,ext].concat().chars() {
        if !char.is_ascii() || INVALID_CHARS.contains(char) || char.is_ascii_control() {
            debug!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    if base.len()<1 || base.len()>8 {
        info!("base name length {} out of range",base.len());
        return false;
    }
    if ext.len()>3 {
        info!("extension name too long, max 3");
        return false;
    }
    true
}

/// Same as is_name_valid except dot is not needed or allowed
pub fn is_label_valid(s: &str) -> bool {
    if s.len()<1 || s.len()>11 {
        info!("label length {} out of range",s.len());
        return false;
    }
    // TODO: handle extended chars like KANJI
    for char in s.chars() {
        if !char.is_ascii() || INVALID_CHARS.contains(char) || char.is_ascii_control() {
            debug!("bad file name character `{}` (codepoint {})",char,char as u32);
            return false;
        }
    }
    true
}

/// Convert label bytes to an ASCII string.
/// Will not panic, will escape the string if necessary.
pub fn label_to_string(label: [u8;11]) -> String {
    match String::from_utf8(label.to_vec()) {
        Ok(l) => l.trim_end().to_string(),
        _ => {
            warn!("escaping invalid filename");
            crate::escaped_ascii_from_bytes(&label.to_vec(), true, false).trim_end().to_string()
        }
    }
}

/// Convert filename bytes to an ASCII string.
/// Dot and DotDot are specially handled.
/// Will not panic, will escape the string if necessary.
pub fn file_name_to_string(name: [u8;8], typ: [u8;3]) -> String {
    match (name,typ) {
        DOT => ".".to_string(),
        DOTDOT => "..".to_string(),
        _ => match (String::from_utf8(name.to_vec()),String::from_utf8(typ.to_vec())) {
            (Ok(b),Ok(x)) => [b.trim_end(),".",x.trim_end()].concat(),
            _ => {
                warn!("escaping invalid filename");
                [
                    crate::escaped_ascii_from_bytes(&name.to_vec(), true, false).trim_end(),
                    ".",
                    crate::escaped_ascii_from_bytes(&typ.to_vec(), true, false).trim_end()
                ].concat()
            }
        }
    }
}

/// Put the filename bytes as a split ASCII string (name,type).
/// Dot and DotDot are specially handled.
/// Will not panic, will escape the string if necessary.
pub fn file_name_to_split_string(name: [u8;8],typ: [u8;3]) -> (String,String) {
    match (name,typ) {
        DOT => (".".to_string(),"".to_string()),
        DOTDOT => ("..".to_string(),"".to_string()),
        _ => match (String::from_utf8(name.to_vec()),String::from_utf8(typ.to_vec())) {
            (Ok(b),Ok(x)) => (b.trim_end().to_string(),x.trim_end().to_string()),
            _ => {
                warn!("escaping invalid filename");
                (
                    crate::escaped_ascii_from_bytes(&name.to_vec(), true, false).trim_end().to_string(),
                    crate::escaped_ascii_from_bytes(&typ.to_vec(), true, false).trim_end().to_string()
                )
            }
        }
    }
}

/// Convert string to name and type bytes for directory.
/// Dot and DotDot are specially handled.
/// Assumes string contains a valid filename.
pub fn string_to_file_name(s: &str) -> ([u8;8],[u8;3]) {
    if s=="." {
        return DOT;
    }
    if s==".." {
        return DOTDOT;
    }
    let mut ans: ([u8;8],[u8;3]) = ([0;8],[0;3]);
    let upper = s.to_uppercase();
    let it: Vec<&str> = upper.split('.').collect();
    let base = it[0].as_bytes().to_vec();
    let ext = match it.len() {
        1 => Vec::new(),
        _ => it[1].as_bytes().to_vec()
    };
    for i in 0..8 {
        if i<base.len() {
            ans.0[i] = base[i];
        } else {
            ans.0[i] = 0x20;
        }
    }
    for i in 0..3 {
        if i<ext.len() {
            ans.1[i] = ext[i];
        } else {
            ans.1[i] = 0x20;
        }
    }
    return ans;
}

/// Convert label string to name and type bytes for directory.
/// Assumes string contains a valid label name.
pub fn string_to_label_name(s: &str) -> ([u8;8],[u8;3]) {
    let mut ans: ([u8;8],[u8;3]) = ([0;8],[0;3]);
    let upper = s.to_uppercase().as_bytes().to_vec();
    for i in 0..8 {
        if i<upper.len() {
            ans.0[i] = upper[i];
        } else {
            ans.0[i] = 0x20;
        }
    }
    for i in 8..11 {
        if i<upper.len() {
            ans.1[i-8] = upper[i];
        } else {
            ans.1[i-8] = 0x20;
        }
    }
    return ans;
}
