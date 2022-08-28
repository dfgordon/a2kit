use std::str::FromStr;
use std::fmt;
use a2kit_macro::DiskStruct;

/// Structured representation of the bytes on disk that are stored with a BASIC program.  Works with either Applesoft or Integer.
pub struct TokenizedProgram {
    length: [u8;2],
    pub program: Vec<u8>
}

impl TokenizedProgram {
    /// Take unstructured bytes representing the tokens only (sans header) and pack it into the structure
    pub fn pack(prog: &Vec<u8>) -> Self {
        Self {
            length: u16::to_le_bytes(prog.len() as u16),
            program: prog.clone()
        }
    }
}

impl DiskStruct for TokenizedProgram {
    /// Create an empty structure
    fn new() -> Self
    {
        Self {
            length: [0;2],
            program: Vec::new()
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        let end_byte = u16::from_le_bytes([dat[0],dat[1]]) as usize;
        // equality is not required because there could be sector padding
        if end_byte > dat.len() {
            panic!("inconsistent tokenized program length");
        }
        return Self {
            length: [dat[0],dat[1]],
            program: dat[2..end_byte].to_vec().clone()
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.length.to_vec());
        ans.append(&mut self.program.clone());
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = TokenizedProgram::from_bytes(&dat);
        self.length = temp.length;
        self.program = temp.program.clone();
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return 2 + self.program.len();
    }
}

/// Structured representation of sequential text files on disk.  Will not work for random access files.
pub struct SequentialText {
    pub text: Vec<u8>,
    terminator: u8
}

impl SequentialText {
    /// Take unstructured bytes representing the text only (sans terminator) and pack it into the structure.
    /// These are not standard UTF8 bytes, see `FromStr` and `Display` below if you need to convert.
    pub fn pack(txt: &Vec<u8>) -> Self {
        Self {
            text: txt.clone(),
            terminator: 0
        }
    }
}

/// Allows the structure to be created from string slices using `from_str`.
/// This is not quite trivial: A2 text is negative ASCII and uses carriage returns only.
impl FromStr for SequentialText {
    type Err = std::fmt::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        let pos: Vec<u8> = s.as_bytes().to_vec();
        let mut neg: Vec<u8> = Vec::new();
        for i in 0..pos.len() {
            if neg.len()>0 && neg[neg.len()-1]==0x8d && pos[i]==0x0a {
                continue;
            }
            if pos[i]==0x0a || pos[i]==0x0d {
                neg.push(0x8d);
            }
            if pos[i]<128 {
                neg.push(pos[i]+0x80);
            } else {
                return Err(std::fmt::Error);
            }
        }
        return Ok(Self {
            text: neg.clone(),
            terminator: 0
        });
    }
}

/// Allows the text to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the structure can be converted to `String`.  This
/// is not quite trivial: A2 text is negative ASCII and uses carriage returns only.
impl fmt::Display for SequentialText {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut pos: Vec<u8> = Vec::new();
        let neg: Vec<u8> = self.text.clone();
        for i in 0..neg.len() {
            if neg[i]==0x8d {
                pos.push(0x0a);
            }
            if neg[i]>127 {
                pos.push(neg[i]-0x80);
            } else {
                return Err(std::fmt::Error);
            }
        }
        let res = String::from_utf8(pos);
        match res {
            Ok(s) => write!(f,"{}",s),
            Err(_) => write!(f,"err")
        }
    }
}

impl DiskStruct for SequentialText {
    /// Create an empty structure
    fn new() -> Self {
        Self {
            text: Vec::new(),
            terminator: 0
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        // find end of text
        let mut end_byte = dat.len();
        for i in 0..dat.len() {
            if dat[i]==0 {
                end_byte = i;
                break;
            }
        }
        Self {
            text: dat[0..end_byte as usize].to_vec(),
            terminator: 0
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.text.clone());
        ans.push(0);
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = SequentialText::from_bytes(&dat);
        self.text = temp.text.clone();
        self.terminator = 0;
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return self.text.len() + 1;
    }
}

/// Structured representation of binary data on disk
pub struct BinaryData {
    pub start: [u8;2],
    length: [u8;2],
    pub data: Vec<u8>
}

impl BinaryData {
    /// Take unstructured bytes representing the data only (sans header) and pack it into the structure
    pub fn pack(bin: &Vec<u8>, addr: u16) -> Self {
        Self {
            start: u16::to_le_bytes(addr),
            length: u16::to_le_bytes(bin.len() as u16),
            data: bin.clone()
        }
    }
}

impl DiskStruct for BinaryData {
    /// Create an empty structure
    fn new() -> Self
    {
        Self {
            start: [0;2],
            length: [0;2],
            data: Vec::new()
        }
    }
    /// Create structure using flattened bytes (typically from disk)
    fn from_bytes(dat: &Vec<u8>) -> Self {
        let end_byte = u16::from_le_bytes([dat[2],dat[3]]) + 4;
        Self {
            start: [dat[0],dat[1]],
            length: [dat[2],dat[3]],
            data: dat[4..end_byte as usize].to_vec()
        }
    }
    /// Return flattened bytes (typically written to disk)
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.start.to_vec());
        ans.append(&mut self.length.to_vec());
        ans.append(&mut self.data.clone());
        return ans;
    }
    /// Update with flattened bytes (useful mostly as a crutch within a2kit_macro)
    fn update_from_bytes(&mut self,dat: &Vec<u8>) {
        let temp = BinaryData::from_bytes(&dat);
        self.start = temp.start;
        self.length = temp.length;
        self.data = temp.data.clone();
    }
    /// Length of the flattened structure
    fn len(&self) -> usize {
        return 4 + self.data.len();
    }
}