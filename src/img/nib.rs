//! ## Support for NIB disk images
//! 
//! This uses the nibble machinery in module `disk525`. In particular, NIB and WOZ share
//! the same nibble routines.  The only difference is that NIB sets `sync_bits` to 8
//! regardless of the nibble encoding.

use a2kit_macro::DiskStructError;
use log::{debug,error};
// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
use crate::img::disk525;
use crate::img;
use crate::{STDRESULT,DYNERR};
use super::woz::HeadCoords;

pub const TRACK_BYTE_CAPACITY_NIB: usize = 6656;
pub const TRACK_BYTE_CAPACITY_NB2: usize = 6384;
const RCH: &str = "unreachable was reached";
 
pub fn file_extensions() -> Vec<String> {
    vec!["nib".to_string(),"nb2".to_string()]
}

pub struct Nib {
    kind: img::DiskKind,
    tracks: usize,
    trk_cap: usize,
    data: Vec<u8>,
    head_coords: HeadCoords
}

impl Nib {
    /// Create the image of a specific kind of disk (panics if unsupported disk kind).
    /// The volume is used to format the address fields on the tracks.
    pub fn create(vol: u8,kind: img::DiskKind) -> Self {
        if kind!=img::names::A2_DOS32_KIND && kind!=img::names::A2_DOS33_KIND {
            panic!("Nib can only accept 5.25 inch Apple formats")
        }
        let mut data: Vec<u8> = Vec::new();
        for track in 0..35 {
            let (mut buf,_obj) = match kind {
                img::names::A2_DOS32_KIND => disk525::format_std13_track(vol, track, TRACK_BYTE_CAPACITY_NIB,8),
                img::names::A2_DOS33_KIND => disk525::format_std16_track(vol, track, TRACK_BYTE_CAPACITY_NIB,8),
                _ => panic!("{}",RCH)
            };
            data.append(&mut buf);
        }
        Self {
            kind,
            tracks: 35,
            trk_cap: TRACK_BYTE_CAPACITY_NIB,
            data,
            head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
        }
    }
    /// Get a reference to the track bits
    fn get_trk_bits_ref(&self,track: u8) -> &[u8] {
        &self.data[track as usize * self.trk_cap..(track as usize+1) * self.trk_cap]
    }
    /// Get a mutable reference to the track bits
    fn get_trk_bits_mut(&mut self,track: u8) -> &mut [u8] {
        &mut self.data[track as usize * self.trk_cap..(track+1) as usize * self.trk_cap]
    }
    /// Create a lightweight trait object to read/write the bits.  The nibble format will be
    /// determined by the image's underlying `DiskKind`.
    fn new_rw_obj(&mut self,track: u8) -> Box<dyn super::TrackBits> {
        if self.head_coords.track != track as usize {
            debug!("goto track {} of {}",track,self.kind);
            self.head_coords.track = track as usize;
        }
        let bit_count = self.trk_cap * 8;
        let mut ans: Box<dyn super::TrackBits> = match self.kind {
            super::names::A2_DOS32_KIND => Box::new(disk525::TrackBits::create_nib(
                track as usize,
                bit_count,
                disk525::SectorAddressFormat::create_std13(),
                disk525::SectorDataFormat::create_std13())),
            super::names::A2_DOS33_KIND => Box::new(disk525::TrackBits::create_nib(
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

impl img::woz::WozUnifier for Nib {
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn num_tracks(&self) -> usize {
        self.tracks as usize
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

impl img::DiskImage for Nib {
    fn track_count(&self) -> usize {
        self.tracks
    }
    fn num_heads(&self) -> usize {
        1
    }
    fn byte_capacity(&self) -> usize {
        match self.kind {
            img::names::A2_DOS32_KIND => self.tracks*13*256,
            img::names::A2_DOS33_KIND => self.tracks*16*256,
            _ => panic!("disk type not supported")
        }
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::NIB
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
        match buf.len() {
            l if l==35*TRACK_BYTE_CAPACITY_NIB => {
                let mut disk = Self {
                    kind: img::names::A2_DOS33_KIND,
                    tracks: 35,
                    trk_cap: TRACK_BYTE_CAPACITY_NIB,
                    data: buf.to_vec(),
                    head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
                };
                if let Ok(Some(_sol)) = disk.get_track_solution(0) {
                    debug!("setting disk kind to {}",disk.kind);
                    return Ok(disk);
                } else {
                    debug!("Looks like NIB, but could not solve track 0");
                    return Err(DiskStructError::UnexpectedValue);
                }
            },
            l if l==35*TRACK_BYTE_CAPACITY_NB2 => {
                let mut disk = Self {
                    kind: img::names::A2_DOS33_KIND,
                    tracks: 35,
                    trk_cap: TRACK_BYTE_CAPACITY_NB2,
                    data: buf.to_vec(),
                    head_coords: HeadCoords { track: usize::MAX, bit_ptr: usize::MAX }
                };
                if let Ok(Some(_sol)) = disk.get_track_solution(0) {
                    debug!("setting disk kind to {}",disk.kind);
                    return Ok(disk);
                } else {
                    debug!("Looks like NB2, but could not solve track 0");
                    return Err(DiskStructError::UnexpectedValue);
                }
            }
            _ => {
                debug!("Buffer size {} fails to match nib or nb2",buf.len());
                Err(DiskStructError::UnexpectedSize)
            }
        }
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        self.data.clone()
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
}
