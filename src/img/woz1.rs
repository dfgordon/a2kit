//! ## Support for WOZ v1 disk images
//! 
//! Use WOZ v2 unless there is a specific need to process v1 disks.
//! 
//! This uses the nibble machinery in module `tracks::gcr` to handle the bit streams.
//! The `DiskStruct` trait is used to flatten and unflatten the wrapper structures.
//! WOZ v1 cannot handle actual 3.5 inch disk tracks.  Therefore the top level `create`
//! function is set to panic if a 3.5 inch disk is requested.  You can use WOZ v2 for
//! 3.5 inch disks.

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use a2kit_macro::{DiskStructError,DiskStruct};
use a2kit_macro_derive::DiskStruct;
use crate::img;
use crate::img::meta;
use crate::img::tracks::{TrackKey,SectorKey,Method,FluxCells};
use crate::img::tracks::gcr::TrackEngine;
use crate::img::woz::{TMAP_ID,TRKS_ID,INFO_ID,META_ID};
use crate::bios::blocks::apple;
use crate::{STDRESULT,DYNERR,getByte,getByteEx,putByte,putStringBuf};

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
    fmt: Option<img::tracks::DiskFormat>,
    header: Header,
    info: Info,
    tmap: TMap,
    trks: Trks,
    meta: Option<Vec<u8>>,
    /// state: controller
    engine: TrackEngine,
    /// state: current track data and angle
    cells: Option<FluxCells>,
    /// state: current track, accounts for quarter tracks
    tmap_pos: usize,
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
                img::DiskKind::D525(_) => 1,
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
    fn blank() -> Self {
        let map: [u8;160] = [0xff;160];
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
    fn create(fmt: &img::tracks::DiskFormat) -> Self {
        let mut map: [u8;160] = [0xff;160];
        let motor_head = fmt.get_motor_and_head();
        let mut slot = 0;
        for (m,h) in motor_head {
            if h>0 {
                panic!("WOZ v1 rejected side 2");
            }
            if m>0 {
                map[m-1] = slot;
            }
            map[m] = slot;
            if m<159 {
                map[m+1] = slot;
            }
            slot += 1;
        }
        Self {
            id: u32::to_le_bytes(TMAP_ID),
            size: u32::to_le_bytes(160),
            map
        }
    }
}

impl Trk {
    /// `vol` and `track` are only used for the address field
    fn create(skey: SectorKey,fmt: &img::tracks::ZoneFormat) -> Result<Self,DYNERR> {
        fmt.check_flux_code(img::FluxCode::GCR)?;
        let mut engine = TrackEngine::create(Method::Edit,false);
        let cells = engine.format_track(skey, TRACK_BYTE_CAPACITY, fmt)?;
        let (bits,_) = cells.to_woz_buf(Some(TRACK_BYTE_CAPACITY),0);
        let bytes_used = u16::to_le_bytes(bits.len() as u16);
        Ok(Self {
            bits: bits.try_into().expect("track buffer mismatch"),
            bytes_used,
            bit_count: u16::to_le_bytes(cells.count() as u16),
            splice_point: u16::to_le_bytes(0xffff),
            splice_nib: 0,
            splice_bit_count: 0,
            pad: [0,0]
        })
    }
}

impl Trks {
    fn blank() -> Self {
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        ans.size = [0,0,0,0];
        return ans;
    }
    fn create(vol: u8,kind: &img::DiskKind,fmt: &img::tracks::DiskFormat,tmap: &[u8]) -> Result<Self,DYNERR> {
        let mut ans = Trks::new();
        ans.id = u32::to_le_bytes(TRKS_ID);
        // Spec gives track data location = (tmap_value * 6656) + 256.
        // This means we have to store a buffer for unused slots that
        // occur prior to the last used slot.
        let mut end_slot = 0;
        for slot in 0..160 {
            if img::woz::get_trks_slot_id(vol,slot, tmap, None, kind).is_some() {
                end_slot = slot + 1;
            }
        }
        ans.size = u32::to_le_bytes(end_slot as u32 * Trk::new().len() as u32);
        for slot in 0..end_slot {
            if let Some((motor,skey,_)) = img::woz::get_trks_slot_id(vol, slot, tmap, None, kind) {
                ans.tracks.push(Trk::create(skey,fmt.get_zone_fmt(motor as usize,0)?)?);
            } else {
                ans.tracks.push(Trk::new());
            }
        }
        Ok(ans)
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
            fmt: None,
            header: Header::new(),
            info: Info::new(),
            tmap: TMap::new(),
            trks: Trks::new(),
            meta: None,
            engine: TrackEngine::create(Method::Edit,false),
            cells: None,
            tmap_pos: usize::MAX,
        }
    }
    /// Panics if `kind` is not supported
    pub fn blank(kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_DOS33_KIND {
            panic!("WOZ v1 can only accept 5.25 inch Apple formats")
        }
        Self {
            kind,
            fmt: None,
            header: Header::create(),
            info: Info::create(kind),
            tmap: TMap::blank(),
            trks: Trks::blank(),
            meta: None,
            engine: TrackEngine::create(Method::Edit,false),
            cells: None,
            tmap_pos: usize::MAX,
        }
    }
    /// Create the image of a specific kind of disk (panics if unsupported disk kind).
    /// The volume is used to format the address fields on the tracks.
    /// Panics if `kind` is not supported.
    pub fn create(vol: u8,kind: img::DiskKind,maybe_fmt: Option<img::tracks::DiskFormat>) -> Result<Self,DYNERR> {
        let fmt = match maybe_fmt {
            Some(fmt) => fmt,
            None => img::woz::kind_to_format(&kind).unwrap()
        };
        let tmap = TMap::create(&fmt);
        let trks = Trks::create(vol,&kind,&fmt,&tmap.map)?;
        Ok(Self {
            kind,
            fmt: Some(fmt),
            header: Header::create(),
            info: Info::create(kind),
            tmap,
            trks,
            meta: None,
            engine: TrackEngine::create(Method::Edit,false),
            cells: None,
            tmap_pos: usize::MAX,
        })
    }
    fn sanity_check(&self) -> Result<(),DiskStructError> {
        match u32::from_le_bytes(self.info.id)>0 && u32::from_le_bytes(self.tmap.id)>0 && u32::from_le_bytes(self.trks.id)>0 && self.info.disk_type==1 {
            true => Ok(()),
            false => {
                log::debug!("WOZ v1 sanity checks failed");
                return Err(DiskStructError::IllegalValue);
            }
        }
    }
    /// see if there is a buffer for the given quarter track, does not change disk state
    fn try_motor(&self,tmap_idx: usize) -> Result<usize,DYNERR> {
        if self.tmap.map[tmap_idx] == 0xff {
            log::info!("touched blank media at TMAP index {}",tmap_idx);
            Err(Box::new(super::NibbleError::BadTrack))
        } else if self.tmap.map[tmap_idx] as usize >= self.trks.tracks.len() {
            Err(Box::new(super::Error::TrackCountMismatch))
        } else {
            Ok(self.tmap.map[tmap_idx] as usize)
        }
    }
    /// Get a reference to the track bits
    fn get_trk_bits_ref(&self,tkey: TrackKey) -> Result<&[u8],DYNERR> {
        let tmap_idx = img::woz::get_tmap_index(tkey,&self.kind)?;
        let idx = self.try_motor(tmap_idx)?;
        return Ok(&self.trks.tracks[idx].bits);
    }
    /// Get a mutable reference to the track bits
    fn get_trk_bits_mut(&mut self,tkey: TrackKey) -> Result<&mut [u8],DYNERR> {
        let tmap_idx = img::woz::get_tmap_index(tkey,&self.kind)?;
        let idx = self.try_motor(tmap_idx)?;
        return Ok(&mut self.trks.tracks[idx].bits);
    }
    /// Save changes to the current track buffer if they exist
    fn write_back_track(&mut self) {
        if let Some(cells) = &self.cells {
            let idx = self.try_motor(self.tmap_pos).expect("out of sequence access");
            let (buf,_) = cells.to_woz_buf(Some(TRACK_BYTE_CAPACITY),0);
            self.trks.tracks[idx].bits = buf.try_into().expect("track buffer mismatch");
        }
    }
    /// Goto track and extract FluxCells if necessary, returns [motor,head,width]
    fn goto_track(&mut self,tkey: TrackKey) -> Result<[usize;3],DYNERR> {
        let tmap_idx = img::woz::get_tmap_index(tkey.clone(),&self.kind)?;
        if self.tmap_pos != tmap_idx {
            log::debug!("goto {} of {}",tkey,self.kind);
            self.write_back_track();
            self.tmap_pos = tmap_idx;
            let idx = self.try_motor(tmap_idx)?;
            let bit_count = u16::from_le_bytes(self.trks.tracks[idx].bit_count) as usize;
            let mut new_cells = FluxCells::from_woz_bits(bit_count, &self.trks.tracks[idx].bits);
            if let Some(cells) = &self.cells {
                new_cells.sync_to_other_track(cells);
            }
            self.cells = Some(new_cells);
        }
        img::woz::get_motor_pos(tkey, &self.kind)
    }
}

impl img::DiskImage for Woz1 {
    fn track_count(&self) -> usize {
        let mut ans = 0;
        for trk in &self.trks.tracks {
            if trk.bit_count != [0;2] {
                ans += 1;
            }
        }
        ans
    }
    fn num_heads(&self) -> usize {
        1
    }
    fn motor_steps_per_cyl(&self) ->usize {
        4
    }
    fn byte_capacity(&self) -> usize {
        self.track_count()*16*256
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
    fn change_method(&mut self,method: img::tracks::Method) {
        self.engine.change_method(method);
    }
    fn change_format(&mut self,fmt: img::tracks::DiskFormat) -> STDRESULT {
        self.fmt = Some(fmt);
        Ok(())
    }
    fn read_block(&mut self,addr: crate::fs::Block) -> Result<Vec<u8>,DYNERR> {
        apple::read_block(self, addr)
    }
    fn write_block(&mut self, addr: crate::fs::Block, dat: &[u8]) -> STDRESULT {
        apple::write_block(self, addr, dat)
    }
    fn read_sector(&mut self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,DYNERR> {
        self.read_pro_sector(TrackKey::CH((cyl,head)),sec)
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &[u8]) -> STDRESULT {
        self.write_pro_sector(TrackKey::CH((cyl,head)),sec,dat)
    }
    fn read_pro_sector(&mut self,tkey: TrackKey,sec: usize) -> Result<Vec<u8>,DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        let [motor,head,_] = self.goto_track(tkey.clone())?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?;
        let skey = SectorKey::a2_525(254, u8::try_from((motor+1)/4)?);
        let ans = self.engine.read_sector(self.cells.as_mut().unwrap(),&skey,u8::try_from(sec)?,zfmt)?;
        Ok(ans)
    }
    fn write_pro_sector(&mut self,tkey: TrackKey,sec: usize,dat: &[u8]) -> Result<(),DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        let [motor,head,_] = self.goto_track(tkey.clone())?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?.clone();
        let skey = SectorKey::a2_525(254, u8::try_from((motor+1)/4)?);
        self.engine.write_sector(self.cells.as_mut().unwrap(),dat,&skey,u8::try_from(sec)?,&zfmt)?;
        Ok(())
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
        log::info!("identified WOZ v1 header");
        let mut ptr: usize= 12;
        while ptr>0 {
            let (next,id,maybe_chunk) = img::woz::get_next_chunk(ptr, buf);
            match (id,maybe_chunk) {
                (INFO_ID,Some(chunk)) => ans.info.update_from_bytes(&chunk)?,
                (TMAP_ID,Some(chunk)) => ans.tmap.update_from_bytes(&chunk)?,
                (TRKS_ID,Some(chunk)) => ans.trks.update_from_bytes(&chunk)?,
                (META_ID,Some(chunk)) => ans.meta = Some(chunk),
                _ => if id!=0 {
                    log::info!("unprocessed chunk with id {:08X}/{}",id,String::from_utf8_lossy(&u32::to_le_bytes(id)))
                }
            }
            ptr = next;
        }
        // leaving kind as unknown can lead to panics
        ans.kind = img::names::A2_DOS33_KIND;
        ans.sanity_check()?;
        for baseline_track in [0,3] {
            log::info!("baseline scan of track {}",baseline_track);
            if let Ok(Some(_sol)) = ans.get_track_solution(baseline_track) {
                log::info!("setting disk kind to {}",ans.kind);
                return Ok(ans);
            }
        }
        log::warn!("no baseline, continuing with {}",ans.kind);
        return Ok(ans);
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        self.write_back_track();
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
        Ok(self.get_trk_bits_ref(TrackKey::CH((cyl,head)))?.to_vec())
    }
    fn get_pro_track_buf(&mut self,tkey: TrackKey) -> Result<Vec<u8>,DYNERR> {
        Ok(self.get_trk_bits_ref(tkey)?.to_vec())
    }
    fn set_track_buf(&mut self,cyl: usize,head: usize,dat: &[u8]) -> STDRESULT {
        self.set_pro_track_buf(TrackKey::CH((cyl,head)),dat)
    }
    fn set_pro_track_buf(&mut self,tkey: TrackKey,dat: &[u8]) -> STDRESULT {
        let bits = self.get_trk_bits_mut(tkey)?;
        if bits.len()!=dat.len() {
            log::error!("source track buffer is {} bytes, destination track buffer is {} bytes",dat.len(),bits.len());
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        bits.copy_from_slice(dat);
        Ok(())
    }
    fn get_track_solution(&mut self,track: usize) -> Result<Option<img::TrackSolution>,DYNERR> {
        self.get_pro_track_solution(TrackKey::Track(track))
    }
    fn get_pro_track_solution(&mut self,tkey: TrackKey) -> Result<Option<img::TrackSolution>,DYNERR> {
        let [motor,head,width] = self.goto_track(tkey.clone())?;
        // First try the given format if it exists
        if let Some(fmt) = &self.fmt {
            log::trace!("try current format");
            let zfmt = fmt.get_zone_fmt(motor,head)?;
            if let Ok(chss_map) = self.engine.chss_map(self.cells.as_mut().unwrap(),zfmt) {
                return Ok(Some(zfmt.track_solution(motor,head,width,chss_map)));
            }
        }
        // If the given format fails try some standard ones
        log::trace!("try DOS 3.2 format");
        self.kind = img::names::A2_DOS32_KIND;
        self.fmt = img::woz::kind_to_format(&self.kind);
        let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
        if let Ok(chss_map) = self.engine.chss_map(self.cells.as_mut().unwrap(),zfmt) {
            if chss_map.len()==13 {
                return Ok(Some(zfmt.track_solution(motor,head,width,chss_map)));
            }
        }
        log::trace!("try DOS 3.3 format");
        self.kind = img::names::A2_DOS33_KIND;
        self.fmt = img::woz::kind_to_format(&self.kind);
        let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
        if let Ok(chss_map) = self.engine.chss_map(self.cells.as_mut().unwrap(),zfmt) {
            if chss_map.len()==16 {
                return Ok(Some(zfmt.track_solution(motor,head,width,chss_map)));
            }
        }
        return Ok(None);
    }
    fn export_geometry(&mut self,indent: Option<u16>) -> Result<String,DYNERR> {
        let pkg = img::package_string(&self.kind());
        let mut track_sols = Vec::new();
        // simple strategy, advance by full tracks if we find something,
        // advance by half tracks if not.
        let mut motor = 0;
        while motor < 160 {
            match self.get_pro_track_solution(TrackKey::Motor((motor,0))) {
                Ok(Some(sol)) => {
                    track_sols.push(sol);
                    motor += 4;
                },
                _ => {
                    motor += 2;
                }
            }
        }
        img::geometry_json(pkg,track_sols,indent)
    }
    fn get_track_nibbles(&mut self,cyl: usize,head: usize) -> Result<Vec<u8>,DYNERR> {
        self.get_pro_track_nibbles(TrackKey::CH((cyl,head)))
    }
    fn get_pro_track_nibbles(&mut self,tkey: TrackKey) -> Result<Vec<u8>,DYNERR> {
        let [motor,head,_] = self.goto_track(tkey.clone())?;
        let zfmt = img::tracks::get_zone_fmt(motor, head, &self.fmt)?;
        Ok(self.engine.to_nibbles(self.cells.as_mut().unwrap(), zfmt))
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        let tkey = super::woz::tkey_from_tmap_idx(self.tmap_pos, &self.kind);
        let [motor,head,_] = img::woz::get_motor_pos(tkey.clone(), &self.kind).expect("could not get head position");
        let zfmt = match img::tracks::get_zone_fmt(motor, head, &self.fmt) {
            Ok(z) => Some(z),
            _ => None
        };
        super::woz::track_string_for_display(0, &bytes, zfmt)
    }
    fn get_metadata(&self,indent: Option<u16>) -> String {
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
        if let Some(spaces) = indent {
            json::stringify_pretty(root,spaces)
        } else {
            json::stringify(root)
        }
    }
    fn put_metadata(&mut self,key_path: &Vec<String>,maybe_str_val: &json::JsonValue) -> STDRESULT {
        if let Some(val) = maybe_str_val.as_str() {
            log::debug!("put key `{:?}` with val `{}`",key_path,val);
            let woz1 = self.what_am_i().to_string();
            meta::test_metadata(key_path, self.what_am_i())?;
            if meta::match_key(key_path,&[&woz1,"info","disk_type"]) {
                log::warn!("skipping read-only `disk_type`");
                return Ok(());
            }
            if key_path.len()>2 && key_path[0]=="woz1" && key_path[1]=="info" {
                if !self.info.verify_value(&key_path[2], val) {
                    log::error!("INFO chunk key `{}` had a bad value `{}`",key_path[2],val);
                    return Err(Box::new(img::Error::MetadataMismatch));
                }
            }
            putByte!(val,key_path,woz1,self.info.write_protected);
            putByte!(val,key_path,woz1,self.info.synchronized);
            putByte!(val,key_path,woz1,self.info.cleaned);
            putStringBuf!(val,key_path,woz1,self.info.creator,0x20);
        }
        log::error!("unresolved key path {:?}",key_path);
        Err(Box::new(img::Error::MetadataMismatch))
    }
}
