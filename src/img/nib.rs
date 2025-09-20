//! ## Support for NIB disk images
//! 
//! NIB tracks contain a filtered bitstream, i.e., leading bits that resolve to 0 are thrown out.
//! We handle these tracks using the same track engine that handles WOZ tracks.  The trick is to
//! set the sync-byte length to 8 bits.  This works so long as the NIB track has been properly aligned,
//! and we are careful to start the bit pointer on a multiple of 8.
//! 
//! Nowadays use of NIB is discouraged because of its incomplete representation of the bit stream.

use a2kit_macro::DiskStructError;
use crate::img;
use crate::img::{Track,Sector,SectorHood};
use crate::img::tracks::{Method,FluxCells};
use crate::img::tracks::gcr::TrackEngine;
use crate::{STDRESULT,DYNERR};

pub const TRACK_BYTE_CAPACITY_NIB: usize = 6656;
pub const TRACK_BYTE_CAPACITY_NB2: usize = 6384;
 
pub fn file_extensions() -> Vec<String> {
    vec!["nib".to_string(),"nb2".to_string()]
}

pub struct Nib {
    kind: img::DiskKind,
    fmt: Option<img::tracks::DiskFormat>,
    tracks: usize,
    trk_cap: usize,
    data: Vec<u8>,
    /// state: controller
    engine: TrackEngine,
    /// state: current track data and angle
    cells: FluxCells,
    /// state: current track
    track_pos: usize,
}

impl Nib {
    /// Create the image of a specific kind of disk (panics if unsupported disk kind).
    /// The volume is used to format the address fields on the tracks.
    pub fn create(vol: u8,kind: img::DiskKind) -> Result<Self,DYNERR> {
        let fmt = match kind {
            img::names::A2_DOS32_KIND => img::tracks::DiskFormat::apple_525_13(8),
            img::names::A2_DOS33_KIND => img::tracks::DiskFormat::apple_525_16(8),
            _ => {
                log::error!("Nib can only accept 5.25 inch Apple formats");
                return Err(Box::new(img::Error::ImageTypeMismatch));
            }
        };
        let mut data: Vec<u8> = Vec::new();
        let mut init_cells = None;
        let mut engine = TrackEngine::create(Method::Fast,true);
        for track in 0..35 {
            let hood = SectorHood::a2_525(vol, track);
            let zfmt = fmt.get_zone_fmt(0, 0)?;
            let cells = engine.format_track(hood, TRACK_BYTE_CAPACITY_NIB,&zfmt)?;
            let (mut buf,_) = cells.to_woz_buf(Some(TRACK_BYTE_CAPACITY_NIB),0xff);
            if track==0 {
                init_cells = Some(cells);
            }
            data.append(&mut buf);
        }
        Ok(Self {
            kind,
            fmt: Some(fmt),
            tracks: 35,
            trk_cap: TRACK_BYTE_CAPACITY_NIB,
            data,
            engine,
            cells: init_cells.unwrap(),
            track_pos: 0,
        })
    }
    fn try_track(&self,trk: Track) -> Result<usize,DYNERR> {
        match trk {
            Track::Motor((m,h)) if m%4==0 && m < 140 && h==0 => Ok(m/4),
            Track::CH((c,h)) if c < 35 && h==0 => Ok(c),
            Track::Num(t) if t < 35 => Ok(t),
            _ => {
                log::error!("Nib image could not handle track key {}",trk);
                Err(Box::new(img::Error::ImageTypeMismatch))
            }
        }
    }
    /// Get a reference to the track bits
    fn get_trk_bits_ref(&self,trk: Track) -> Result<&[u8],DYNERR> {
        let track = self.try_track(trk)?;
        Ok(&self.data[track * self.trk_cap..(track+1) * self.trk_cap])
    }
    /// Get a mutable reference to the track bits
    fn get_trk_bits_mut(&mut self,trk: Track) -> Result<&mut [u8],DYNERR> {
        let track = self.try_track(trk)?;
        Ok(&mut self.data[track * self.trk_cap..(track+1) * self.trk_cap])
    }
    /// Save changes to the track
    fn write_back_track(&mut self) {
        let track = self.track_pos;
        let (src,_) = self.cells.to_woz_buf(Some(self.trk_cap), 0xff);
        self.data[track * self.trk_cap..(track+1) * self.trk_cap].copy_from_slice(&src);
    }
    /// Goto track and extract FluxCells if necessary, returns [motor,head,width]
    fn goto_track(&mut self,trk: Track) -> Result<[usize;3],DYNERR> {
        let track = self.try_track(trk)?;
        if self.track_pos != track {
            log::debug!("goto track {} of {}",track,self.kind);
            self.write_back_track();
            self.track_pos = track;
            let bit_count = self.trk_cap * 8;
            self.cells = FluxCells::from_woz_bits(bit_count,self.get_trk_bits_ref(trk)?,0,false);
        }
        img::woz::get_motor_pos(trk, &self.kind)
    }
}

impl img::DiskImage for Nib {
    fn track_count(&self) -> usize {
        self.tracks
    }
    fn end_track(&self) -> usize {
        self.tracks
    }
    fn num_heads(&self) -> usize {
        1
    }
    fn nominal_capacity(&self) -> Option<usize> {
        match self.kind {
            img::names::A2_DOS32_KIND => Some(self.tracks*13*256),
            img::names::A2_DOS33_KIND => Some(self.tracks*16*256),
            _ => None
        }
    }
    fn actual_capacity(&mut self) -> Result<usize,DYNERR> {
        match self.nominal_capacity() {
            Some(ans) => Ok(ans),
            None => Err(Box::new(img::Error::DiskKindMismatch))
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
    fn read_block(&mut self,addr: crate::bios::Block) -> Result<Vec<u8>,DYNERR> {
        crate::bios::blocks::apple::read_block(self, addr)
    }
    fn write_block(&mut self, addr: crate::bios::Block, dat: &[u8]) -> STDRESULT {
        crate::bios::blocks::apple::write_block(self, addr, dat)
    }
    fn read_sector(&mut self,trk: Track,sec: Sector) -> Result<Vec<u8>,DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        self.goto_track(trk)?;
        let [motor,head,_] = img::woz::get_motor_pos(trk, &self.kind)?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?;
        let hood = SectorHood::a2_525(254, u8::try_from((motor+1)/4)?);
        let ans = self.engine.read_sector(&mut self.cells,&hood,&sec,zfmt)?;
        Ok(ans)
    }
    fn write_sector(&mut self,trk: Track,sec: Sector,dat: &[u8]) -> Result<(),DYNERR> {
        if self.fmt.is_none() {
            return Err(Box::new(img::Error::UnknownDiskKind));
        }
        let [motor,head,_] = self.goto_track(trk)?;
        let fmt = self.fmt.as_ref().unwrap(); // guarded above 
        let zfmt = fmt.get_zone_fmt(motor,head)?.clone();
        let hood = SectorHood::a2_525(254, u8::try_from((motor+1)/4)?);
        self.engine.write_sector(&mut self.cells,dat,&hood,&sec,&zfmt)?;
        Ok(())
    }
    fn from_bytes(buf: &[u8]) -> Result<Self,DiskStructError> where Self: Sized {
        match buf.len() {
            l if l==35*TRACK_BYTE_CAPACITY_NIB => {
                let data = buf.to_vec();
                let cells = FluxCells::from_woz_bits(TRACK_BYTE_CAPACITY_NIB*8, &data[0..TRACK_BYTE_CAPACITY_NIB],0,false);
                let mut disk = Self {
                    kind: img::names::A2_DOS33_KIND,
                    fmt: Some(img::tracks::DiskFormat::apple_525_16(8)),
                    tracks: 35,
                    trk_cap: TRACK_BYTE_CAPACITY_NIB,
                    data,
                    engine: TrackEngine::create(Method::Fast, true),
                    cells,
                    track_pos: 0,
                };
                match disk.get_track_solution(Track::Num(0)) { Ok(_) => {
                    log::debug!("setting disk kind to {}",disk.kind);
                    return Ok(disk);
                } _ => {
                    log::debug!("Looks like NIB, but could not solve track 0");
                    return Err(DiskStructError::UnexpectedValue);
                }}
            },
            l if l==35*TRACK_BYTE_CAPACITY_NB2 => {
                let data = buf.to_vec();
                let cells = FluxCells::from_woz_bits(TRACK_BYTE_CAPACITY_NB2*8, &data[0..TRACK_BYTE_CAPACITY_NB2],0,false);
                let mut disk = Self {
                    kind: img::names::A2_DOS33_KIND,
                    fmt: Some(img::tracks::DiskFormat::apple_525_16(8)),
                    tracks: 35,
                    trk_cap: TRACK_BYTE_CAPACITY_NB2,
                    data,
                    engine: TrackEngine::create(Method::Fast, true),
                    cells,
                    track_pos: usize::MAX
                };
                match disk.get_track_solution(Track::Num(0)) { Ok(_) => {
                    log::debug!("setting disk kind to {}",disk.kind);
                    return Ok(disk);
                } _ => {
                    log::debug!("Looks like NB2, but could not solve track 0");
                    return Err(DiskStructError::UnexpectedValue);
                }}
            }
            _ => {
                log::debug!("Buffer size {} fails to match nib or nb2",buf.len());
                Err(DiskStructError::UnexpectedSize)
            }
        }
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        self.write_back_track();
        self.data.clone()
    }
    fn get_track_buf(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR> {
        Ok(self.get_trk_bits_ref(trk)?.to_vec())
    }
    fn set_track_buf(&mut self,trk: Track,dat: &[u8]) -> STDRESULT {
        let bits = self.get_trk_bits_mut(trk)?;
        if bits.len()!=dat.len() {
            log::error!("source track buffer is {} bytes, destination track buffer is {} bytes",dat.len(),bits.len());
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        bits.copy_from_slice(dat);
        Ok(())
    }
    fn get_track_solution(&mut self,trk: Track) -> Result<img::TrackSolution,DYNERR> {
        let [motor,head,_] = self.goto_track(trk)?;
        // First try the given format if it exists
        if let Some(fmt) = &self.fmt {
            log::trace!("try current format");
            let zfmt = fmt.get_zone_fmt(motor,head)?;
            if let Ok((addr_map,size_map)) = self.engine.get_sector_map(&mut self.cells,zfmt) {
                return Ok(zfmt.track_solution(addr_map,size_map,"VTSK",[255,255,255,255,0,0],None));
            }
        }
        // If the given format fails try some standard ones
        log::trace!("try DOS 3.2 format");
        self.kind = img::names::A2_DOS32_KIND;
        self.fmt = img::woz::kind_to_format(&self.kind);
        let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
        if let Ok((addr_map,size_map)) = self.engine.get_sector_map(&mut self.cells,zfmt) {
            if addr_map.len()==13 {
                return Ok(zfmt.track_solution(addr_map,size_map,"VTSK",[255,255,255,255,0,0],None));
            }
        }
        log::trace!("try DOS 3.3 format");
        self.kind = img::names::A2_DOS33_KIND;
        self.fmt = img::woz::kind_to_format(&self.kind);
        let zfmt = img::tracks::get_zone_fmt(motor,head,&self.fmt)?;
        if let Ok((addr_map,size_map)) = self.engine.get_sector_map(&mut self.cells,zfmt) {
            if addr_map.len()==16 {
                return Ok(zfmt.track_solution(addr_map,size_map,"VTSK",[255,255,255,255,0,0],None));
            }
        }
        return Ok(img::TrackSolution::Unsolved);
    }
    fn export_geometry(&mut self,indent: Option<u16>) -> Result<String,DYNERR> {
        let pkg = img::package_string(&self.kind());
        let mut track_sols = Vec::new();
        for track in 0..self.tracks {
            let sol = self.get_track_solution(Track::Num(track))?;
            let [c,h] = self.get_rz(Track::Num(track))?;
            track_sols.push((c as f64,h,sol));
        }
        img::geometry_json(pkg,track_sols,35,1,1,indent)
    }
    fn export_format(&self,indent: Option<u16>) -> Result<String,DYNERR> {
        match &self.fmt {
            Some(f) => f.to_json(indent),
            None => Err(Box::new(super::Error::UnknownFormat))
        }
    }
    fn get_track_nibbles(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR> {
        let [motor,head,_] = self.goto_track(trk)?;
        let zfmt = img::tracks::get_zone_fmt(motor, head, &self.fmt)?;
        Ok(self.engine.to_nibbles(&mut self.cells, zfmt))
    }
    fn display_track(&self,bytes: &[u8]) -> String {
        let trk = Track::Num(self.track_pos);
        let [motor,head,_] = img::woz::get_motor_pos(trk, &self.kind).expect("could not get head position");
        let zfmt = match img::tracks::get_zone_fmt(motor, head, &self.fmt) {
            Ok(z) => Some(z),
            _ => None
        };
        super::woz::track_string_for_display(0, &bytes, zfmt)
    }
}
