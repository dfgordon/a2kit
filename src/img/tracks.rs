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
    /// Ordered expressions used to calculate CHS values as they appear in address fields.
    /// This is straightforward for IBM disks, for others we align the values as best we can.
    /// Variables may include (a0,a1,a2,...), i.e., the actual address values.
    chs_extract_expr: Vec<String>,
    /// When reading replace `swap_nibs[i][0]` with `swap_nibs[i][1]`, when writing do the opposite.
    swap_nibs: Vec<[u8;2]>,
    /// When the file system asks for cylinder `x`, go to `x + cyl_shift[x]`
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
    /// Return whatever is in the CHS bytes for this address
    fn get_chs(&self,addr: &Vec<u8>) -> Result<[u8;3],DYNERR> {
        if self.chs_extract_expr.len() != 3 {
            log::error!("chs_expr in format should have 3 elements");
            return Err(Box::new(crate::commands::CommandError::InvalidCommand));
        }
        let mut ctx = HashMap::new();
        for i in 0..addr.len() {
            ctx.insert(["a",&usize::to_string(&i)].concat(),u8::to_string(&addr[i]));
        }
        Ok([
            eval_u8(&self.chs_extract_expr[0],&ctx)?,
            eval_u8(&self.chs_extract_expr[1],&ctx)?,
            eval_u8(&self.chs_extract_expr[2],&ctx)?
        ])
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
    pub fn track_solution(&self,motor: usize,head: usize,head_width: usize,chss_map: Vec<[usize;4]>) -> img::TrackSolution {
        img::TrackSolution {
            cylinder: motor/head_width,
            fraction: [motor%head_width,head_width],
            head,
            flux_code: self.flux_code,
            addr_code: self.addr_nibs,
            data_code: self.data_nibs,
            chss_map
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
