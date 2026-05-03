//! Module for handling random access text

use std::fmt;
use std::str::FromStr;
use std::collections::HashMap;
use super::{FileImage,Records,TextConversion,Error};
use crate::{STDRESULT,DYNERR};

/// See if `fimg` can be interpreted as records, and if so return the record length.
/// The converter is only used to decide whether we should expect postive or negative ASCII.
/// It is always assumed the record separator is a run of ascii null.
/// This basically relies on finding a run of contiguous records with
/// clean runs of separators between every record.
fn deduce_record_length(fimg: &FileImage,converter: &impl TextConversion) -> Option<usize> {
    let mut l = usize::MAX; // current estimate of record length
    let mut num = 0; // number of contiguous records found
    let mut pos = 0; // position in tentative record
    let mut in_fields = false;
    let lims = match converter.to_utf8(&[10]) {
        None => [129,255],
        Some(s) if s.as_str()=="\u{0000}" => [129,255],
        _ => [1,127]
    };
    let Some(last_chunk) = fimg.chunks.keys().map(|x| *x).max() else { return None; };
    for k in 0..=last_chunk {
        let chunk = match fimg.chunks.get(&k) {
            Some(c) => c.clone(),
            None => vec![0;fimg.chunk_len]
        };
        for b in chunk {
            if in_fields && b == 0 {
                in_fields = false;
                pos += 1;
            } else if in_fields && b > lims[0] && b < lims[1] {
                pos += 1;
            } else if !in_fields && b > lims[0] && b < lims[1]  {
                in_fields = true;
                num += 1;
                if pos > 0 {
                    if l == usize::MAX {
                        l = pos;
                    } else if pos < l && l % pos == 0 {
                        l = pos;
                    } else if pos < l && l % pos != 0 {
                        return None;
                    } else if pos > l && pos % l != 0 {
                        return None;
                    }
                }
                pos = 1; // pointer is after character we have found
            } else if !in_fields && b == 0 {
                pos += 1;
            } else {
                return None;
            }
        }
    }
    if num > 3 && l > 2 && l <= 0xffff {
        Some(l)
    } else {
        None
    }
}

impl Records {
    pub fn new(record_len: usize) -> Self {
        Self {
            record_len,
            map: HashMap::new()
        }
    }
    /// see if the slice is a JSON string with fimg_type = rec
    pub fn test(dat: &[u8]) -> bool {
        match str::from_utf8(dat) {
            Ok(s) => match json::parse(s) {
                Ok(parsed) => {
                    match parsed["fimg_type"].as_str() {
                        Some(val) => val == "rec",
                        None => false
                    }
                },
                _ => false
            },
            _ => false
        }
    }
    /// add a string as record number `num`, fields should be separated by LF or CRLF.
    pub fn add_record(&mut self,num: usize,fields: &str) {
        self.map.insert(num,fields.to_string());
    }
    /// Derive records from file image, this should find any real record, but may also find spurious ones.
    /// This is due to fundamental non-invertibility of the A2 file system's random access storage pattern.
    /// This routine assumes ASCII null terminates any record.  If `record_length` is 0 an analysis is
    /// triggered that tries to match a random access text pattern against rules deduced from the `converter` object.
    /// If the matching succeeds the Records are returned, if not an error is returned.
    pub fn from_fimg(fimg: &FileImage,mut record_length: usize,converter: impl TextConversion) -> Result<Records,DYNERR> {
        // TODO: allow for record separators other than ascii null
        if record_length == 0 {
            record_length = match deduce_record_length(fimg,&converter) {
                Some(l) => l,
                None => return Err(Box::new(Error::FileFormat)) // don't want a log message here
            };
        }
        if record_length < 2 || record_length > 0xffff {
            log::error!("refusing record length {}",record_length);
            return Err(Box::new(Error::FileFormat));
        }
        let mut ans = Records::new(record_length);
        let mut list: Vec<usize> = Vec::new();
        // add record index for each starting record boundary that falls within a chunk
        let chunk_len = fimg.chunk_len;
        for c in fimg.chunks.keys() {
            let start_rec = c*chunk_len/record_length + match c*chunk_len%record_length { x if x>0 => 1, _ => 0 };
            let end_rec = (c+1)*chunk_len/record_length + match (c+1)*chunk_len%record_length { x if x>0 => 1, _ => 0 };
            for r in start_rec..end_rec {
                list.push(r);
            }
        }
        // add only records with complete data
        for r in list {
            let start_chunk = r*record_length/chunk_len;
            let end_chunk = 1 + (r+1)*record_length/chunk_len;
            let start_offset = r*record_length%chunk_len;
            let mut bytes: Vec<u8> = Vec::new();
            let mut complete = true;
            for chunk_num in start_chunk..end_chunk {
                match fimg.chunks.get(&chunk_num) {
                    Some(chunk) => {
                       for i in chunk {
                            bytes.push(*i);
                        }
                    },
                    _ => complete = false
                }
            }
            if complete && start_offset < bytes.len() {
                let actual_end = usize::min(start_offset+record_length,bytes.len());
                if let Some(long_str) = converter.to_utf8(&bytes[start_offset..actual_end].to_vec()) {
                    if let Some(partial) = long_str.split("\u{0000}").next() {
                        if partial.len()>0 {
                            ans.map.insert(r,partial.to_string());
                        }
                    } else {
                        if long_str.len()>0 {
                            ans.map.insert(r,long_str);
                        }
                    }
                }
            }
        }
        return Ok(ans);
    }
    /// Update a file image's data using the records, this is usually done before writing to a disk image.
    /// This will set the file image's eof, but no other metadata.
    pub fn update_fimg(&self,ans: &mut FileImage,require_first: bool,converter: impl TextConversion,clear: bool) -> STDRESULT {
        if self.record_len < 2 || self.record_len > 0xffff {
            log::error!("refusing record length {}",self.record_len);
            return Err(Box::new(Error::FileFormat));
        }
        if clear {
            ans.chunks = HashMap::new();
        }
        let chunk_len = ans.chunk_len;
        let mut eof: usize = 0;
        // always need to have the first chunk referenced on ProDOS
        if require_first {
            ans.chunks.insert(0,vec![0;chunk_len]);
        }
        let mut next_buf = |old_chunk: usize,old_buf: Option<Vec<u8>>| -> Vec<u8> {
            let mut chunk = old_chunk;
            if let Some(old) = old_buf {
                ans.chunks.insert(chunk,old);
                chunk += 1;
            }
            match ans.chunks.get_mut(&chunk) {
                Some(v) => v.clone(),
                None => Vec::new()
            }
        };
        // now insert the actual records, first chunk can always be overwritten
        for (rec_num,fields) in &self.map {
            match converter.from_utf8(fields) {
                Some(data_bytes) => {
                    if data_bytes.len() > self.record_len {
                        log::warn!("record {} is too long and will corrupt other records",rec_num);
                        //log::warn!("truncating record {} to maximum {}",rec_num,self.record_len);
                        //data_bytes = data_bytes[0..self.record_len].to_vec();
                    }
                    let mut chunk = self.record_len * rec_num / chunk_len;
                    let mut offset = self.record_len * rec_num % chunk_len;
                    let mut buf = next_buf(chunk,None);
                    for i in 0..data_bytes.len() {
                        if offset >= buf.len() {
                            for _j in buf.len()..offset {
                                buf.push(0);
                            }
                            buf.push(data_bytes[i]);
                        } else {
                            buf[offset] = data_bytes[i];
                        }
                        offset += 1;
                        if offset >= chunk_len || i+1 == data_bytes.len() {
                            eof = usize::max(chunk*chunk_len + buf.len(),eof);
                            buf = next_buf(chunk,Some(buf));
                            chunk += 1;
                            offset = 0;
                        }
                    }
                },
                None => return Err(Box::new(std::fmt::Error))
            }
        }
        ans.set_eof(eof);
        return Ok(());
    }
    /// Get records from the JSON string representation
    pub fn from_json(json_str: &str) -> Result<Records,DYNERR> {
        match json::parse(json_str) {
            Ok(parsed) => {
                let maybe_type = parsed["fimg_type"].as_str();
                let maybe_len = parsed["record_length"].as_usize();
                if let (Some(typ),Some(len)) = (maybe_type,maybe_len) {
                    if typ=="rec" {
                        let mut records: HashMap<usize,String> = HashMap::new();
                        let map_obj = &parsed["records"];
                        if map_obj.entries().len()==0 {
                            log::error!("no object entries in json records");
                            return Err(Box::new(Error::FileImageFormat));
                        }
                        for (key,lines) in map_obj.entries() {
                            if let Ok(num) = usize::from_str(key) {
                                let mut fields = String::new();
                                for maybe_field in lines.members() {
                                    if let Some(line) = maybe_field.as_str() {
                                        fields = fields + line + "\n";
                                    } else {
                                        log::error!("record is not a string");
                                        return Err(Box::new(Error::FileImageFormat));
                                    }
                                }
                                records.insert(num,fields);
                            } else {
                                log::error!("key is not a number");
                                return Err(Box::new(Error::FileImageFormat));
                            }
                        }
                        return Ok(Self {
                            record_len: len,
                            map: records
                        });    
                    } else {
                        log::error!("json metadata type mismatch");
                        return Err(Box::new(Error::FileImageFormat));
                    }
                }
                log::error!("json records missing metadata");
                Err(Box::new(Error::FileImageFormat))
            },
            Err(_e) => Err(Box::new(Error::FileImageFormat))
        } 
    }
    /// Put records into the JSON string representation, if indent=0 use unpretty form
    pub fn to_json(&self,indent: Option<u16>) -> String {
        let mut json_map = json::JsonValue::new_object();
        for (r,l) in &self.map {
            let mut json_array = json::JsonValue::new_array();
            for line in l.lines() {
                json_array.push(line).expect("error while building JSON array");
            }
            json_map[r.to_string()] = json_array;
        }
        let ans = json::object! {
            fimg_type: "rec",
            record_length: self.record_len,
            records: json_map
        };
        if let Some(spaces) = indent {
            return json::stringify_pretty(ans, spaces);
        } else {
            return json::stringify(ans);
        }
    }
}

/// Allows the records to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
impl fmt::Display for Records {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx,fields) in &self.map {
            write!(f,"Record {}",idx).expect("format error");
            for field in fields.lines() {
                write!(f,"    {}",field).expect("format error");
            }
        }
        write!(f,"Record Count = {}",self.map.len())
    }
}
