use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::str::FromStr;
use std::fmt;
use a2kit_macro::DiskStruct;
use crate::disk_base::TextEncoder;
use log::info;

pub const BLOCK_SIZE: usize = 512;
pub const TEXT_PAGE: usize = 1024;
pub const VOL_HEADER_BLOCK: usize = 2;
pub const ENTRY_SIZE: usize = 26;
pub const MAX_CAT_REPS: usize = 100;
pub const INVALID_CHARS: &str = " $=?,[#:";

/// Enumerates Pascal errors.  The `Display` trait will print the long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("parity error (CRC)")]
    BadBlock,
    #[error("bad device number")]
    BadDevNum,
    #[error("illegal operation")]
    BadMode,
    #[error("undefined hardware error")]
    Hardware,
    #[error("lost device")]
    LostDev,
    #[error("lost file")]
    LostFile,
    #[error("illegal filename")]
    BadTitle,
    #[error("insufficient space")]
    NoRoom,
    #[error("no device")]
    NoDev,
    #[error("no file")]
    NoFile,
    #[error("duplicate file")]
    DuplicateFilename,
    #[error("attempt to open already-open file")]
    NotClosed,
    #[error("attempt to access closed file")]
    NotOpen,
    #[error("error reading real or integer")]
    BadFormat,
    #[error("characters arriving too fast")]
    BufferOverflow,
    #[error("disk is write protected")]
    WriteProtected,
    #[error("failed to complete read or write")]
    DevErr
}

/// Map file type codes to strings for display
pub const TYPE_MAP_DISP: [(u8,&str);9] = [
    (0x00, "NONE"),
    (0x01, "BAD"),
    (0x02, "CODE"),
    (0x03, "TEXT"),
    (0x04, "INFO"),
    (0x05, "DATA"),
    (0x06, "GRAF"),
    (0x07, "FOTO"),
    (0x08, "SECURE")
];

/// Enumerates the seven basic file types, available conversions are:
/// * Type to u8: `as u8`
/// * u8 to Type: `FromPrimitive::from_u8`
/// * &str to Type: `Type::from_str`, str can be a number or mnemonic
#[derive(FromPrimitive)]
pub enum Type {
    Non = 0x00,
    Bad = 0x01,
    Code = 0x02,
    Text = 0x03,
    Info = 0x04,
    Data = 0x05,
    Graf = 0x06,
    Foto = 0x07,
    Secure = 0x08
}

impl FromStr for Type {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        // string can be the number itself
        if let Ok(num) = u8::from_str(s) {
            return match FromPrimitive::from_u8(num) {
                Some(typ) => Ok(typ),
                _ => Err(Error::BadMode)
            };
        }
        // or a mnemonic
        match s {
            "bin" => Ok(Self::Data),
            "txt" => Ok(Self::Text),
            "pcode" => Ok(Self::Code),
            _ => Err(Error::BadMode)
        }
    }
}

/// This is for convenience in testing.  Sometimes the emulator will pad the data with random bytes at the end.
/// We need a way to append these bytes without changing the length calculation for comparisons.
// fn append_junk(dat: &Vec<u8>,trailing: Option<&Vec<u8>>) -> Vec<u8> {
//     match trailing {
//         Some(v) => [dat.clone(),v.clone()].concat(),
//         None => dat.clone()
//     }
// }

/// Pascal text is +ASCII, split into 1024 byte pages padded with nulls.
/// Newline is 0x0d.
/// ASCII 0x10 indicates the next byte is an indentation count + 0x20.
pub struct Encoder {
    line_terminator: Option<u8>
}

/// if we moved past a page boundary go back and pad with nulls after the last CR,
/// and move remainder text to the next page.  Return Ok(new page number) or Error
fn paginate(ans: &mut Vec<u8>,page: usize,count_on_page: usize) -> Result<usize,Error> {
    if count_on_page >= TEXT_PAGE {
        let offset = page*TEXT_PAGE;
        for i in (0..TEXT_PAGE).rev() {
            if ans[offset+i]==0x0d {
                for _j in 0..1023-i {
                    ans.insert(offset+i+1,0);
                }
                return Ok(page+1);
            }
        }
        return Err(Error::BadFormat);
    }
    return Ok(page);
}

impl TextEncoder for Encoder {
    fn new(line_terminator: Option<u8>) -> Self {
        Self {
            line_terminator
        }
    }
    fn encode(&self,txt: &str) -> Option<Vec<u8>> {
        info!("encoding text");
        let src: Vec<u8> = txt.as_bytes().to_vec();
        let mut ans: Vec<u8> = Vec::new();
        let mut starting_line = true;
        let mut indenting = 0;
        let mut page = 0;
        let mut count_on_page: usize = 0;
        for i in 0..src.len() {
            // handle CRLF
            if i+1 < src.len() && src[i]==0x0d && src[i+1]==0x0a {
                continue;
            }
            // handle indents and line feeds
            // Pascal 1.2 seems to always put the indent code even for no indent, so mimic that.
            if starting_line {
                if i>0 && src[i]==0x20 {
                    indenting += 1;
                    starting_line = false;
                } else {
                    if i>0 {
                        ans.push(0x10);
                        ans.push(0x20);
                        count_on_page += 2;
                    }
                    if src[i]!=0x0a && src[i]!=0x0d {
                        starting_line = false;
                        ans.push(src[i]);
                    } else {
                        ans.push(0x0d);
                    }
                    count_on_page += 1;
                }
            } else if indenting>0 {
                if src[i]==0x20 && indenting+0x20<0xff{
                    indenting += 1;
                } else {
                    ans.push(0x10);
                    ans.push(0x20 + indenting);
                    if src[i]!=0x0a && src[i]!=0x0d {
                        ans.push(src[i]);
                    } else {
                        ans.push(0x0d);
                        starting_line = true;
                    }
                    indenting = 0;
                    count_on_page += 3;
                }
            } else if src[i]==0x0a || src[i]==0x0d {
                ans.push(0x0d);
                count_on_page += 1;
                starting_line = true;
            } else if src[i]<128 {
                ans.push(src[i]);
                count_on_page += 1;
                starting_line = false;
            } else {
                return None;
            }
            // handle pagination
            match paginate(&mut ans,page,count_on_page) {
                Ok(new_page) => page = new_page,
                Err(e) => {
                    eprintln!("{}",e);
                    return None
                }
            }
            count_on_page = count_on_page % TEXT_PAGE;
        }
        // if CR is required and missing add it
        if let Some(terminator) = self.line_terminator {
            if ans[ans.len()-1] != terminator {
                ans.push(terminator);
                count_on_page += 1;
                //starting_line = true;
            }
        }
        // handle pagination one last time
        match paginate(&mut ans,page,count_on_page) {
            Ok(_new_page) => {},//page = new_page},
            Err(e) => {
                eprintln!("{}",e);
                return None
            }
        }
        // pad the rest of the last page
        while ans.len()%TEXT_PAGE>0 {
            ans.push(0);
        }
        return Some(ans);
    }
    fn decode(&self,src: &Vec<u8>) -> Option<String> {
        let mut ans: Vec<u8> = Vec::new();
        let mut await_indent = false;
        for i in 0..src.len() {
            if await_indent {
                for _rep in 0..src[i]-32 {
                    ans.push(0x20);
                }
                await_indent = false;
            } else if src[i]==0x0d {
                ans.push(0x0a);
            } else if src[i]==0x10 {
                await_indent = true;
            } else if src[i]<127 && src[i]>0 {
                ans.push(src[i]);
            }
        }
        let res = String::from_utf8(ans);
        match res {
            Ok(s) => Some(s),
            Err(_) => None
        }
    }
}

/// Structured representation of text files on disk.
/// There is a page structure that we do not put into the structure.
/// The result of this decoder has to pass over nulls, and encoder has to insert them.
pub struct SequentialText {
    pub header: Vec<u8>,
    pub text: Vec<u8>
}

/// Allows the structure to be created from string slices using `from_str`.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let encoder = Encoder::new(None);
        if let Some(dat) = encoder.encode(s) {
            return Ok(Self {
                header: [0;TEXT_PAGE].to_vec(),
                text: dat.clone()
            });
        }
        Err(std::fmt::Error)
    }
}

/// Allows the text to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoder = Encoder::new(None);
        if let Some(ans) = encoder.decode(&self.text) {
            return write!(f,"{}",ans);
        }
        write!(f,"err")
    }
}

impl DiskStruct for SequentialText {
    /// Create an empty structure
    fn new() -> Self {
        Self {
            header: Vec::new(),
            text: Vec::new()
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    /// Due to the pagination, we must keep all the nulls.
    fn from_bytes(dat: &Vec<u8>) -> Self {
        info!("from_bytes: {}",dat.len());
        Self {
            header: dat[0..TEXT_PAGE].to_vec(),
            text: dat[TEXT_PAGE..].to_vec()
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        info!("to_bytes: header {} text {}",self.header.len(),self.text.len());
        ans.append(&mut self.header.clone());
        ans.append(&mut self.text.clone());
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = SequentialText::from_bytes(&dat);
        self.text = temp.text.clone();
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return self.header.len() + self.text.len();
    }
}
