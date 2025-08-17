//! # Track Engines and Formats
//!
//! This module provides tools for working with tracks at the bitstream level.
//! The `DiskFormat` struct provides everything needed to create, read, and write disk tracks.
//! It breaks up into `ZoneFormat` structs that are often passed into track handling functions.
//! Functions are provided to create standard formats.
//! A format can also be created from a JSON string provided externally.
//! 
//! A key concept is that the format contains expressions describing a transformation
//! from standard sector addresses to proprietary ones.  This way a sector can always be
//! found using a standard address; the entity doing the seeking merely has to apply
//! the given transformation.
//! 
//! Standard sector addresses are not necessarily geometrical.
//! If there is an interleave on the standard track associated with this disk kind,
//! then standard sector 1 is not the geometrical neighbor or standard sector 2.

use crate::img;
use crate::DYNERR;
use crate::STDRESULT;
use std::fmt;
use std::str::FromStr;
use std::collections::HashMap;
use math_parse::MathParse;
use bit_vec::BitVec;

pub mod gcr;
mod formats;
mod parse_user_fmt;

fn eval_u8(expr: &str,ctx: &std::collections::HashMap<String,String>) -> Result<u8,DYNERR> {
    match MathParse::parse(expr) {
        Ok(parsed) => match parsed.solve_int(Some(ctx)) {
            Ok(ans) => match u8::try_from(ans) {
                Ok(ans) => Ok(ans),
                Err(_) => {
                    log::error!("{} evaluated to {} which is not a u8",expr,ans);
                    Err(Box::new(img::Error::MetadataMismatch))
                }
            }
            Err(e) => {
                log::error!("problem solving {}: {}",expr,e);
                if expr.contains("/") && !expr.contains("//") {
                    log::warn!("floating point division detected in user expression, use `//` for integer division")
                }
                log::debug!("variables: {:?}",ctx);
                Err(Box::new(img::Error::MetadataMismatch))
            }
        },
        Err(e) => {
            log::error!("problem parsing {}: {}",expr,e);
            Err(Box::new(img::Error::MetadataMismatch))
        }
    }
}

#[derive(Clone,PartialEq)]
pub enum Method {
    /// select based on track
    Auto,
    /// direct manipulation, good for textbook data streams
    Edit,
    /// emulate, but use hints the real system might not have
    Analyze,
    /// emulate the real system as near as possible
    Emulate,
}

impl FromStr for Method {
    type Err = super::Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "auto" => Ok(Method::Auto),
            "analyze" => Ok(Method::Analyze),
            "edit" => Ok(Method::Edit),
            "emulate" => Ok(Method::Emulate),
            _ => Err(super::Error::MetadataMismatch)
        }
    }
}

/// While this object is in the form of flux cells, it can actually be used for
/// any form of track data: flux streams, bit streams, or nibble streams.
/// Each bit in the stream represents a flux-cell where there may be a flux transition.
/// If `fshift < bshift`, we have a flux stream.
/// If `fshift == bshift`, we have a bit stream.
/// If uniform-length nibbles are tightly packed, we have a nibble stream.
pub struct FluxCells {
    /// each bit represents a window of time, bit value indicates whether there is a transition
    stream: BitVec,
    /// current location in the flux stream in units of ticks
    ptr: usize,
    /// emulated time in ticks, origin of time is up to caller
    time: u64,
    /// one revolution in units of ticks
    revolution: usize,
    /// defines a tick, fixing the basis of time in picoseconds
    tick_ps: usize,
    /// `1 << fshift` is length of a flux cell in ticks
    fshift: usize,
    /// `ptr & fmask == 0` indicates start of flux cell
    fmask: usize,
    /// `1 << bshift` is length of a bit cell in ticks
    bshift: usize,
    /// `ptr & bmask == 0` indicates start of bit cell
    bmask: usize,
}

impl FluxCells {
    /// Create a NIB or WOZ bitstream with 4 or 2 microsecond bit cells
    fn new_woz_bits(ptr: usize,stream: BitVec,time: u64,double_speed: bool) -> Self {
        let shft = double_speed as usize;
        let revolution = stream.len() << 5;
        Self {
            stream,
            ptr,
            time,
            revolution,
            tick_ps: 125000 >> shft,
            fshift: 5,
            fmask: (1 << 5) - 1,
            bshift: 5,
            bmask: (1 << 5) - 1,
        }
    }
    /// Create a WOZ fluxstream with 500 or 250 nanosecond flux cells
    fn new_woz_flux(ptr: usize,stream: BitVec,time: u64,double_speed: bool) -> Self {
        let shft = double_speed as usize;
        let revolution = stream.len() << 2;
        Self {
            stream,
            ptr,
            time,
            revolution,
            tick_ps: 125000 >> shft,
            fshift: 2,
            fmask: (1 << 2) - 1,
            bshift: 5,
            bmask: (1 << 5) - 1,
        }
    }
    /// Create cells from a track buffer in the form of a nibble stream or bit stream, assuming 4 microsecond bit cells
    pub fn from_woz_bits(bit_count: usize,buf: &[u8],time: u64,double_speed: bool) -> Self {
        let mut stream = BitVec::from_bytes(buf);
        stream.truncate(bit_count);
        Self::new_woz_bits(0,stream,time,double_speed)
    }
    /// Create cells from a WOZ flux track buffer, the flux cells will
    /// be set to 500 ns.  Any padding in `buf` is ignored.
    pub fn from_woz_flux(byte_count: usize,buf: &[u8],time: u64,double_speed: bool) -> Self {
        let mut stream = BitVec::new();
        let mut write = |carryover0: &mut usize, carryover1: &mut usize| {
            let ticks = *carryover0 + *carryover1;
            let cells = ticks / 4; // 500 ns cell
            for j in 0..cells {
                stream.push(j==0 && *carryover1 > 0 || j==cells-1);
            }
            *carryover0 = (cells>0) as usize * (ticks % 4);
            *carryover1 = (cells==0) as usize * (ticks % 4);
        };
        let mut carryover0 = 0;
        let mut carryover1 = 0;
        for segment in &buf[0..byte_count] {
            carryover0 += *segment as usize;
            if *segment!=255 {
                write(&mut carryover0,&mut carryover1);
            }
        }
        write(&mut carryover0,&mut carryover1);
        Self::new_woz_flux(0,stream,time,double_speed )
    }
    /// Change the resolution of the cells. Mainly useful for converting between bitstream tracks and flux tracks.
    /// N.b. if resolution is being reduced information will be lost, in general.
    pub fn change_resolution(&mut self,flux_shift: usize) {
        if flux_shift == self.fshift {
            return;
        } else if flux_shift < self.fshift {
            // resolution is higher
            let factor = 1 << self.fshift >> flux_shift;
            let mut new_stream = BitVec::new();
            for bit in &self.stream {
                new_stream.push(bit);
                new_stream.append(&mut BitVec::from_elem(factor-1,false));
            }
            self.fshift = flux_shift;
            self.fmask = (1 << flux_shift) - 1;
            self.stream = new_stream;
        } else {
            // resolution is lower
            let factor = 1 << flux_shift >> self.fshift;
            let new_len = self.stream.len() << self.fshift >> flux_shift;
            let mut new_stream = BitVec::new();
            for i in 0..new_len {
                let mut val = false;
                for j in 0..factor {
                    val |= self.stream[i*factor+j];
                }
                new_stream.push(val);
            }
            self.fshift = flux_shift;
            self.fmask = (1 << flux_shift) - 1;
            self.stream = new_stream;
        }
    }
    /// Convert the cells to the native WOZ or NIB track buffer, works
    /// for any kind of cell.  If `padded_len==None` the result is padded
    /// to the nearest 512 byte boundary.  If `padded_len==Some` and the data fits within
    /// the prescribed length, it is used, otherwise panic.
    /// Returns (buf,count), where count is either the
    /// bit count for bit streams, or byte count for flux streams.
    pub fn to_woz_buf(&self,padded_len: Option<usize>,padded_val: u8) -> (Vec<u8>,usize) {
        let (mut buf,count) = if self.fshift==self.bshift {
            (self.stream.to_bytes(),self.stream.len())
        } else {
            let mut zeroes = 0;
            let mut end = usize::MAX;
            let mut buf = Vec::new();
            // figure out where the last transition is and make that the end,
            // otherwise we could have a broken encoding
            for (i,cell) in self.stream.iter().rev().enumerate() {
                if cell {
                    end = self.stream.len() - i;
                    break;
                }
            }
            if end==usize::MAX {
                log::info!("no flux transitions on track");
                end = 1;
            }
            let mut iter_clos = |cell| {
                if cell {
                    // The following line presumes the tick count is inclusive of whatever time
                    // the transition takes, or else the transition is infinitessimal in duration. 
                    buf.push(zeroes + (1 << self.fshift));
                    zeroes = 0;
                } else {
                    zeroes += 1 << self.fshift;
                }
                if zeroes >= 0xff {
                    buf.push(0xff);
                    zeroes = zeroes % 0xff;
                }
            };
            for i in end..self.stream.len() {
                iter_clos(self.stream[i]);
            }
            for i in 0..end {
                iter_clos(self.stream[i]);
            }
            let count = buf.len();
            (buf,count)
        };
        let padding = match (buf.len(),padded_len) {
            (l,Some(tot)) if l<=tot => tot-l,
            (l,Some(tot)) => panic!("buffer too small {}/{}",l,tot),
            (l,_) if l==0 => 512,
            (l,_) => ((l-1)/512)*512 + 512 - l
        };
        buf.append(&mut vec![padded_val;padding]);
        (buf,count)
    }
    /// Synchronize these cells to another set of cells, used when switching tracks.
    /// This will impose alignment of the time-pointer to a flux cell boundary.
    pub fn sync_to_other_track(&mut self,other: &FluxCells) {
        self.ptr = (other.ptr * self.revolution / other.revolution) >> self.fshift << self.fshift;
    }
    /// How many cells are on this track
    pub fn count(&self) -> usize {
        self.stream.len()
    }
    pub fn set_ptr(&mut self,ticks: usize) {
        self.ptr = ticks;
    }
    /// advance on the track by `ticks` and update the elapsed time
    pub fn fwd(&mut self,ticks: usize) {
        self.ptr = (self.ptr + ticks) % self.revolution;
        self.time += ticks as u64;
    }
    /// go back on the track by `ticks`, this will also reverse the elapsed time
    pub fn rev(&mut self,ticks: usize) {
        self.ptr = (self.ptr + self.revolution - ticks) % self.revolution;
        self.time -= ticks as u64;
    }
    /// ticks since the reference tick
    pub fn ticks_since(&self,ref_tick: u64) -> u64 {
        self.time - ref_tick
    }
    /// picoseconds since the reference tick
    pub fn ps_since(&self,ref_tick: u64) -> u64 {
        self.tick_ps as u64 * (self.time - ref_tick)
    }
    /// emit a pulse from the current bit cell and advance
    pub fn read_bit(&mut self) -> bool {
        let cells_per_bit = 1 << self.bshift >> self.fshift;
        let mut ans = false;
        for _ in 0..cells_per_bit {
            ans |= self.stream[self.ptr >> self.fshift];
            self.fwd(1 << self.fshift);
        }
        ans
    }
    /// write a pulse to the current bit cell and advance
    pub fn write_bit(&mut self,pulse: bool) {
        let cells_per_bit = 1 << self.bshift >> self.fshift;
        for _ in 0..cells_per_bit {
            self.stream.set(self.ptr >> self.fshift,pulse && (self.ptr & self.bmask==0));
            self.fwd(1 << self.fshift);
        }
    }
}

/// Encapsulates 3 ways a track might be idenfified
#[derive(Clone,PartialEq)]
pub enum TrackKey {
    /// single index to a track, often `C * num_heads + H`
    Track(usize),
    /// cylinder and head
    CH((usize, usize)),
    /// stepper motor position and head, needed for, e.g., WOZ quarter tracks
    Motor((usize, usize)),
}

/// Contains standard values that are used in forming a sector address.
/// The ordinary sector number itself is supplied separately.
/// Format objects determine how these values map to a specific address.
pub struct SectorKey {
    vol: u8,
    cyl: u8,
    head: u8,
    aux: u8
}

/// bit pattern that marks off a sector address or data run,
/// for FM/MFM the pattern shall include clock pulses
#[derive(Clone)]
struct SectorMarker {
    key: Vec<u8>,
    mask: Vec<u8>,
}

/// Format of a contiguous set of tracks.
#[derive(Clone)]
pub struct ZoneFormat {
    flux_code: img::FluxCode,
    addr_nibs: img::FieldCode,
    data_nibs: img::FieldCode,
    speed_kbps: usize,
    motor_start: usize,
    motor_end: usize,
    motor_step: usize,
    heads: Vec<usize>,
    /// Ordered expressions used to calculate sector address bytes for use during formatting, including checksum.
    /// The expression give the decoded bytes, in order, in terms standard variables (vol,cyl,head,sec,aux).
    /// For complex CRC bytes, some identifier will have to be used in place of an expression.
    addr_fmt_expr: Vec<String>,
    /// Ordered expressions used to calculate sector address bytes for use during seeking, including checksum.
    /// In addition to (vol,cyl,head,sec,aux), variables may include (a0,a1,a2,...).  The latter refer to the
    /// actual address values.  These will generally be used in the checksum, and can
    /// also be used to effectively mask out bits you don't need to match.
    addr_seek_expr: Vec<String>,
    /// In most cases this is simply `["dat"]`, which means sector data and checksum.
    /// We do not try to describe data checksums here as they can be very complex.
    /// Any other expressions are evaluated as byte values and packed into the data field in the order given.
    data_expr: Vec<String>,
    /// fixed markers used to identify address start, address stop, data start, data stop
    markers: [SectorMarker; 4],
    /// gaps at start of track, end of sector, end of data (often for syncing)
    gaps: [BitVec; 3],
    /// Ordered expressions used to calculate human readable address bytes.
    /// Variables may include (a0,a1,a2,...), i.e., the actual address bytes.
    addr_extract_expr: Vec<String>,
    /// When reading replace `swap_nibs[i][0]` with `swap_nibs[i][1]`, when writing do the opposite.
    swap_nibs: Vec<[u8;2]>,
    /// Capacity of each sector, in some cases the possible values are tightly constrained
    capacity: Vec<usize>
}

/// Format of a disk broken up into zones.
#[derive(Clone)]
pub struct DiskFormat {
    zones: Vec<ZoneFormat>
}

impl fmt::Display for TrackKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CH((c, h)) => write!(f, "cyl {} head {}", c, h),
            Self::Motor((m, h)) => write!(f, "motor-pos {} head {}", m, h),
            Self::Track(t) => write!(f, "track {}", t),
        }
    }
}

impl PartialOrd for TrackKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self,other) {
            (Self::CH(x),Self::CH(y)) => x.partial_cmp(y),
            (Self::Motor(x),Self::Motor(y)) => x.partial_cmp(y),
            (Self::Track(x),Self::Track(y)) => x.partial_cmp(y),
            _ => None
        }
    }
}

impl TrackKey {
    /// jump in units of normal cylinder separation, if this is a `Track` discriminant an error is returned
    pub fn jump(&mut self, cyls: isize, new_head: Option<usize>, steps_per_cyl: usize) -> STDRESULT {
        match self {
            Self::CH((c,h)) => {
                *c = usize::try_from(*c as isize + cyls)?;
                if let Some(head) = new_head {
                    *h = head;
                }
                Ok(())
            },
            Self::Motor((m,h)) => {
                *m = usize::try_from(*m as isize + cyls * steps_per_cyl as isize)?;
                if let Some(head) = new_head {
                    *h = head;
                }
                Ok(())
            },
            _ => Err(Box::new(crate::commands::CommandError::InvalidCommand))
        }
    }
}

impl fmt::Display for SectorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{},{},{}",self.vol,self.cyl,self.head,self.aux)
    }
}

impl SectorKey {
    fn get_fmt_vars(&self,sec: u8) -> Result<HashMap<String,String>,DYNERR> {
        let mut ctx = HashMap::new();
        ctx.insert("vol".to_string(),u8::to_string(&self.vol));
        ctx.insert("cyl".to_string(),u8::to_string(&self.cyl));
        ctx.insert("head".to_string(),u8::to_string(&self.head));
        ctx.insert("sec".to_string(),u8::to_string(&sec));
        ctx.insert("aux".to_string(),u8::to_string(&self.aux));
        Ok(ctx)
    }
    fn get_seek_vars(&self,sec: u8,actual: &[u8]) -> Result<HashMap<String,String>,DYNERR> {
        let mut ctx = HashMap::new();
        ctx.insert("vol".to_string(),u8::to_string(&self.vol));
        ctx.insert("cyl".to_string(),u8::to_string(&self.cyl));
        ctx.insert("head".to_string(),u8::to_string(&self.head));
        ctx.insert("sec".to_string(),u8::to_string(&sec));
        ctx.insert("aux".to_string(),u8::to_string(&self.aux));
        for i in 0..actual.len() {
            ctx.insert(["a",&usize::to_string(&i)].concat(),u8::to_string(&actual[i]));
        }
        Ok(ctx)
    }
    pub fn head(&self) -> u8 {
        self.head
    }
    pub fn a2_525(vol: u8,trk: u8) -> Self {
        Self {
            vol,
            cyl: trk,
            head: 0,
            aux: 0
        }
    }
    pub fn a2_35(cyl: u8,head: u8) -> Self {
        Self {
            vol: 0,
            cyl,
            head,
            aux: 0
        }
    }
    pub fn c64(cyl: u8,aux: u8) -> Self {
        Self {
            vol: 0,
            cyl,
            head: 0,
            aux
        }
    }
    pub fn ibm(cyl: u8,head: u8,aux: u8) -> Self {
        Self {
            vol: 0,
            cyl,
            head,
            aux
        }
    }
}

impl ZoneFormat {
    pub fn check_flux_code(&self,flux_code: img::FluxCode) -> STDRESULT {
        if flux_code == self.flux_code {
            Ok(())
        } else {
            Err(Box::new(super::Error::ImageTypeMismatch))
        }
    }
    /// returns (key,mask)
    fn get_marker(&self, which: usize) -> (&[u8],&[u8]) {
        (
            &self.markers[which].key,
            &self.markers[which].mask
        )
    }
    fn get_gap_bits(&self, which: usize) -> &BitVec {
        &self.gaps[which]
    }
    /// Get a sector address field appropriate for use in formatting.
    /// The inputs are transformed by expressions stored with the format.
    /// The formatter may rearrange the address, but it does *not* encode the address.
    /// The formatter can also compute simple checksums.
    fn get_addr_for_formatting(&self,skey: &SectorKey,sec: u8) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let ctx = skey.get_fmt_vars(sec)?;
        for expr in &self.addr_fmt_expr {
            ans.push(eval_u8(expr,&ctx)?);
        }
        Ok(ans)
    }
    /// Return any information (not markers) that should precede the sector data (often none).
    fn get_data_header(&self,skey: &SectorKey,sec: u8) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let ctx = skey.get_fmt_vars(sec)?;
        // work out every byte until we encounter "dat"
        for expr in &self.data_expr {
            if expr == "dat" {
                return Ok(ans);
            } else {
                ans.push(eval_u8(expr,&ctx)?);
            }
        }
        Ok(ans)
    }
    /// Return a version of the address that might be transformed for human interpretation
    fn get_nice_addr(&self,addr: &Vec<u8>) -> Result<Vec<u8>,DYNERR> {
        if self.addr_extract_expr.len() < 3 || self.addr_extract_expr.len() > 4 {
            log::error!("addr_extract_expr in format should have 3 or 4 elements");
            return Err(Box::new(crate::commands::CommandError::InvalidCommand));
        }
        let mut ctx = HashMap::new();
        for i in 0..addr.len() {
            ctx.insert(["a",&usize::to_string(&i)].concat(),u8::to_string(&addr[i]));
        }
        let mut ans = Vec::new();
        for expr in &self.addr_extract_expr {
            ans.push(eval_u8(expr,&ctx)?);
        }
        Ok(ans)
    }
    /// Get a sector address field to be matched against an actual address field during seeking.
    /// The inputs are transformed by expressions stored with the format. These transformations
    /// can (and usually do) involve `actual`, which should be the decoded address field actually found.
    /// In particular, checksums are normally matched against the checksum of the actual bytes.
    fn get_addr_for_seeking(&self,skey: &SectorKey,sec: u8,actual: &[u8]) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let ctx = skey.get_seek_vars(sec,actual)?;
        // work out every byte except "chk"; if we find "chk" save the index where it occurs.
        for expr in &self.addr_seek_expr {
            ans.push(eval_u8(expr,&ctx)?);
        }
        Ok(ans)
    }
    /// Returns `(actual ^ pattern)` for each address byte.  If all bytes are 0 this is a match.
    /// This will transform (but not encode) the arguments according the expressions stored with this format before comparing.
    fn diff_addr(&self, skey: &SectorKey, sec: u8, actual: &[u8]) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let pattern = self.get_addr_for_seeking(skey,sec,actual)?;
        if pattern.len() != actual.len() {
            log::error!("lengths did not match during address comparison");
            return Err(Box::new(super::Error::SectorAccess));
        }
        for i in 0..pattern.len() {
            ans.push(actual[i] ^ pattern[i]);
        }
        Ok(ans)
    }
    fn addr_nibs(&self) -> img::FieldCode {
        self.addr_nibs
    }
    fn data_nibs(&self) -> img::FieldCode {
        self.data_nibs
    }
    /// `sec` is sector id before any format transformation happens, and is used as the index
    /// into the capacity vector.  Wrap around is used to always give an answer.  Will panic if
    /// the capacity vector is empty.
    fn capacity(&self,sec: usize) -> usize {
        self.capacity[sec % self.capacity.len()]
    }
    pub fn sector_count(&self) -> usize {
        self.capacity.len()
    }
    pub fn track_solution(&self,motor: usize,head: usize,head_width: usize,addr_map: Vec<[u8;4]>,size_map: Vec<usize>,addr_type: &str) -> img::TrackSolution {
        img::TrackSolution {
            cylinder: motor/head_width,
            fraction: [motor%head_width,head_width],
            head,
            speed_kbps: self.speed_kbps,
            flux_code: self.flux_code,
            addr_code: self.addr_nibs,
            data_code: self.data_nibs,
            addr_type: addr_type.to_string(),
            addr_map,
            size_map
        }
    }
    /// see if `win` matches marker `which` at any stage and return the mnemonic.
    /// update the marker information if matching the final byte.
    fn chk_marker(&self, i: usize, win: &[u8;5], which: usize, mnemonic: &[char], fallback: char, last_marker: &mut usize, last_marker_end: &mut usize) -> char {
        let count = usize::min(3,self.markers[which].key.len());
        for stage in 0..count {
            let mut matching = true;
            for i in 0..count {
                let y = self.markers[which].key[i];
                let mask = self.markers[which].mask[i];
                matching &= win[2-stage+i] & mask == y & mask;
            }
            if matching {
                if stage+1==count {
                    *last_marker += 1;
                    *last_marker_end = i + 1;
                    if *last_marker > 3 {
                        *last_marker = 0;
                    }
                }
                return mnemonic[stage%mnemonic.len()];
            }
        }
        fallback
    }
    /// Analyzes a neighborhood of the WOZ-like nibble stream given the most recent marker that has been seen,
    /// and produce a character that can be used to guide the eye to interesting regions in the stream.
    /// The `last_marker` 0 means starting or data epilog found, and so on in sequence
    pub fn woz_mnemonic(&self,buf: &[u8],i: usize,last_marker: &mut usize,last_marker_end: &mut usize) -> char {
        let hexdigit = |x| match x {
            x if x < 10 => char::from_u32(x as u32 + 48).unwrap_or('^'),
            x if x < 16 => char::from_u32(x as u32 + 87).unwrap_or('^'),
            _ => '^'
        };
        let addr_nib_count = match self.addr_nibs() {
            img::FieldCode::WOZ((4,4)) => 2*self.addr_fmt_expr.len(),
            _ => self.addr_fmt_expr.len()
        };
        let data_nib_count = match (self.capacity(0),self.data_nibs()) {
            (256,img::FieldCode::WOZ((4,4))) => 512,
            (256,img::FieldCode::WOZ((5,3))) => 411,
            (256,img::FieldCode::WOZ((6,2))) => 343,
            (524,img::FieldCode::WOZ((6,2))) => 703,
            _ => 343
        };
        let mut win = [0;5];
        for rel in 0..5 {
            let abs = i as isize - 2 + rel;
            if abs >= 0 && abs < buf.len() as isize {
                win[rel as usize] = buf[abs as usize];
            }
        }
        let invalid = gcr::decode(buf[i] as usize + 0xaa00, &self.data_nibs).is_err();
        let mut fallback = match (invalid,buf[i]) {
            (true,0xd5) => 'R',
            (true,0xaa) => 'R',
            (true,_) => '?',
            _ => '.'
        };
        // if we have been looking for a data prolog too long give up and look for next address prolog
        if *last_marker == 2 && i > *last_marker_end + 40 {
            *last_marker = 0;
        }
        if *last_marker == 0 {
            // DA gap
            if win[2] == 0xff {
                fallback = '>';
            }
            self.chk_marker(i,&win,0,&['(','A',':'],fallback,last_marker,last_marker_end)
        } else if *last_marker==1 && i < *last_marker_end + addr_nib_count {
            // address field
            match self.addr_nibs() {
                img::FieldCode::WOZ((4,4)) => {
                    if (i-*last_marker_end)%2 == 0 {
                        let val = buf[i] as usize * 256 + win[3] as usize;
                        hexdigit(gcr::decode(val,&self.addr_nibs).unwrap_or(0) >> 4)
                    } else {
                        let val = win[1] as usize * 256 + buf[i] as usize;
                        hexdigit(gcr::decode(val,&self.addr_nibs).unwrap_or(0) & 0x0f)
                    }
                },
                _ => hexdigit(gcr::decode(buf[i] as usize,&self.addr_nibs).unwrap_or(0))
            }
        } else if *last_marker==1 {
            // address epilog
            self.chk_marker(i,&win,1,&[':','A',')'],fallback,last_marker,last_marker_end)
        } else if *last_marker==2 {
            // AD gap
            if win[2] == 0xff {
                fallback = '>';
            }
            self.chk_marker(i,&win,2,&['(','D',':'],fallback,last_marker,last_marker_end)
        } else if *last_marker==3 && i < *last_marker_end + data_nib_count {
            // data field
            fallback
        } else if *last_marker==3 {
            // data epilog
            self.chk_marker(i,&win,3,&[':','D',')'],fallback,last_marker,last_marker_end)
        } else {
            fallback
        }
    }
}

/// short cut to get a ZoneFormat from a maybe DiskFormat 
pub fn get_zone_fmt<'a>(motor: usize,head: usize,fmt: &'a Option<DiskFormat>) -> Result<&'a ZoneFormat,DYNERR> {
	match fmt {
		Some(f) => Ok(f.get_zone_fmt(motor,head)?),
		None => Err(Box::new(img::Error::SectorAccess))
	}
}

impl<'a> DiskFormat {
    pub fn get_zone_fmt(&'a self,motor: usize,head: usize) -> Result<&'a ZoneFormat,DYNERR> {
        for zone in &self.zones {
            if motor >= zone.motor_start && motor < zone.motor_end && zone.heads.contains(&head) {
                return Ok(zone)
            }
        }
        log::error!("zone at motor pos {} not found",motor);
        return Err(Box::new(super::Error::SectorAccess))
    }
    /// concatenate all the (motor,head) tuples for all the zones
    pub fn get_motor_and_head(&self) -> Vec<(usize,usize)> {
        let mut ans = Vec::new();
        for zone in &self.zones {
            for m in (zone.motor_start..zone.motor_end).step_by(zone.motor_step) {
                for h in &zone.heads {
                    ans.push((m,*h));
                }
            }
        }
        ans
    }
}
