
use std::str::FromStr;
use std::collections::{BTreeMap,HashMap};
use super::{FileImage,Packing,UnpackedData,Records,Error};
use super::{cpm,dos3x,fat,pascal,prodos};
use crate::commands::ItemType;
use crate::{STDRESULT,DYNERR};

const A2_DOS: &str = "a2 dos";
const A2_PASCAL: &str = "a2 pascal";
const PRODOS: &str = "prodos";
const CPM: &str = "cpm";
const FAT: &str = "fat";

impl FileImage {
    pub fn fimg_version() -> String {
        "2.1.0".to_string()
    }
    /// the string slices must be in the form X.Y.Z or else we panic
    pub fn version_tuple(vers: &str) -> (usize,usize,usize) {
        let v: Vec<usize> = vers.split(".").map(|s| usize::from_str(s).expect("bad version format")).collect();
        (v[0],v[1],v[2])
    }
    pub fn ordered_indices(&self) -> Vec<usize> {
        let copy = self.chunks.clone();
        let mut idx_list = copy.into_keys().collect::<Vec<usize>>();
        idx_list.sort_unstable();
        return idx_list;
    }
    /// Find the logical number of chunks (assuming indexing from 0..end)
    pub fn end(&self) -> usize {
        match self.ordered_indices().pop() {
            Some(idx) => idx+1,
            None => 0
        }
    }
    pub fn get_eof(&self) -> usize {
        Self::usize_from_truncated_le_bytes(&self.eof)
    }
    pub fn set_eof(&mut self,eof: usize) {
        self.eof = Self::fix_le_vec(eof,self.eof.len());
    }
    pub fn get_ftype(&self) -> usize {
        Self::usize_from_truncated_le_bytes(&self.fs_type)
    }
    pub fn get_aux(&self) -> usize {
        Self::usize_from_truncated_le_bytes(&self.aux)
    }
    /// does the chunk sequence have any gaps or fail to start at zero
    pub fn is_sparse(&self) -> bool {
        let mut test = 0;
        for i in self.ordered_indices() {
            if i!=test {
                return true;
            }
            test += 1;
        }
        false
    }
    /// pack the data sequentially, all structure is lost
    pub fn sequence(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        for chunk in self.ordered_indices() {
            match self.chunks.get(&chunk) {
                Some(v) => ans.append(&mut v.clone()),
                _ => panic!("unreachable")
            };
        }
        return ans;
    }
    /// pack the data sequentially, all structure is lost
    pub fn sequence_limited(&self,max_len: usize) -> Vec<u8> {
        let mut ans = self.sequence();
        if max_len < ans.len() {
            ans = ans[0..max_len].to_vec();
        }
        return ans;
    }
    /// Use any byte stream as the file image data.  The eof is set to the length of the data.
    /// The last chunk is not padded.  The existing chunks, if any, are thrown away.
    pub fn desequence(&mut self, dat: &[u8]) {
        self.chunks = HashMap::new();
        let mut mark = 0;
        let mut idx = 0;
        if dat.len()==0 {
            self.eof = vec![0;self.eof.len()];
            return;
        }
        loop {
            let mut end = mark + self.chunk_len;
            if end > dat.len() {
                end = dat.len();
            }
            self.chunks.insert(idx,dat[mark..end].to_vec());
            mark = end;
            if mark == dat.len() {
                self.eof = Self::fix_le_vec(dat.len(),self.eof.len());
                return;
            }
            idx += 1;
        }
    }
    /// throw out trailing zeros with exact length constraint
    fn fix_le_vec(val: usize,exact_len: usize) -> Vec<u8> {
        let mut ans = usize::to_le_bytes(val).to_vec();
        let mut count = 0;
        for byte in ans.iter().rev() {
            if *byte>0 {
                break;
            }
            count += 1;
        }
        for _i in 0..count {
            ans.pop();
        }
        for _i in ans.len()..exact_len {
            ans.push(0);
        }
        ans[0..exact_len].to_vec()
    }
    /// compute a usize assuming missing trailing bytes are 0
    fn usize_from_truncated_le_bytes(bytes: &[u8]) -> usize {
        let mut ans: usize = 0;
        for i in 0..bytes.len() {
            if i == usize::BITS as usize/8 {
                break;
            }
            ans += (bytes[i] as usize) << (i*8);
        }
        ans
    }
    pub fn parse_hex_to_vec(key: &str,parsed: &json::JsonValue) -> Result<Vec<u8>,DYNERR> {
        if let Some(s) = parsed[key].as_str() {
            if let Ok(bytes) = hex::decode(s) {
                return Ok(bytes);
            }
        }
        log::error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    pub fn parse_usize(key: &str,parsed: &json::JsonValue) -> Result<usize,DYNERR> {
        if let Some(val) = parsed[key].as_usize() {
            return Ok(val);
        }
        log::error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    pub fn parse_str(key: &str,parsed: &json::JsonValue) -> Result<String,DYNERR> {
        if let Some(s) = parsed[key].as_str() {
            return Ok(s.to_string());
        }
        log::error!("a record is missing in the file image");
        return Err(Box::new(Error::FileImageFormat));
    }
    /// Get chunks from the JSON string representation.  If this is going to be written to a new destination,
    /// don't forget to update the path.
    pub fn from_json(json_str: &str) -> Result<FileImage,DYNERR> {
        let parsed = json::parse(json_str)?;
        let fimg_version = FileImage::parse_str("fimg_version",&parsed)?;
        let vers_tup = Self::version_tuple(&fimg_version);
        if vers_tup < (2,0,0) {
            log::error!("file image v2 or higher is required");
            return Err(Box::new(Error::FileFormat));
        }
        let fs = Self::parse_str("file_system",&parsed)?;
        let chunk_len = Self::parse_usize("chunk_len", &parsed)?;
        let fs_type = Self::parse_hex_to_vec("fs_type",&parsed)?;
        let aux = Self::parse_hex_to_vec("aux",&parsed)?;
        let eof = Self::parse_hex_to_vec("eof",&parsed)?;
        let access = Self::parse_hex_to_vec("access",&parsed)?;
        let created = Self::parse_hex_to_vec("created",&parsed)?;
        let modified = Self::parse_hex_to_vec("modified",&parsed)?;
        let version = Self::parse_hex_to_vec("version",&parsed)?;
        let min_version = Self::parse_hex_to_vec("min_version",&parsed)?;
        let accessed = match vers_tup >= (2,1,0) {
            true => Self::parse_hex_to_vec("accessed",&parsed)?,
            false => vec![]
        };
        let full_path = match vers_tup >= (2,1,0) {
            true => Self::parse_str("full_path",&parsed)?,
            false => String::new()
        };
        let mut chunks: HashMap<usize,Vec<u8>> = HashMap::new();
        let map_obj = &parsed["chunks"];
        if map_obj.entries().len()==0 {
            log::warn!("file image contains metadata, but no data");
        }
        for (key,hex) in map_obj.entries() {
            let prev_len = chunks.len();
            if let Ok(num) = usize::from_str(key) {
                if let Some(hex_str) = hex.as_str() {
                    if let Ok(dat) = hex::decode(hex_str) {
                        chunks.insert(num,dat);
                    }
                }
            }
            if chunks.len()==prev_len {
                log::error!("could not read hex string from chunk");
                return Err(Box::new(Error::FileImageFormat));
            }
        }
        return Ok(Self {
            fimg_version,
            file_system: fs.to_string(),
            chunk_len,
            eof,
            fs_type,
            aux,
            access,
            accessed,
            created,
            modified,
            version,
            min_version,
            full_path,
            chunks
        });
    }
    /// Put chunks into the JSON string representation
    pub fn to_json(&self,indent: Option<u16>) -> String {
        let mut json_map = json::JsonValue::new_object();
        let mut sorted : BTreeMap<usize,Vec<u8>> = BTreeMap::new();
        for (c,v) in &self.chunks {
            sorted.insert(*c,v.clone());
        }
        for (c,v) in &sorted {
            json_map[c.to_string()] = json::JsonValue::String(hex::encode_upper(v));
        }
        let ans = json::object! {
            fimg_version: self.fimg_version.clone(),
            file_system: self.file_system.clone(),
            chunk_len: self.chunk_len,
            eof: hex::encode_upper(self.eof.clone()),
            fs_type: hex::encode_upper(self.fs_type.clone()),
            aux: hex::encode_upper(self.aux.clone()),
            access: hex::encode_upper(self.access.clone()),
            accessed: hex::encode_upper(self.accessed.clone()),
            created: hex::encode_upper(self.created.clone()),
            modified: hex::encode_upper(self.modified.clone()),
            version: hex::encode_upper(self.version.clone()),
            min_version: hex::encode_upper(self.min_version.clone()),
            full_path: self.full_path.clone(),
            chunks: json_map
        };
        if let Some(spaces) = indent {
            return json::stringify_pretty(ans, spaces);
        } else {
            return json::stringify(ans);
        }
    }
    fn packer(&self) -> Box<dyn Packing> {
        match self.file_system.as_str() {
            A2_DOS => Box::new(dos3x::Packer::new()),
            A2_PASCAL => Box::new(pascal::Packer::new()), 
            PRODOS => Box::new(prodos::Packer::new()), 
            CPM => Box::new(cpm::Packer::new()),
            FAT => Box::new(fat::Packer::new()),
            _ => panic!("illegal file system in file image")
        }
    }
    pub fn set_path(&mut self, path: &str) -> STDRESULT {
        self.packer().set_path(self,path)
    }
    /// Get load address for this file image, if applicable.
    pub fn get_load_address(&self) -> u16 {
        self.packer().get_load_address(self)
    }
    /// automatically select an unpacking strategy based on the file image metadata
    pub fn unpack(&self) -> Result<UnpackedData,DYNERR> {
        self.packer().unpack(self)
    }
    /// Pack raw byte stream into file image.
    /// Headers used by the file system are *not* automatically inserted.
    /// If the file system has explicit typing, the type is set to text.
    pub fn pack_raw(&mut self, dat: &[u8]) -> STDRESULT {
        self.packer().pack_raw(self,dat)
    }
    /// Get the raw bytestream, including any header used by the file system.
    /// The byte stream will extend to end of block unless `trunc==true`.
    /// Setting `trunc==true` only works if the EOF is stored in the directory.
    pub fn unpack_raw(&self,trunc: bool) -> Result<Vec<u8>,DYNERR> {
        self.packer().unpack_raw(self,trunc)
    }
    /// Pack bytes into file image, if file system uses a header it is added.
    /// The load address will be checked for validity, if not used by FS it must be None.
    pub fn pack_bin(&mut self,dat: &[u8],load_addr: Option<usize>,trailing: Option<&[u8]>) -> STDRESULT {
        self.packer().pack_bin(self,dat,load_addr,trailing)
    }
    /// get bytes from file image, if file system uses a header it is stripped
    pub fn unpack_bin(&self) -> Result<Vec<u8>,DYNERR> {
        self.packer().unpack_bin(self)
    }
    /// Convert UTF8 with either LF or CRLF to the file system's text format.  This returns an error
    /// if the conversion would result in any loss of data.
    pub fn pack_txt(&mut self, txt: &str) -> STDRESULT {
        self.packer().pack_txt(self,txt)
    }
    /// Convert the file system's text format to UTF8 with LF.  This always succeeds because the underlying
    /// text converters will replace unknown characters with ASCII NULL.
    pub fn unpack_txt(&self) -> Result<String,DYNERR> {
        self.packer().unpack_txt(self)
    }
    /// pack language tokens into file image, if file system uses a header it is added
    pub fn pack_tok(&mut self,tok: &[u8],lang: ItemType,trailing: Option<&[u8]>) -> STDRESULT {
        self.packer().pack_tok(self,tok,lang,trailing)
    }
    /// get language tokens from file image, if file system uses a header it is stripped
    pub fn unpack_tok(&self) -> Result<Vec<u8>,DYNERR> {
        self.packer().unpack_tok(self)
    }
    /// pack JSON representation of random access text into a file image
    pub fn pack_rec_str(&mut self, json: &str) -> STDRESULT {
        self.packer().pack_rec_str(self, json)
    }
    /// get JSON representation of random access text
    pub fn unpack_rec_str(&self,rec_len: Option<usize>,indent: Option<u16>) -> Result<String,DYNERR> {
        self.packer().unpack_rec_str(self, rec_len, indent)
    }
    /// pack random access text records into a file image
    pub fn pack_rec(&mut self, recs: &Records) -> STDRESULT {
        self.packer().pack_rec(self,recs)
    }
    /// get random access text records
    pub fn unpack_rec(&self,rec_len: Option<usize>) -> Result<Records,DYNERR> {
        self.packer().unpack_rec(self,rec_len)
    }
}
