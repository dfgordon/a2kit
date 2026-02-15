//! # Disk Image Module
//! 
//! Disk images are represented by objects implementing the `DiskImage` trait.
//! The object type is usually named for the disk image type that it handles, e.g., `Woz2`.
//! This object is perhaps best thought of as a disk plus all the hardware and some of the
//! low level software that runs it.
//! 
//! ## Basic Functions
//! 
//! The trait includes reading and writing tracks, sectors, and blocks.
//! It is agnostic as to the specific manner in which tracks are represented.
//! Creating and formatting disks is left to specific implementations.
//! An important design element is that a disk image can refuse a request as out of scope.
//! As an example, PO images will only handle ProDOS blocks, since the original disk
//! geometry cannot be known (and may not even exist, although in other environments
//! it is appropriate to guess one).
//! 
//! ## Relation to File Systems
//! 
//! The `DiskImage` trait object serves as the underlying storage for `fs` modules.
//! The `fs` modules work by reading blocks from, or writing blocks to, the disk image.
//! The task of mapping blocks to sectors happens in submodules of `img`, sometimes with
//! the aid of `bios`, but never with any help from `fs`.
//! The `fs` module will usually run heuristics on certain key blocks when a disk
//! image is first connected.  If these fail the disk image is refused.
//! 
//! ## Disk Kind Patterns
//! 
//! There is an enumeration called `DiskKind` that can be used to create a disk image.
//! The `names` submodule contains convenient constants that can be passed to creation functions.
//! You can also use this to make a `match` selection based on parameters of a given disk.
//! As an example, if you want to do something only with 3.5 inch disks, you would use a pattern like
//! `DiskKind::D35(_)`.  The embedded structure can be used to limit the pattern to specific elements
//! of a track layout.
//! 
//! ## Sector Addresses
//! 
//! Assuming soft-sectoring, once we have a track, the only way to locate a sector is by matching its
//! address field.  The unique part of an address field is often a sector id that is just an unsigned
//! 8-bit integer, with other bytes encoding cylinder numbers, volume numbers, etc..  The address that
//! is finally used in a sector search is the product of multiple transformations.  If there is a block
//! request, the standard file system transformation is applied first.  This may involve a skew
//! transformation.  Then if there is a special format, a further transformation is applied.  Finally
//! the actual pattern is matched against the track bits.  In the case of naive sector images,
//! special format transformations can become ineffective.
//! 
//! ## Sector Skews
//! 
//! There are two kinds of sector skews.  The first kind of skew is a physical skew,
//! wherein geometric neighbors have disjoint addresses.  The second kind is a logical skew,
//! wherein geometric neighbors have neighboring addresses, but neighboring blocks
//! are mapped to disjoint sectors. Tables describing these orderings are in `bios::skew`.
//! These tables are used by `img` in a variety of ways due to the variety of ways that
//! disk images organize sectors.

pub mod dsk_d13;
pub mod dsk_do;
pub mod dsk_po;
pub mod dsk_img;
pub mod dot2mg;
pub mod nib;
pub mod woz;
pub mod woz1;
pub mod woz2;
pub mod imd;
pub mod td0;
pub mod names;
pub mod meta;
pub mod tracks;

use std::str::FromStr;
use std::fmt;
use crate::bios::Block;
use crate::{STDRESULT,DYNERR};
use tracks::DiskFormat;

use a2kit_macro::DiskStructError;

/// Enumerates disk image errors.  The `Display` trait will print equivalent long message.
#[derive(thiserror::Error,Debug)]
pub enum Error {
    #[error("unknown kind of disk")]
    UnknownDiskKind,
    #[error("unknown image type")]
    UnknownImageType,
    #[error("unknown format")]
    UnknownFormat,
    #[error("invalid kind of disk")]
    DiskKindMismatch,
    #[error("geometric coordinate out of range")]
    GeometryMismatch,
	#[error("image size did not match the request")]
	ImageSizeMismatch,
    #[error("image type not compatible with request")]
    ImageTypeMismatch,
    #[error("error while accessing internal structures")]
    InternalStructureAccess,
    #[error("unable to access sector")]
    SectorAccess,
    #[error("sector not found")]
    SectorNotFound,
    #[error("unable to access track")]
    TrackAccess,
    #[error("track request out of range")]
    TrackNotFound,
    #[error("track is unformatted")]
    BlankTrack,
    #[error("metadata mismatch")]
    MetadataMismatch,
    #[error("wrong context for this request")]
    BadContext,
    #[error("invalid byte while decoding")]
    InvalidByte,
    #[error("bad checksum found in a sector")]
    BadChecksum,
    #[error("could not find bit pattern")]
    BitPatternNotFound,
    #[error("nibble type appeared in wrong context")]
    NibbleType,
    #[error("track lies outside expected zones")]
    UnexpectedZone
}

/// Encapsulates 3 ways a track might be idenfified
#[derive(Clone,Copy,PartialEq)]
pub enum Track {
    /// single index to a track, often `C * num_heads + H`
    Num(usize),
    /// cylinder and head
    CH((usize, usize)),
    /// stepper motor position and head, needed for, e.g., WOZ quarter tracks
    Motor((usize, usize)),
}

/// Properties of a sector's neighborhood that may be used in forming its address.
/// Format objects determine how these values map to a specific address.
pub struct SectorHood {
    vol: u8,
    cyl: u8,
    head: u8,
    aux: u8
}

/// Wraps either a standard sector index or an explicit address.
/// If the index is used, it will in general be combined with `ZoneFormat` and `SectorHood` to produce
/// the actual sector address.  If the explicit address is used, there are no transformations.
#[derive(Clone,PartialEq)]
pub enum Sector {
    /// standard index used by a file system, subject to various transformations
    Num(usize),
    /// (index,address), the address is the explicit sector address to seek without transformation,
    /// the index may be used to determine other sector properties in the usual way 
    Addr((usize,Vec<u8>))
}

/// Indicates the overall scheme of a track
#[derive(PartialEq,Eq,Clone,Copy)]
pub enum FluxCode {
    None,
    FM,
    GCR,
    MFM
}

/// Indicates the encoding of a disk field, this is
/// only necessary for GCR tracks (evidently), for
/// others set to None.
#[derive(PartialEq,Eq,Clone,Copy)]
pub enum FieldCode {
    None,
    WOZ((usize,usize)),
    G64((usize,usize)),
    IBM((usize,usize))
}

#[derive(PartialEq,Eq,Clone,Copy)]
pub struct BlockLayout {
    block_size: usize,
    block_count: usize
}

/// Detailed layout of a single track, this will normally be deduced from actual track data
/// in response to a caller's request for a track solution.
pub struct SolvedTrack {
    flux_code: FluxCode,
    addr_code: FieldCode,
    data_code: FieldCode,
    /// nominal rate of pulses during a run of high bits, n.b. this is not the same as the data rate, e.g. for FM clock pulses are counted
    speed_kbps: usize,
    /// Measures the relative angular density of data, e.g., for a disk spinning at the nominal rate the maximum pulse rate
    /// is `speed_kbps * density`.  Should only be Some if the track supplies timing information.
    density: Option<f64>,
    /// string describing address (like VTS or CHSF)
    addr_type: String,
    /// mask out bits we are ignorant of due to image limitations
    addr_mask: [u8;6],
    /// address of every sector
    addr_map: Vec<[u8;6]>,
    size_map: Vec<usize>
}

/// We can have a track known to be blank, a track that seems to have data but
/// could not be solved, or a full track solution.  If there is a solution
/// the discriminant wraps a `SolvedTrack`.
pub enum TrackSolution {
    Blank,
    Unsolved,
    Solved(SolvedTrack)
}

/// Fixed size representation of how all the tracks on a disk are layed out,
/// useful for pattern matching.  The simplifying assumptions are
/// * at most 5 zones on the disk
/// * every track in a zone is laid out the same
/// * every sector on a track is laid out the same
#[derive(PartialEq,Eq,Clone,Copy)]
pub struct TrackLayout {
    cylinders: [usize;5],
    sides: [usize;5],
    sectors: [usize;5],
    sector_size: [usize;5],
    flux_code: [FluxCode;5],
    addr_code: [FieldCode;5],
    data_code: [FieldCode;5],
    speed_kbps: [usize;5]
}

/// This enumeration is often used in a match arm to take different
/// actions depending on the kind of disk.  It is in the form
/// package(layout), where the layout is a fixed size representation
/// of the track details that can be efficiently pattern matched.
#[derive(PartialEq,Eq,Clone,Copy)]
pub enum DiskKind {
    Unknown,
    LogicalBlocks(BlockLayout),
    LogicalSectors(TrackLayout),
    D3(TrackLayout),
    D35(TrackLayout),
    D525(TrackLayout),
    D8(TrackLayout)
}

#[derive(PartialEq,Clone,Copy)]
pub enum DiskImageType {
    D13,
    DO,
    PO,
    IMG,
    WOZ1,
    WOZ2,
    IMD,
    DOT2MG,
    NIB,
    TD0,
    /// for future expansion
    DOT86F,
    /// for future expansion
    D64,
    /// for future expansion
    G64,
    /// for future expansion
    MFI,
    /// for future expansion
    MFM,
    /// for future expansion
    HFE,
}

impl TrackLayout {
    pub fn track_count(&self) -> usize {
        let mut ans = 0;
        for i in 0..5 {
            ans += self.cylinders[i] * self.sides[i];
        }
        ans
    }
    pub fn sides(&self) -> usize {
        *self.sides.iter().max().unwrap()
    }
    pub fn zones(&self) -> usize {
        for i in 0..5 {
            if self.cylinders[i]==0 {
                return i;
            }
        }
        5
    }
    pub fn zone(&self,track_num: usize) -> usize {
        let mut tcount: [usize;5] = [0;5];
        tcount[0] = self.cylinders[0] * self.sides[0];
        for i in 1..5 {
            tcount[i] = tcount[i-1] + self.cylinders[i] * self.sides[i];
        }
        match track_num {
            n if n < tcount[0] => 0,
            n if n < tcount[1] => 1,
            n if n < tcount[2] => 2,
            n if n < tcount[3] => 3,
            _ => 4
        }
    }
    pub fn byte_capacity(&self) -> usize {
        let mut ans = 0;
        for i in 0..5 {
            ans += self.cylinders[i] * self.sides[i] * self.sectors[i] * self.sector_size[i];
        }
        ans
    }
    // fn sector_bytes(&self,track: usize) -> usize {
    //     let zone = self.zone(track);
    //     self.sector_size[zone]
    // }
}

/// Allows the track layout to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for TrackLayout {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cyls = 0;
        for c in self.cylinders {
            cyls += c;
        }
        write!(f,"{}/{}/{}/{}",cyls,self.sides(),self.sectors[0],self.sector_size[0])
    }
}

/// Allows the disk kind to be displayed to the console using `println!`.  This also
/// derives `to_string`, so the enum can be converted to `String`.
impl fmt::Display for DiskKind {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DiskKind::LogicalBlocks(lay) => write!(f,"Logical disk, {} blocks",lay.block_count),
            DiskKind::LogicalSectors(lay) => write!(f,"Logical disk, {}",lay),
            names::A2_400_KIND => write!(f,"Apple 3.5 inch 400K"),
            names::A2_800_KIND => write!(f,"Apple 3.5 inch 800K"),
            names::A2_DOS32_KIND => write!(f,"Apple 5.25 inch 13 sector"),
            names::A2_DOS33_KIND => write!(f,"Apple 5.25 inch 16 sector"),
            DiskKind::D3(lay) => write!(f,"3.0 inch {}",lay),
            DiskKind::D35(lay) => write!(f,"3.5 inch {}",lay),
            DiskKind::D525(lay) => write!(f,"5.25 inch {}",lay),
            DiskKind::D8(lay) => write!(f,"8 inch {}",lay),
            DiskKind::Unknown => write!(f,"unknown")
        }
    }
}

impl fmt::Display for FieldCode {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            FieldCode::WOZ((x,y)) => write!(f,"{}&{}",x,y),
            FieldCode::G64((x,y)) => write!(f,"G64-{}:{}",x,y),
            FieldCode::IBM((x,y)) => write!(f,"IBM-{}:{}",x,y),
            FieldCode::None => write!(f,"none")
        }
    }
}

impl FromStr for FieldCode {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "4&4" => Ok(FieldCode::WOZ((4,4))),
            "5&3" => Ok(FieldCode::WOZ((5,3))),
            "6&2" => Ok(FieldCode::WOZ((6,2))),
            "G64-5:4" => Ok(FieldCode::G64((5,4))),
            "IBM-5:4" => Ok(FieldCode::IBM((5,4))),
            "none" => Ok(FieldCode::None),
            _ => Err(Error::MetadataMismatch)
        }
    }
}

impl FromStr for FluxCode {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "FM" => Ok(FluxCode::FM),
            "MFM" => Ok(FluxCode::MFM),
            "GCR" => Ok(FluxCode::GCR),
            "none" => Ok(FluxCode::None),
            _ => Err(Error::MetadataMismatch)
        }
    }
}

impl fmt::Display for FluxCode {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            FluxCode::FM => write!(f,"FM"),
            FluxCode::MFM => write!(f,"MFM"),
            FluxCode::GCR => write!(f,"GCR"),
            FluxCode::None => write!(f,"none")
        }
    }
}

/// match command line argument to disk kind
impl FromStr for DiskKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "8in-ibm-sssd" => Ok(names::IBM_CPM1_KIND),
            "8in-trs80-ssdd" => Ok(names::TRS80_M2_CPM_KIND),
            "8in-nabu-dsdd" => Ok(names::NABU_CPM_KIND),
            "5.25in-ibm-ssdd8" => Ok(Self::D525(names::IBM_SSDD_8)),
            "5.25in-ibm-ssdd9" => Ok(Self::D525(names::IBM_SSDD_9)),
            "5.25in-ibm-dsdd8" => Ok(Self::D525(names::IBM_DSDD_8)),
            "5.25in-ibm-dsdd9" => Ok(Self::D525(names::IBM_DSDD_9)),
            "5.25in-ibm-ssqd" => Ok(Self::D525(names::IBM_SSQD)),
            "5.25in-ibm-dsqd" => Ok(Self::D525(names::IBM_DSQD)),
            "5.25in-ibm-dshd" => Ok(Self::D525(names::IBM_DSHD)),
            "5.25in-osb-sssd" => Ok(names::OSBORNE1_SD_KIND),
            "5.25in-osb-ssdd" => Ok(names::OSBORNE1_DD_KIND),
            "5.25in-kay-ssdd" => Ok(names::KAYPROII_KIND),
            "5.25in-kay-dsdd" => Ok(names::KAYPRO4_KIND),
            "5.25in-apple-13" => Ok(names::A2_DOS32_KIND),
            "5.25in-apple-16" => Ok(names::A2_DOS33_KIND),
            "3.5in-apple-400" => Ok(names::A2_400_KIND),
            "3.5in-apple-800" => Ok(names::A2_800_KIND),
            "3.5in-ibm-720" => Ok(Self::D35(names::IBM_720)),
            "3.5in-ibm-1440" => Ok(Self::D35(names::IBM_1440)),
            "3.5in-ibm-2880" => Ok(Self::D35(names::IBM_2880)),
            "3in-amstrad-ssdd" => Ok(names::AMSTRAD_SS_KIND),
            "hdmax" => Ok(names::A2_HD_MAX),
            _ => Err(Error::UnknownDiskKind)
        }
    }
}

impl FromStr for DiskImageType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self,Self::Err> {
        match s {
            "d13" => Ok(Self::D13),
            "do" => Ok(Self::DO),
            "po" => Ok(Self::PO),
            "img" => Ok(Self::IMG),
            "woz1" => Ok(Self::WOZ1),
            "woz2" => Ok(Self::WOZ2),
            "imd" => Ok(Self::IMD),
            "2mg" => Ok(Self::DOT2MG),
            "2img" => Ok(Self::DOT2MG),
            "nib" => Ok(Self::NIB),
            "td0" => Ok(Self::TD0),
            _ => Err(Error::UnknownImageType)
        }
    }
}

impl fmt::Display for DiskImageType {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::D13 => write!(f,"d13"),
            Self::DO => write!(f,"do"),
            Self::PO => write!(f,"po"),
            Self::IMG => write!(f,"img"),
            Self::WOZ1 => write!(f,"woz1"),
            Self::WOZ2 => write!(f,"woz2"),
            Self::IMD => write!(f,"imd"),
            Self::DOT2MG => write!(f,"2mg"),
            Self::NIB => write!(f,"nib"),
            Self::TD0 => write!(f,"td0"),
            Self::D64 => write!(f,"d64"),
            Self::DOT86F => write!(f,"86f"),
            Self::G64 => write!(f,"g64"),
            Self::HFE => write!(f,"hfe"),
            Self::MFM => write!(f,"mfm"),
            Self::MFI => write!(f,"mfi")
        }
    }
}

/// The main trait for working with any kind of disk image.
/// The corresponding trait object serves as storage for `DiskFS`.
/// Reading can mutate the object because the image may be keeping
/// track of the head position or other status indicators.
pub trait DiskImage {
    /// Get the count of formatted tracks, not necessarily the same as `end_track`
    fn track_count(&self) -> usize;
    /// Get the id of the end-track (last-track + 1)
    fn end_track(&self) -> usize;
    fn num_heads(&self) -> usize;
    fn motor_steps_per_cyl(&self) ->usize {
        1
    }
    /// Get the geometric [cyl,head].  Default truncates fractional tracks in a reasonable
    /// way if there are either 1 or 4 steps per track.
    fn get_rz(&self,trk: Track) -> Result<[usize;2],DYNERR> {
        let msc = self.motor_steps_per_cyl();
        let ans = match trk {
            Track::Num(t) => [t/self.num_heads(),t%self.num_heads()],
            Track::CH((c,h)) => [c,h],
            Track::Motor((m,h)) => [(m+msc/4)/msc,h]
        };
        Ok(ans)
    }
    /// Get the geometric track.  Default truncates fractional tracks in a reasonable
    /// way if there are either 1 or 4 steps per track.
    fn get_track(&self,trk: Track) -> Result<usize,DYNERR> {
        let msc = self.motor_steps_per_cyl();
        let ans = match trk {
            Track::Num(t) => t,
            Track::CH((c,h)) => c*self.num_heads() + h,
            Track::Motor((m,h)) => ((m+msc/4)/msc)*self.num_heads() + h
        };
        Ok(ans)
    }
    /// Get the geometric [cyl,head,sec].
    /// Default truncates fractional tracks in a reasonable way if there are either 1 or 4 steps per track.
    /// If an explicit address is given, the sector will be taken from the most likely address byte.
    fn get_rzq(&self,trk: Track,sec: Sector) -> Result<[usize;3],DYNERR> {
        let [c,h] = self.get_rz(trk)?;
        let s = match sec {
            Sector::Num(s) => s,
            Sector::Addr((_,addr)) => {
                match self.kind() {
                    names::A2_400_KIND | names::A2_800_KIND => addr[1] as usize,
                    _ => addr[2] as usize
                }
            }
        };
        Ok([c,h,s])
    }
    /// Get the capacity in bytes supposing this disk were formatted in a standard way.
    /// May return `None` if format hints are insufficient.
    fn nominal_capacity(&self) -> Option<usize>;
    /// Get the capacity in bytes given the way the disk is actually formatted.
    /// The expense can be high, and may change the disk state.
    fn actual_capacity(&mut self) -> Result<usize,DYNERR>;
    fn what_am_i(&self) -> DiskImageType;
    fn file_extensions(&self) -> Vec<String>;
    fn kind(&self) -> DiskKind;
    /// Change the kind of disk, but do not change the format
    fn change_kind(&mut self,kind: DiskKind);
    /// Change details of how sectors are identified and decoded
    fn change_format(&mut self,_fmt: DiskFormat) -> STDRESULT {
        Err(Box::new(Error::ImageTypeMismatch))
    }
    /// Change the broad method by which nibbles are extracted from a track.
    /// `Emulate` will try to produce nibbles just as the hardware would.
    /// `Fast` and `Analyze` will show something more idealized.
    fn change_method(&mut self,_method: tracks::Method) {
    }
    fn from_bytes(buf: &[u8]) -> Result<Self,DiskStructError> where Self: Sized;
    fn to_bytes(&mut self) -> Vec<u8>;
    /// Read a block from the image; can affect disk state
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR>;
    /// Write a block to the image
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT;
    /// Read a physical sector from the image; can affect disk state.
    fn read_sector(&mut self,trk: Track,sec: Sector) -> Result<Vec<u8>,DYNERR>;
    /// Write a physical sector to the image
    fn write_sector(&mut self,trk: Track,sec: Sector,dat: &[u8]) -> STDRESULT;
    /// Get the track buffer exactly in the form the image stores it
    fn get_track_buf(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR>;
    /// Set the track buffer using another track buffer, the sizes must match
    fn set_track_buf(&mut self,trk: Track,dat: &[u8]) -> STDRESULT;
    /// Determined sector layout and update internal formatting hints.
    /// Implement this at a low level, making as few assumptions as possible.
    /// The expense of this operation can vary widely depending on the image type.
    fn get_track_solution(&mut self,trk: Track) -> Result<TrackSolution,DYNERR>;
    /// Get the track bytes as aligned nibbles; for user inspection
    fn get_track_nibbles(&mut self,trk: Track) -> Result<Vec<u8>,DYNERR>;
    /// Write the track to a string suitable for display, input should be pre-aligned nibbles, e.g. from `get_track_nibbles`.
    /// Any required details of the track format have to come from the internal state of the image.
    fn display_track(&self,bytes: &[u8]) -> String;
    /// Get image metadata into JSON string.
    /// Default contains only the image type.
    fn get_metadata(&self,indent: Option<u16>) -> String {
        let mut root = json::JsonValue::new_object();
        let typ = self.what_am_i().to_string();
        root[typ] = json::JsonValue::new_object();
        if let Some(spaces) = indent {
            json::stringify_pretty(root,spaces)
        } else {
            json::stringify(root)
        }
    }
    /// Add or change a single metadata item.  This is designed to take as its arguments the
    /// outputs produced by walking a JSON tree with `crate::JsonCursor`.
    /// The `key_path` has the keys leading up to the leaf, e.g., `/type/item/_raw`, and
    /// the `val` is the JSON value associated with the leaf (anything but an object).
    /// The special keys `_raw` and `_pretty` should be handled as follows.
    /// If a leaf is neither `_raw` nor `_pretty` treat it as raw.
    /// If a leaf is `_pretty` ignore it.
    fn put_metadata(&mut self,key_path: &Vec<String>, _val: &json::JsonValue) -> STDRESULT {
        meta::test_metadata(key_path,self.what_am_i())
    }
    /// Write the disk geometry, including all track solutions, into a JSON string
    fn export_geometry(&mut self,indent: Option<u16>) -> Result<String,DYNERR> {
        let pkg = package_string(&self.kind());
        let mut track_sols = Vec::new();
        for trk in 0..self.end_track() {
            log::trace!("solve track {}",trk);
            let sol = self.get_track_solution(Track::Num(trk))?;
            let [c,h] = self.get_rz(Track::Num(trk))?;
            track_sols.push((c as f64,h,sol));
        }
        geometry_json(pkg,track_sols,self.end_track(),self.num_heads(),self.motor_steps_per_cyl(),indent)
    }
    /// Write the abstract disk format into a JSON string
    fn export_format(&self,_indent: Option<u16>) -> Result<String,DYNERR> {
        Err(Box::new(Error::UnknownFormat))
    }
}

fn solved_track_json(sol: SolvedTrack) -> Result<json::JsonValue,DYNERR> {
    let mut ans = json::JsonValue::new_object();
    ans["flux_code"] = match sol.flux_code {
        FluxCode::None => json::JsonValue::Null,
        f => json::JsonValue::String(f.to_string())
    };
    ans["addr_code"] = match sol.addr_code {
        FieldCode::None => json::JsonValue::Null,
        n => json::JsonValue::String(n.to_string())
    };
    ans["nibble_code"] = match sol.data_code {
        FieldCode::None => json::JsonValue::Null,
        n => json::JsonValue::String(n.to_string())
    };
    ans["speed_kbps"] = json::JsonValue::Number(sol.speed_kbps.into());
    ans["density"] = match sol.density {
        Some(val) => json::JsonValue::Number(val.into()),
        None => json::JsonValue::Null
    };
    ans["addr_map"] = json::JsonValue::new_array();
    for addr in sol.addr_map {
        ans["addr_map"].push(json::JsonValue::String(hex::encode_upper(&addr[0..sol.addr_type.len()])))?;
    }
    ans["size_map"] = json::JsonValue::new_array();
    for size in sol.size_map {
        ans["size_map"].push(size)?;
    }
    ans["addr_type"] = json::JsonValue::String(sol.addr_type);
    ans["addr_mask"] = json::JsonValue::new_array();
    for by in sol.addr_mask {
        ans["addr_mask"].push(by)?;
    }
    Ok(ans)
}

/// Create geometry string for external consumption, `cylinders` should be the nominal cylinder count for this
/// kind of disk, the actuals will be computed and provided automatically.
fn geometry_json(pkg: String,desc: Vec<(f64,usize,TrackSolution)>,cylinders: usize,heads: usize,width: usize,indent: Option<u16>) -> Result<String,DYNERR> {
    let mut root = json::JsonValue::new_object();
    root["package"] = json::JsonValue::String(pkg);
    let mut trk_ary = json::JsonValue::new_array();
    let mut blank_track_count = 0;
    let mut solved_track_count = 0;
    let mut unsolved_track_count = 0;
    let mut last_blank_track: Option<usize> = None;
    let mut last_solved_track: Option<usize> = None;
    let mut last_unsolved_track: Option<usize> = None;
    let mut idx = 0;
    for (fcyl,head,sol) in desc {
        let mut trk_obj = json::JsonValue::new_object();
        trk_obj["cylinder"] = json::JsonValue::Number(fcyl.into());
        trk_obj["head"] = json::JsonValue::Number(head.into());
        let ignore = match sol {
            TrackSolution::Blank => {
                if fcyl < cylinders as f64 {
                    blank_track_count += 1;
                    last_blank_track = Some(idx);
                }
                fcyl >= cylinders as f64
            },
            TrackSolution::Unsolved => {
                unsolved_track_count += 1;
                last_unsolved_track = Some(idx);
                false
            },
            TrackSolution::Solved(_) => {
                solved_track_count += 1;
                last_solved_track = Some(idx);
                false
            }
        };
        trk_obj["solution"] = match sol {
            TrackSolution::Blank => json::JsonValue::String("blank".to_string()),
            TrackSolution::Unsolved => json::JsonValue::String("unsolved".to_string()),
            TrackSolution::Solved(sol) => solved_track_json(sol)?
        };
        if !ignore {
            trk_ary.push(trk_obj)?;
        }
        idx += 1;
    }

    root["summary"] = json::JsonValue::new_object();
    root["summary"]["cylinders"] = json::JsonValue::Number(cylinders.into());
    root["summary"]["heads"] = json::JsonValue::Number(heads.into());
    root["summary"]["blank_tracks"] = json::JsonValue::Number(blank_track_count.into());
    root["summary"]["solved_tracks"] = json::JsonValue::Number(solved_track_count.into());
    root["summary"]["unsolved_tracks"] = json::JsonValue::Number(unsolved_track_count.into());
    root["summary"]["last_blank_track"] = match last_blank_track {
        Some(t) => json::JsonValue::Number(t.into()),
        None => json::JsonValue::Null
    };
    root["summary"]["last_solved_track"] = match last_solved_track {
        Some(t) => json::JsonValue::Number(t.into()),
        None => json::JsonValue::Null
    };
    root["summary"]["last_unsolved_track"] = match last_unsolved_track {
        Some(t) => json::JsonValue::Number(t.into()),
        None => json::JsonValue::Null
    };
    root["summary"]["steps_per_cyl"] = json::JsonValue::Number(width.into());

    if trk_ary.len()==0 {
        root["tracks"] = json::JsonValue::Null;
    } else {
        root["tracks"] = trk_ary;
    }
    if let Some(spaces) = indent {
        Ok(json::stringify_pretty(root,spaces))
    } else {
        Ok(json::stringify(root))
    }
}

/// Test a buffer for a size match to DOS-oriented track and sector counts.
pub fn is_dos_size(dsk: &Vec<u8>,allowed_track_counts: &Vec<usize>,sectors: usize) -> STDRESULT {
    let bytes = dsk.len();
    for tracks in allowed_track_counts {
        if bytes==tracks*sectors*256 {
            return Ok(());
        }
    }
    log::info!("image size was {}",bytes);
    return Err(Box::new(Error::ImageSizeMismatch));
}

/// If a data source is smaller than `quantum` bytes, pad it with zeros.
/// If it is larger, do not include the extra bytes.
pub fn quantize_block(src: &[u8],quantum: usize) -> Vec<u8> {
	let mut padded: Vec<u8> = Vec::new();
	for i in 0..quantum {
		if i<src.len() {
			padded.push(src[i])
		} else {
			padded.push(0);
		}
	}
    return padded;
}

/// Package designation for geometry JSON (e.g., "3.5", "5.25", ...)
pub fn package_string(kind: &DiskKind) -> String {
    match kind {
        DiskKind::D3(_) => "3".to_string(),
        DiskKind::D35(_) => "3.5".to_string(),
        DiskKind::D525(_) => "5.25".to_string(),
        DiskKind::D8(_) => "8".to_string(),
        DiskKind::LogicalBlocks(_) => "logical".to_string(),
        DiskKind::LogicalSectors(_) => "logical".to_string(),
        DiskKind::Unknown => "unknown".to_string()
    }
}

fn highest_bit(mut val: usize) -> u8 {
    let mut ans = 0;
    while val > 0 {
        ans += 1;
        val = val >> 1;
    }
    ans
}

/// Calculate the IBM CRC bytes given the sector address and optionally custom sync bytes and IDAM.
/// The full address including CRC is returned.
pub fn append_ibm_crc(addr: [u8;4],maybe_sync: Option<[u8;4]>) -> [u8;6]
{
    let mut buf = vec![];
    match maybe_sync {
        Some(sync) => buf.append(&mut sync.to_vec()),
        None => buf.append(&mut vec![0xa1,0xa1,0xa1,0xfe])
    };
    buf.append(&mut addr.to_vec());
    let buf = [[0xa1,0xa1,0xa1,0xfe],[addr[0],addr[1],addr[2],addr[3]]].concat();
    let mut crc: u16 = 0xffff;
    for i in 0..buf.len() {
        crc ^= (buf[i] as u16) << 8;
        for _bit in 0..8 {
            crc = (crc << 1) ^ match crc & 0x8000 { 0 => 0, _ => 0x1021 };
        }
    }
    let be = u16::to_be_bytes(crc);
    [addr[0],addr[1],addr[2],addr[3],be[0],be[1]]
}
