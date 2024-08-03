//! ## Support for WOZ v1 disk images
//! 
//! This uses the nibble machinery in module `disk525` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.
//! WOZ v1 cannot handle actual 3.5 inch disk tracks.  Therefore the top level `create`
//! function is set to panic if a 3.5 inch disk is requested.  You can use WOZ v2 for
//! 3.5 inch disks.

use log::{debug,info,warn,error};
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;
use crate::img::disk525;
use crate::img;
use crate::img::meta;
use crate::img::woz::{TMAP_ID,TRKS_ID,INFO_ID,META_ID};
use crate::{STDRESULT,DYNERR,getByte,getByteEx,putByte,putStringBuf};

use super::woz::HeadCoords;

const TRACK_BYTE_CAPACITY: usize = 6646;

pub fn file_extensions() -> Vec<String> {
    vec!["woz".to_string()]
}

#[derive(DiskStruct)]
pub struct Header {
    vers: [u8;4],
    high_bits: u8,
    lfcrlf: [u8;3],
    crc32: [u8;4]
}

#[derive(DiskStruct)]
pub struct Info {
    id: [u8;4],
    size: [u8;4],
    vers: u8,
    disk_type: u8,
    write_protected: u8,
    synchronized: u8,
    cleaned: u8,
    creator: [u8;32],
    pad: [u8;23]
}

#[derive(DiskStruct)]
pub struct TMap {
    id: [u8;4],
    size: [u8;4],
    map: [u8;160]
}

#[derive(DiskStruct,Clone,Copy)]
pub struct Trk {
    bits: [u8;TRACK_BYTE_CAPACITY],
    bytes_used: [u8;2],
    bit_count: [u8;2],
    splice_point: [u8;2],
    splice_nib: u8,
    splice_bit_count: u8,
    pad: [u8;2]
}

pub struct Trks {
    id: [u8;4],
    size: [u8;4],
    tracks: Vec<Trk>
}

pub struct Woz1 {
    kind: img::DiskKind,
    header: Header,
    info: Info,
    tmap: TMap,
    trks: Trks,
    meta: Option<Vec<u8>>,
    head_coords: HeadCoords
}

impl Header {
    fn create() -> Self {
        Self {
            vers: [0x57,0x4f,0x5a,0x31],
            high_bits: 0xff,
            lfcrlf: [0x0a,0x0d,0x0a],
            crc32: [0,0,0,0]
        }
    }
}

impl Info {
    fn create(kind: img::DiskKind) -> Self {

        let creator_str = "a2kit v".to_string() + env!("CARGO_PKG_VERSION");
        let mut creator: [u8;32] = [0x20;32];
        for i in 0..creator_str.len() {
            creator[i] = creator_str.as_bytes()[i];
        }
        Self {
            id: u32::to_le_bytes(INFO_ID),
            size: u32::to_le_bytes(60),
            vers: 1,
            disk_type: match kind {
                img::names::A2_DOS32_KIND => 1,
                img::names::A2_DOS33_KIND => 1,
                _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
            },
            write_protected: 0,
            synchronized: 0,
            cleaned: 0,
            creator,
            pad: [0;23]
        }
    }
    fn verify_value(&self,key: &str,hex_str: &str) -> bool {
        match key {
            stringify!(disk_type) => hex_str=="01" || hex_str=="02",
            stringify!(write_protected) => hex_str=="00" || hex_str=="01",
            stringify!(synchronized) => hex_str=="00" || hex_str=="01",
            stringify!(cleaned) => hex_str=="00" || hex_str=="01",
            _ => true
        }
    }
}

impl TMap {
    fn create(kind: img::DiskKind) -> Self {
        let mut map: [u8;160] = [0xff;160];
        match kind {
            img::names::A2_DOS32_KIND | img::names::A2_DOS33_KIND => {
                for i in 0 as u8..139 {
                    map[i as usize] = match i {
                        x if x%4==0 => x/4,
                        x if x%4==1 => x/4,
                        x if x%4==2 => 0xff,
                        x => x/4 + 1
                    };
                }
            }
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        }
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
}

impl Trk {
    fn create(vol: u8,track: u8,kind: img::DiskKind) -> Self {
        let (bits,track_obj) = match kind {
            img::names::A2_DOS32_KIND => disk525::create_std13_track(vol,track,TRACK_BYTE_CAPACITY),
            img::names::A2_DOS33_KIND => disk525::create_std16_track(vol,track,TRACK_BYTE_CAPACITY),
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        };
        let bytes_used = u16::to_le_bytes(bits.len() as u16);
        Self {
            bits: bits.try_into().expect("bit buffer mismatch"),
            bytes_used,
            bit_count: u16::to_le_bytes(track_obj.bit_count() as u16),
            splice_point: u16::to_le_bytes(0xffff),
            splice_nib: 0,
            splice_bit_count: 0,
            pad: [0,0]
        }
    }
}

impl Trks {
    fn create(vol: u8,kind: img::DiskKind) -> Self {
        let tracks: usize = match kind {
            img::names::A2_DOS32_KIND => 35,
            img::names::A2_DOS33_KIND => 35,
            _ => panic!("WOZ v1 can only accept physical 5.25 inch Apple formats")
        };
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        ans.size = u32::to_le_bytes(tracks as u32 * Trk::new().len() as u32);
        for track in 0..tracks {
            let trk = Trk::create(vol,track as u8,kind);
            ans.tracks.push(trk);
        }
        return ans;
    }
    fn num_tracks(&self) -> usize {
        // would this list ever be padded?
        return self.tracks.len();
    }
}

impl DiskStruct for Trks {
    fn new() -> Self where Self: Sized {
        Self {
            id: [0,0,0,0],
            size: [0,0,0,0],
            tracks: Vec::new()
        }
    }
    fn len(&self) -> usize {
        8 + u32::from_le_bytes(self.size) as usize
    }
    fn update_from_bytes(&mut self,bytes: &[u8]) -> Result<(),DiskStructError> {
        let sz = Trk::new().len();
        self.id = [bytes[0],bytes[1],bytes[2],bytes[3]];
        self.size = [bytes[4],bytes[5],bytes[6],bytes[7]];
        let num_tracks = u32::from_le_bytes(self.size) as usize / sz;
        let mut off = 8;
        self.tracks = Vec::new();
        for _track in 0..num_tracks {
            let trk = Trk::from_bytes(&bytes[off..off+sz])?;
            self.tracks.push(trk);
            off += sz;
        }
        Ok(())
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        let mut ans = Trks::new();
        ans.update_from_bytes(bytes)?;
        Ok(ans)
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.id.to_vec());
        ans.append(&mut self.size.to_vec());
        for trk in &self.tracks {
            ans.append(&mut trk.to_bytes());
        }
        return ans;
    }
}

impl Woz1 {
    fn new() -> Self {
        Self {
            kind: img::DiskKind::Unknown,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            trks: Trks::new(),
            meta: None,
            head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
        }
    }
    /// Create the image of a specific kind of disk (panics if unsupported disk kind).
    /// The volume is used to format the address fields on the tracks.
    pub fn create(vol: u8,kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_DOS33_KIND {
            panic!("WOZ v1 can only accept 5.25 inch Apple formats")
        }
        Self {
            kind,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::create(kind),
            trks: Trks::create(vol,kind),
            meta: None,
            head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
        }
    }
    /// Get index to the `Trk` structure, searching main track and nearby quarter-tracks.
    /// If no data this will panic.
    fn get_trk_idx(&self,track: u8) -> usize {
        let key_idx = track as usize*4;
        if self.tmap.map[key_idx]!=0xff {
            return self.tmap.map[key_idx] as usize;
        }
        if key_idx!=0 {
            if self.tmap.map[key_idx-1]!=0xff {
                return self.tmap.map[key_idx-1] as usize;
            }
        }
        if key_idx!=self.tmap.map.len() {
            if self.tmap.map[key_idx+1]!=0xff {
                return self.tmap.map[key_idx+1] as usize;
            }
        }
        error!("This image has a missing track; cannot be handled in general");
        panic!("WOZ track not found");
    }
    /// Find track and get a reference
    fn get_trk_ref(&self,track: u8) -> &Trk {
        return &self.trks.tracks[self.get_trk_idx(track)];
    }
    /// Get a reference to the track bits
    fn get_trk_bits_ref(&self,track: u8) -> &[u8] {
        return &self.trks.tracks[self.get_trk_idx(track)].bits;
    }
    /// Get a mutable reference to the track bits
    fn get_trk_bits_mut(&mut self,track: u8) -> &mut [u8] {
        let idx = self.get_trk_idx(track);
        return &mut self.trks.tracks[idx].bits;
    }
    /// Create a lightweight trait object to read/write the bits.  The nibble format will be
    /// determined by the image's underlying `DiskKind`.
    fn new_rw_obj(&mut self,track: u8) -> Box<dyn super::TrackBits> {
        if self.head_coords.track != track as usize {
            debug!("goto track {} of {}",track,self.kind);
            self.head_coords.track = track as usize;
        }
        let bit_count_le = self.get_trk_ref(track).bit_count;
        let bit_count = u16::from_le_bytes(bit_count_le) as usize;
        let mut ans: Box<dyn super::TrackBits> = match self.kind {
            super::names::A2_DOS32_KIND => Box::new(disk525::TrackBits::create(
                track as usize,
                bit_count,
                disk525::SectorAddressFormat::create_std13(),
                disk525::SectorDataFormat::create_std13())),
            super::names::A2_DOS33_KIND => Box::new(disk525::TrackBits::create(
                track as usize,
                bit_count,
                disk525::SectorAddressFormat::create_std16(),
                disk525::SectorDataFormat::create_std16())),
            _ => panic!("incompatible disk")
        };
        if self.head_coords.bit_ptr < bit_count {
            ans.set_bit_ptr(self.head_coords.bit_ptr);
        }
        return ans;
    }
}

impl img::woz::WozUnifier for Woz1 {
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn num_tracks(&self) -> usize {
        self.trks.num_tracks()
    }
    fn read_sector(&mut self,track: u8,sector: u8) -> Result<Vec<u8>,img::NibbleError> {
        let mut reader = self.new_rw_obj(track);
        let ans = reader.read_sector(self.get_trk_bits_ref(track),track,sector)?;
        self.head_coords.bit_ptr = reader.get_bit_ptr();
        Ok(ans)
    }
    fn write_sector(&mut self,dat: &[u8],track: u8,sector: u8) -> Result<(),img::NibbleError> {
        let mut writer = self.new_rw_obj(track);
        writer.write_sector(self.get_trk_bits_mut(track),dat,track,sector)?;
        self.head_coords.bit_ptr = writer.get_bit_ptr();
        Ok(())
    }
}

impl img::DiskImage for Woz1 {
    fn track_count(&self) -> usize {
        match self.info.disk_type {
            1 => 35,
            2 => 160,
            _ => panic!("disk type not supported")
        }
    }
    fn num_heads(&self) -> usize {
        1
    }
    fn byte_capacity(&self) -> usize {
        match self.info.disk_type {
            1 => self.track_count()*16*256,
            2 => 1600*512,
            _ => panic!("disk type not supported")
        }
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::WOZ1
    }
    fn file_extensions(&self) -> Vec<String> {
        file_extensions()
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn read_block(&mut self,addr: crate::fs::Block) -> Result<Vec<u8>,DYNERR> {
        super::woz::read_block(self, addr)
    }
    fn write_block(&mut self, addr: crate::fs::Block, dat: &[u8]) -> STDRESULT {
        super::woz::write_block(self, addr, dat)
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        super::woz::read_sector(self,cyl,head,sec)
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        super::woz::write_sector(self, cyl, head, sec, dat)
    }
    fn from_bytes(buf: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        if buf.len()<12 {
            return Err(DiskStructError::UnexpectedSize);
        }
        let mut ans = Woz1::new();
        ans.header.update_from_bytes(&buf[0..12].to_vec())?;
        if ans.header.vers!=[0x57,0x4f,0x5a,0x31] {
            return Err(DiskStructError::IllegalValue);
        }
        info!("identified WOZ v1 header");
        let mut ptr: usize= 12;
        while ptr>0 {
            let (next,id,maybe_chunk) = img::woz::get_next_chunk(ptr, buf);
            match (id,maybe_chunk) {
                (INFO_ID,Some(chunk)) => ans.info.update_from_bytes(&chunk)?,
                (TMAP_ID,Some(chunk)) => ans.tmap.update_from_bytes(&chunk)?,
                (TRKS_ID,Some(chunk)) => ans.trks.update_from_bytes(&chunk)?,
                (META_ID,Some(chunk)) => ans.meta = Some(chunk),
                _ => if id!=0 {
                    info!("unprocessed chunk with id {:08X}/{}",id,String::from_utf8_lossy(&u32::to_le_bytes(id)))
                }
            }
            ptr = next;
        }
        if u32::from_le_bytes(ans.info.id)>0 && u32::from_le_bytes(ans.tmap.id)>0 && u32::from_le_bytes(ans.trks.id)>0 && ans.info.disk_type==1 {
            if let Ok(Some(_sol)) = ans.get_track_solution(0) {
                debug!("setting disk kind to {}",ans.kind);
            } else {
                warn!("could not solve track 0, continuing with {}",ans.kind);
            }
            return Ok(ans);
        }
        warn!("WOZ v1 sanity checks failed, refusing");
        return Err(DiskStructError::UnexpectedValue);
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.append(&mut self.header.to_bytes());
        ans.append(&mut self.info.to_bytes());
        ans.append(&mut self.tmap.to_bytes());
        ans.append(&mut self.trks.to_bytes());
        if let Some(mut meta) = self.meta.clone() {
            ans.append(&mut meta);
        }
        let crc = u32::to_le_bytes(img::woz::crc32(0, &ans[12..].to_vec()));
        ans[8] = crc[0];
        ans[9] = crc[1];
        ans[10] = crc[2];
        ans[11] = crc[3];
        return ans;
    }
    fn get_track_buf(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        Ok(self.get_trk_bits_ref(track_num as u8).to_vec())
    }
    fn set_track_buf(&mut self,cyl: usize,head: usize,dat: &[u8]) -> STDRESULT {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let bits = self.get_trk_bits_mut(track_num as u8);
        if bits.len()!=dat.len() {
            error!("source track buffer is {} bytes, destination track buffer is {} bytes",dat.len(),bits.len());
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        bits.copy_from_slice(dat);
        Ok(())
    }
    fn get_track_solution(&mut self,track: usize) -> Result<Option<img::TrackSolution>,DYNERR> {
        let [cylinder,head] = self.track_2_ch(track);
        self.kind = img::names::A2_DOS32_KIND;
        let mut reader = self.new_rw_obj(track as u8);
        if let Ok(chss_map) = reader.chss_map(self.get_trk_bits_ref(track as u8)) {
            return Ok(Some(img::TrackSolution {
                cylinder,
                head,
                flux_code: img::FluxCode::GCR,
                nib_code: img::NibbleCode::N53,
                chss_map
            }));
        }
        self.kind = img::names::A2_DOS33_KIND;
        reader = self.new_rw_obj(track as u8);
        if let Ok(chss_map) = reader.chss_map(self.get_trk_bits_ref(track as u8)) {
            return Ok(Some(img::TrackSolution {
                cylinder,
                head,
                flux_code: img::FluxCode::GCR,
                nib_code: img::NibbleCode::N62,
                chss_map
            }));
        }
        return Err(Box::new(img::Error::UnknownDiskKind));
    }
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        let track_num = super::woz::cyl_head_to_track(self, cyl, head)?;
        let mut reader = self.new_rw_obj(track_num as u8);
        Ok(reader.to_nibbles(self.get_trk_bits_ref(track_num as u8)))
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        super::woz::display_track(self, 0, &bytes)
    }
    fn get_metadata(&self,indent: u16) -> String {
        let mut root = json::JsonValue::new_object();
        let woz1 = self.what_am_i().to_string();
        root[&woz1] = json::JsonValue::new_object();
        root[&woz1]["info"] = json::JsonValue::new_object();
        getByteEx!(root,woz1,self.info.disk_type);
        root[&woz1]["info"]["disk_type"]["_pretty"] = json::JsonValue::String(match self.info.disk_type {
            1 => "Apple 5.25 inch".to_string(),
            2 => "Apple 3.5 inch".to_string(),
            _ => "Unexpected value".to_string()
        });
        getByte!(root,woz1,self.info.write_protected);
        getByte!(root,woz1,self.info.synchronized);
        getByte!(root,woz1,self.info.cleaned);
        root[woz1]["info"]["creator"] = json::JsonValue::String(String::from_utf8_lossy(&self.info.creator).trim_end().to_string());
        if indent==0 {
            json::stringify(root)
        } else {
            json::stringify_pretty(root, indent)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            debug!("put key `{:?}` with val `{}`",key_path,val);
            let woz1 = self.what_am_i().to_string();
            meta::test_metadata(key_path, self.what_am_i())?;
            if meta::match_key(key_path,&[&woz1,"info","disk_type"]) {
                warn!("skipping read-only `disk_type`");
                return Ok(());
            }
            if key_path.len()>2 && key_path[0]=="woz1" && key_path[1]=="info" {
                if !self.info.verify_value(&key_path[2], val) {
                    error!("INFO chunk key `{}` had a bad value `{}`",key_path[2],val);
                    return Err(Box::new(img::Error::MetadataMismatch));
                }
            }
            putByte!(val,key_path,woz1,self.info.write_protected);
            putByte!(val,key_path,woz1,self.info.synchronized);
            putByte!(val,key_path,woz1,self.info.cleaned);
            putStringBuf!(val,key_path,woz1,self.info.creator,0x20);
        }
        error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}
