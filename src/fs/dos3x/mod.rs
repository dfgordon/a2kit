//! ## DOS 3.x file system module
//! This manipulates disk images containing one standard bootable
//! or non-bootable DOS 3.x volume.  At the level of this module,
//! wide latitude is allowed for track counts, while sector counts
//! are restricted to 13, 16, or 32.
//! 
//! * Analogues of BASIC commands like SAVE, BSAVE etc. are exposed through the `DiskFS` trait
//! * The module will try to emulate the order in which DOS would access sectors

pub mod types;
mod boot;
mod directory;

use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use num_traits::FromPrimitive;
use a2kit_macro::DiskStruct;
use log::{info,debug,error};

use types::*;
use directory::*;
use super::{Block,TextEncoder};
use crate::img;
use crate::commands::ItemType;

use crate::lang::applesoft;
use crate::{STDRESULT,DYNERR};

/// This will accept lower case; case will be automatically converted as appropriate
fn is_name_valid(s: &str) -> bool {
    for char in s.chars() {
        if !char.is_ascii() {
            debug!("non-ascii file name character `{}` (codepoint {})",char,char as u32);
            info!("use hex escapes to introduce arbitrary bytes");
            return false;
        }
    }
    if s.len()>30 {
        info!("file name too long, max 30");
        return false;
    }
    true
}

fn file_name_to_string(fname: [u8;30]) -> String {
    // fname is negative ASCII padded to the end with spaces
    // non-ASCII will go as hex escapes
    return String::from(crate::escaped_ascii_from_bytes(&fname.to_vec(),true,true).trim_end());
}

fn string_to_file_name(s: &str) -> [u8;30] {
    if s.len()> 30 {
        panic!("DOS filename was loo long");
    }
    let mut ans: [u8;30] = [0xa0;30]; // fill with negative spaces
    let unescaped = crate::escaped_ascii_to_bytes(s, true);
    for i in 0..30 {
        if i<unescaped.len() {
            ans[i] = unescaped[i];
        }
    }
    return ans;
}


/// The primary interface for disk operations.
pub struct Disk
{
    // VTOC works for any DOS 3.x
    maybe_vtoc: Option<VTOC>,
    img: Box<dyn img::DiskImage>
}

impl Disk
{
    fn new_fimg(chunk_len: usize) -> super::FileImage {
        super::FileImage {
            fimg_version: super::FileImage::fimg_version(),
            file_system: String::from("a2 dos"),
            fs_type: vec![0],
            aux: vec![],
            eof: vec![],
            created: vec![],
            modified: vec![],
            access: vec![],
            version: vec![],
            min_version: vec![],
            chunk_len,
            chunks: HashMap::new()
        }
    }
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    pub fn from_img(img: Box<dyn img::DiskImage>) -> Self {
        Self {
            maybe_vtoc: None,
            img
        }
    }
    fn test_img_13(img: &mut Box<dyn img::DiskImage>) -> bool {
        if let Ok(dat) = img.read_block(Block::D13([17,0])) {
            let vtoc = VTOC::from_bytes(&dat);
            let (tlen,slen) = (35,13);
            if vtoc.version>2 {
                debug!("D13: VTOC wrong version {}",vtoc.version);
                return false;
            }
            if vtoc.vol<1 || vtoc.vol>254 {
                debug!("D13: Volume {} out of range",vtoc.vol);
                return false;
            }
            if vtoc.track1 != VTOC_TRACK || vtoc.sector1 != slen-1 {
                debug!("D13: VTOC wrong track1 {}, sector1 {}",vtoc.track1,vtoc.sector1);
                return false;
            }
            if vtoc.bytes != [0,1] || vtoc.sectors != slen as u8 || vtoc.tracks != tlen as u8 {
                debug!("D13: VTOC wrong bytes {:?}, sectors {}, tracks {}",vtoc.bytes,vtoc.sectors,vtoc.tracks);
                return false;
            }
            return true;
        }
        debug!("VTOC sector was not readable as D13");
        return false;
    }
    fn test_img_16(img: &mut Box<dyn img::DiskImage>) -> bool {
        if let Ok(dat) = img.read_block(Block::DO([17,0])) {
            let vtoc = VTOC::from_bytes(&dat);
            let (tlen,slen) = (35,16);
            if vtoc.version<3 {
                debug!("VTOC wrong version {}",vtoc.version);
                return false;
            }
            if vtoc.vol<1 || vtoc.vol>254 {
                debug!("Volume {} out of range",vtoc.vol);
                return false;
            }
            if vtoc.track1 != VTOC_TRACK || vtoc.sector1 != slen-1 {
                debug!("VTOC wrong track1 {}, sector1 {}",vtoc.track1,vtoc.sector1);
                return false;
            }
            if vtoc.bytes != [0,1] || vtoc.sectors != slen as u8 || vtoc.tracks != tlen as u8 {
                debug!("VTOC wrong bytes {:?}, sectors {}, tracks {}",vtoc.bytes,vtoc.sectors,vtoc.tracks);
                return false;
            }
            return true;
        }
        debug!("VTOC sector was not readable as DO");
        return false;
    }
    /// Test an image to see if it already contains DOS 3.x.
    pub fn test_img(img: &mut Box<dyn img::DiskImage>) -> bool {
        let tlen = img.track_count();
        if tlen!=35 {
            debug!("track count is unexpected");
            return false;
        }
        let old_kind = img.kind();
        img.change_kind(img::names::A2_DOS32_KIND);
        debug!("change to 13 sectors");
        if Self::test_img_13(img) {
            return true;
        }
        debug!("change to 16 sectors");
        img.change_kind(img::names::A2_DOS33_KIND);
        if Self::test_img_16(img) {
            return true;
        }
        img.change_kind(old_kind);
        return false;
    }
    /// Open VTOC buffer if not already present.  Will usually be called indirectly.
    fn open_vtoc_buffer(&mut self) -> STDRESULT {
        match &self.maybe_vtoc {
            Some(_) => Ok(()),
            None => {
                debug!("open VTOC buffer");
                // We can use physical addressing for either DOS if sector = 0
                let buf = self.img.read_sector(VTOC_TRACK as usize, 0, 0)?;
                self.maybe_vtoc = Some(VTOC::from_bytes(&buf));
                Ok(())
            }
        }
    }
    /// Get the buffered VTOC mutably, will open buffer if necessary.
    fn get_vtoc_mut(&mut self) -> Result<&mut VTOC,DYNERR> {
        self.open_vtoc_buffer()?;
        if let Some(vtoc) = self.maybe_vtoc.as_mut() {
            return Ok(vtoc);
        }
        panic!("VTOC buffer failed to open");
    }
    /// Get the buffered VTOC immutably, will open buffer if necessary.
    fn get_vtoc_ref(&mut self) -> Result<&VTOC,DYNERR> {
        self.open_vtoc_buffer()?;
        if let Some(vtoc) = self.maybe_vtoc.as_ref() {
            return Ok(vtoc);
        }
        panic!("VTOC buffer failed to open");
    }
    /// Gets constant fields of the VTOC as copies, will open buffer if necessary.
    fn get_vtoc_constants(&mut self) -> Result<VolumeConstants,DYNERR> {
        self.open_vtoc_buffer()?;
        if let Some(vtoc) = self.maybe_vtoc.as_ref() {
            return Ok(vtoc.get_constants());
        }
        panic!("VTOC buffer failed to open");
    }
    /// Buffer needs to be written back when an external caller
    /// asks for the underlying image.
    fn writeback_vtoc_buffer(&mut self) -> STDRESULT {
        let buf = match self.maybe_vtoc.as_ref() {
            Some(vtoc) => vtoc.to_bytes(),
            None => return Ok(())
        };
        debug!("writeback VTOC buffer");
        // We can use physical addressing for either DOS if sector = 0
        self.img.write_sector(VTOC_TRACK as usize, 0, 0, &buf)
    }
    fn addr(&self,ts: [u8;2]) -> Block {
        match self.img.kind() {
            img::names::A2_DOS32_KIND => Block::D13([ts[0] as usize,ts[1] as usize]),
            img::names::A2_DOS33_KIND => Block::DO([ts[0] as usize,ts[1] as usize]),
            _ => panic!("unexpected disk kind")
        }
    }
    fn verify_ts(vconst: &VolumeConstants,track: u8,sector: u8) -> STDRESULT {
        if track>=vconst.tracks || sector>=vconst.sectors {
            error!("track {} sector {} out of bounds",track,sector);
            return Err(Box::new(Error::Range));
        }
        Ok(())
    }
    fn get_track_map(vtoc: &VTOC,track: u8) -> u32 {
        let bm = &vtoc.bitmap;
        let i = (track*4) as usize;
        u32::from_be_bytes([bm[i],bm[i+1],bm[i+2],bm[i+3]])
    }
    fn save_track_map(vtoc: &mut VTOC,track: u8,map: u32) {
        let i = (track*4) as usize;
        let slice: [u8;4] = u32::to_be_bytes(map);
        vtoc.bitmap[i] = slice[0];
        vtoc.bitmap[i+1] = slice[1];
        vtoc.bitmap[i+2] = slice[2];
        vtoc.bitmap[i+3] = slice[3];
    }
    fn update_last_track(&mut self,track: u8) -> STDRESULT {
        let vtoc = self.get_vtoc_mut()?;
        // The last_direction and last_track fields are not discussed in DOS manual.
        // This way of setting them is a guess based on emulator outputs.
        // If/how they are used in the free sector search is yet another question.
        if track<VTOC_TRACK {
            vtoc.last_direction = 255;
            vtoc.last_track = track;
        }
        if track>VTOC_TRACK {
            vtoc.last_direction = 1;
            vtoc.last_track = track;
        }
        Ok(())
    }
    fn allocate_sector(&mut self,track: u8,sector: u8) -> STDRESULT {
        let vtoc = self.get_vtoc_mut()?;
        let mut map = Self::get_track_map(vtoc,track);
        let eff_sec: u32 = (sector + 32 - vtoc.sectors) as u32;
        map &= (1 << eff_sec) ^ u32::MAX;
        Ok(Self::save_track_map(vtoc,track,map))
    }
    fn deallocate_sector(&mut self,track: u8,sector: u8) -> STDRESULT {
        let vtoc = self.get_vtoc_mut()?;
        let mut map = Self::get_track_map(vtoc,track);
        let eff_sec: u32 = (sector + 32 - vtoc.sectors) as u32;
        map |= 1 << eff_sec;
        Ok(Self::save_track_map(vtoc,track,map))
    }
    fn is_sector_free(vtoc: &VTOC,track: u8,sector: u8) -> bool {
        let map = Self::get_track_map(vtoc,track);
        let eff_sec: u32 = (sector + 32 - vtoc.sectors) as u32;
        (map & (1 << eff_sec)) > 0
    }
    /// Read a sector of data into buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the sector, the partial sector is copied.
    /// If sector is VTOC get it from the VTOC buffer.
    fn read_sector(&mut self,data: &mut [u8],ts: [u8;2], offset: usize) -> STDRESULT {
        let vtoc = self.get_vtoc_ref()?;
        let bytes_per_sector = u16::from_le_bytes(vtoc.bytes) as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read sector"),
            x if x<=bytes_per_sector => x,
            _ => bytes_per_sector
        };
        let buf = match ts {
            // TODO: getting VTOC sector from buffer forces trailing bytes to read as 0;
            // High level callers can always read the physical sector from the image to avoid this.
            [VTOC_TRACK,0] => img::quantize_block(&vtoc.to_bytes(), 256),
            _ => self.img.read_block(self.addr(ts))?
        };
        for i in 0..actual_len as usize {
            data[offset + i] = buf[i];
        }
        Ok(())
    }
    /// Zap and allocate the sector in one step.
    /// If it is the VTOC panic; we should only be zapping VTOC.
    fn write_sector(&mut self,data: &[u8],ts: [u8;2], offset: usize) -> STDRESULT {
        if ts==[VTOC_TRACK,0] {
            panic!("attempt to write VTOC, zap it instead");
        }
        let vconst = self.get_vtoc_constants()?;
        let bytes_per_sector = u16::from_le_bytes(vconst.bytes);
        self.zap_sector(data,ts,offset,bytes_per_sector)?;
        self.allocate_sector(ts[0],ts[1])
    }
    /// Writes a sector of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the sector, trailing bytes are unaffected.
    fn zap_sector(&mut self,data: &[u8],ts: [u8;2], offset: usize, bytes_per_sector: u16) -> STDRESULT {
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write sector"),
            x if x<=bytes_per_sector as i32 => x as usize,
            _ => bytes_per_sector as usize
        };
        if ts==[VTOC_TRACK,0] {
            self.maybe_vtoc = None;
        }
        self.img.write_block(self.addr(ts), &data[offset..offset+actual_len].to_vec())
    }
    /// Create any DOS 3.x volume
    pub fn init(&mut self,vol:u8,bootable:bool,last_track_written:u8,tracks:u8,sectors:u8) -> STDRESULT {
        assert!(vol>0 && vol<255);
        assert!(tracks>VTOC_TRACK && tracks<=50);
        assert!(sectors==13 || sectors==16 || sectors==32);
        assert!(last_track_written>0 && last_track_written<tracks);
        
        // First write the Volume Table of Contents (VTOC)
        let mut vtoc = VTOC::new();
        vtoc.pad1 = match sectors {
            13 => 2,
            16 | 32 => 4,
            _ => panic!("unexpected sector count")
        };
        vtoc.vol = vol;
        vtoc.last_track = last_track_written;
        vtoc.last_direction = 1;
        vtoc.max_pairs = 0x7a;
        vtoc.track1 = VTOC_TRACK;
        vtoc.sector1 = sectors-1;
        vtoc.version = match sectors {
            13 => 2,
            16 | 32 => 3,
            _ => panic!("unexpected sector count")
        };
        vtoc.bytes = [0,1];
        vtoc.sectors = sectors;
        vtoc.tracks = tracks;
        // Mark as free except track 0
        let all_free: [u8;4] = match sectors {
            13 => u32::to_be_bytes(0xfff80000),
            16 => u32::to_be_bytes(0xffff0000),
            32 => u32::to_be_bytes(0xffffffff),
            _ => panic!("unexpected sector count")
        };
        for track in 1..tracks as usize {
            vtoc.bitmap[track*4+0] = all_free[0];
            vtoc.bitmap[track*4+1] = all_free[1];
            vtoc.bitmap[track*4+2] = all_free[2];
            vtoc.bitmap[track*4+3] = all_free[3];
        }
        // If bootable mark DOS tracks as entirely used
        if bootable {
            for i in 1*4..3*4 {
                vtoc.bitmap[i] = 0;
            }
        }
        // Mark track VTOC_TRACK as entirely used (VTOC and directory)
        for i in VTOC_TRACK*4..(VTOC_TRACK+1)*4 {
            vtoc.bitmap[i as usize] = 0;
        }
        // zap in the VTOC
        self.zap_sector(&vtoc.to_bytes(),[VTOC_TRACK,0],0,256)?;
        // Write the directory sectors
        let mut dir = DirectorySector::new();
        self.write_sector(&dir.to_bytes(),[VTOC_TRACK,1],0)?;
        for sec in 2 as u8..sectors as u8 {
            dir.next_track = VTOC_TRACK;
            dir.next_sector = sec - 1;
            self.write_sector(&dir.to_bytes(),[VTOC_TRACK,sec],0)?;
        }
        // If bootable write DOS tracks
        if bootable {
            let flat = match sectors {
                13 => boot::DOS32_TRACKS.to_vec(),
                16 => boot::DOS33_TRACKS.to_vec(),
                _ => panic!("only 13 or 16 sector disks can be made bootable")
            };
            for track in 0..3 {
                for sector in 0..sectors as usize {
                    let offset = track*sectors as usize*256 + sector*256;
                    self.write_sector(&flat,[track as u8,sector as u8],offset)?;
                }
            }
        }
        Ok(())
    }
    /// Create a standard DOS 3.2 volume (116K)
    pub fn init32(&mut self,vol:u8,bootable:bool) -> STDRESULT {
        self.init(vol,bootable,17,35,13)
    }
    /// Create a standard DOS 3.3 small volume (140K)
    pub fn init33(&mut self,vol:u8,bootable:bool) -> STDRESULT {
        self.init(vol,bootable,17,35,16)
    }
    fn num_free_sectors(&mut self) -> Result<usize,DYNERR> {
        let vtoc = self.get_vtoc_ref()?;
        let mut ans: usize = 0;
        for track in 0..vtoc.tracks {
            for sector in 0..vtoc.sectors {
                if Self::is_sector_free(vtoc, track, sector) {
                    ans += 1;
                }
            }
        }
        return Ok(ans);
    }
    fn get_next_free_sector(&mut self,prefer_jump: bool) -> Result<[u8;2],DYNERR> {
        // Search algorithm outlined in DOS manual seems inconsistent with actual results from emulators.
        // This algorithm is a guess at how DOS is doing it, based on emulator outputs.
        // Fortunately we don't have to emulate this exactly for the disk to work.
        let vtoc = self.get_vtoc_ref()?;
        let tvtoc: u8 = vtoc.track1;
        let tstart = match vtoc.last_track {
            x if x>=vtoc.tracks => tvtoc-1,
            x if x>tvtoc && prefer_jump => x+1,
            x if x<tvtoc && prefer_jump => x-1,
            x => x
        };
        let tend = vtoc.tracks;
        // build search order
        let search_tracks: Vec<u8>;
        if tstart<tvtoc {
            search_tracks = [
                (1..tstart+1).rev().collect::<Vec<u8>>(),
                (tvtoc+1..tend).collect(),
                (tstart+1..tvtoc).rev().collect()
            ].concat();
        } else {
            search_tracks = [
                (tstart..tend).collect::<Vec<u8>>(),
                (1..tvtoc).rev().collect(),
                (tvtoc+1..tstart).collect()
            ].concat();
        }
        // search
        for track in search_tracks {
            for sector in (0..vtoc.sectors).rev() {
                if Self::is_sector_free(vtoc,track,sector) {
                    return Ok([track,sector]);
                }
            }
        }
        Err(Box::new(Error::DiskFull))
    }
    /// Return a tuple with ([track,sector],entry index)
    fn get_next_directory_slot(&mut self) -> Result<([u8;2],u8),DYNERR> {
        let vconst = self.get_vtoc_constants()?;
        let mut ts = [vconst.track1,vconst.sector1];
        let mut buf = vec![0;256];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,ts[0], ts[1])?;
            self.read_sector(&mut buf, ts, 0)?;
            let dir = DirectorySector::from_bytes(&buf);
            for e in 0..7 {
                if dir.entries[e].tsl_track==0 || dir.entries[e].tsl_track==255 {
                    return Ok((ts,e as u8));
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return Err(Box::new(Error::DiskFull));
            }
        }
        error!("number of directory sectors is not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Scan the directory sectors to find the named file, possible returned tuple is (tslist ptr,file type)
    fn get_tslist_sector(&mut self,name: &str) -> Result<Option<([u8;2],u8)>,DYNERR> {
        let vconst = self.get_vtoc_constants()?;
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut ts = [vconst.track1,vconst.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,ts[0], ts[1])?;
            self.read_sector(&mut buf, ts, 0)?;
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    return Ok(Some(([entry.tsl_track,entry.tsl_sector],entry.file_type)));
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return Ok(None);
            }
        }
        error!("number of directory sectors is not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Read any file into the sparse file format.  Use `FileImage.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&mut self,name: &str) -> Result<super::FileImage,DYNERR> {
        let vconst = self.get_vtoc_constants()?;
        let (mut next_tslist,ftype) = match self.get_tslist_sector(name) {
            Ok(Some((ts,typ))) => (ts,typ),
            Ok(None) => return Err(Box::new(Error::FileNotFound)),
            Err(e) => return Err(e)
        };
        let mut ans = Disk::new_fimg(256);
        let mut buf = vec![0;256];
        let mut count: usize = 0;
        // loop up to a maximum, if it is reached return error
        for _try in 0..types::MAX_TSLIST_REPS {
            self.read_sector(&mut buf,next_tslist,0)?;
            let tslist = TrackSectorList::from_bytes(&buf);
            for p in 0..vconst.max_pairs as usize {
                let next = [tslist.pairs[p*2],tslist.pairs[p*2+1]];
                if next[0]>0 {
                    let mut full_buf: Vec<u8> = vec![0;256];
                    self.read_sector(&mut full_buf,next,0)?;
                    ans.chunks.insert(count,full_buf);
                }
                count += 1;
            }
            if tslist.next_track==0 {
                ans.fs_type = vec![ftype];
                return Ok(ans);
            }
            next_tslist = [tslist.next_track,tslist.next_sector];
        }
        error!("number of track-sector list sectors is not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    /// Write any sparse or sequential file.  Use `FileImage::desequence` to put sequential data
    /// into the sparse file format, with no loss of generality.
    /// Unlike DOS, nothing is written unless there is enough space for all the data.
    fn write_file(&mut self,name: &str, fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if !is_name_valid(name) {
            error!("invalid DOS filename");
            return Err(Box::new(Error::SyntaxError));
        }
        let vconst = self.get_vtoc_constants()?;
        if fimg.chunks.len()==0 {
            error!("empty data is not allowed for DOS 3.x file images");
            return Err(Box::new(Error::EndOfData));
        }
        match self.get_tslist_sector(name) {
            Ok(Some(_)) => {
                error!("overwriting is not allowed");
                return Err(Box::new(Error::WriteProtected))
            },
            Ok(None) => debug!("no existing file, OK to proceed"),
            Err(e) => return Err(e)
        };
        // this is a new file
        // unlike DOS, we do not write anything unless there is room
        assert!(fimg.chunks.len()>0);
        let data_sectors = fimg.chunks.len();
        let tslist_sectors = 1 + (fimg.end()-1)/vconst.max_pairs as usize;
        debug!("file needs {} data secs, {} tslist secs; {} available",data_sectors,tslist_sectors,self.num_free_sectors()?);
        if data_sectors + tslist_sectors > self.num_free_sectors()? {
            return Err(Box::new(Error::DiskFull));
        }

        // we are doing this
        let mut sec_base = 0; // in units of pairs
        let mut p = 0; // pairs written in current tslist sector
        let mut tslist = TrackSectorList::new();
        let mut tslist_ts = self.get_next_free_sector(true)?;
        self.allocate_sector(tslist_ts[0],tslist_ts[1])?; // reserve this sector
        self.update_last_track(tslist_ts[0])?;

        // write the directory entry
        let (ts,e) = self.get_next_directory_slot()?;
        let mut dir_buf = vec![0;256];
        self.read_sector(&mut dir_buf, ts, 0)?;
        let mut dir = DirectorySector::from_bytes(&dir_buf);
        dir.entries[e as usize].tsl_track = tslist_ts[0];
        dir.entries[e as usize].tsl_sector = tslist_ts[1];
        match fimg.fs_type.len() {
            0 => return Err(Box::new(Error::Range)),
            _ => dir.entries[e as usize].file_type = fimg.fs_type[0],
        } 
        dir.entries[e as usize].name = string_to_file_name(name);
        dir.entries[e as usize].sectors = [tslist_sectors as u8 + data_sectors as u8 ,0];
        self.write_sector(&dir.to_bytes(), ts, 0)?;

        // write the data and TS list as we go
        for s in 0..fimg.end() {
            if let Some(chunk) = fimg.chunks.get(&s) {
                let data_ts = self.get_next_free_sector(false)?;
                tslist.pairs[p*2] = data_ts[0];
                tslist.pairs[p*2+1] = data_ts[1];
                self.write_sector(&tslist.to_bytes(), tslist_ts, 0)?;
                self.write_sector(chunk,data_ts,0)?;
                self.update_last_track(data_ts[0])?;
            } else {
                tslist.pairs[p*2] = 0;
                tslist.pairs[p*2+1] = 0;
                self.write_sector(&tslist.to_bytes(), tslist_ts, 0)?;
            }
            p += 1;
            if p==vconst.max_pairs as usize  && s+1!=fimg.end() {
                // tslist spilled over to another sector
                let next_tslist_ts = self.get_next_free_sector(false)?;
                tslist.next_track = next_tslist_ts[0];
                tslist.next_sector = next_tslist_ts[1];
                self.write_sector(&tslist.to_bytes(),tslist_ts,0)?;
                self.update_last_track(tslist_ts[0])?;
                tslist_ts = next_tslist_ts;
                sec_base += vconst.max_pairs as usize;
                tslist = TrackSectorList::new();
                tslist.sector_base = u16::to_le_bytes(sec_base as u16);
                p = 0;
            }
        }
        
        return Ok(data_sectors + tslist_sectors);
    }
    /// Verify that the new name does not already exist
    fn ok_to_rename(&mut self,new_name: &str) -> STDRESULT {
        if !is_name_valid(&new_name) {
            error!("invalid DOS filename");
            return Err(Box::new(Error::SyntaxError));
        }
        match self.get_tslist_sector(new_name) {
            Ok(None) => Ok(()),
            Ok(_) => Err(Box::new(Error::FileLocked)),
            Err(e) => Err(e)
        }
    }
    /// modify a file entry, optionally lock, unlock, rename, retype; attempt to change already locked file will fail.
    fn modify(&mut self,name: &str,maybe_lock: Option<bool>,maybe_new_name: Option<&str>,maybe_ftype: Option<&str>) -> STDRESULT {
        if !is_name_valid(&name) {
            error!("old name is invalid, perhaps use hex escapes");
            return Err(Box::new(Error::SyntaxError));
        }
        let vconst = self.get_vtoc_constants()?;
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut dir_ts = [vconst.track1,vconst.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,dir_ts[0], dir_ts[1])?;
            self.read_sector(&mut buf, dir_ts, 0)?;
            let mut dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_mut() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    if entry.file_type > 127 && maybe_new_name!=None {
                        return Err(Box::new(Error::FileLocked));
                    }
                    entry.file_type = match maybe_lock {
                        Some(true) => entry.file_type | 0x80,
                        Some(false) => entry.file_type & 0x7f,
                        None => entry.file_type
                    };
                    if let Some(new_name) = maybe_new_name {
                        entry.name = string_to_file_name(new_name);
                    }
                    if let Some(ftype) = maybe_ftype {
                        match FileType::from_str(ftype) {
                            Ok(typ) => entry.file_type = typ as u8,
                            Err(e) => return Err(Box::new(e))
                        }
                    }
                    return self.write_sector(&dir.to_bytes(),dir_ts,0)
                }
            }
            dir_ts = [dir.next_track,dir.next_sector];
            if dir_ts == [0,0] {
                return Err(Box::new(Error::FileNotFound));
            }
        }
        error!("number of directory sectors is not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
}

impl super::DiskFS for Disk {
    fn new_fimg(&self,chunk_len: usize) -> super::FileImage {
        Disk::new_fimg(chunk_len)
    }
    fn catalog_to_stdout(&mut self, _path: &str) -> STDRESULT {
        let vconst = self.get_vtoc_constants()?;
        let typ_map: HashMap<u8,&str> = HashMap::from([(0," T"),(1," I"),(2," A"),(4," B"),(128,"*T"),(129,"*I"),(130,"*A"),(132,"*B")]);
        let mut ts = [vconst.track1,vconst.sector1];
        let mut buf = vec![0;256];
        println!();
        println!("DISK VOLUME {}",vconst.vol);
        println!();
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,ts[0], ts[1])?;
            self.read_sector(&mut buf, ts, 0)?;
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if entry.tsl_track>0 && entry.tsl_track<255 {
                    let name = file_name_to_string(entry.name);
                    let sectors = u16::from_le_bytes(entry.sectors);
                    if let Some(typ) = typ_map.get(&entry.file_type) {
                        println!("{} {:03} {}",typ,sectors,name);
                    } else {
                        println!("?? {:03} {}",sectors,name);
                    }
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                println!();
                return Ok(());
            }
        }
        error!("the disk image directory seems to be damaged");
        return Err(Box::new(Error::IOError));
    }
    fn tree(&mut self,include_meta: bool) -> Result<String,DYNERR> {
        let vconst = self.get_vtoc_constants()?;
        let mut ts = [vconst.track1,vconst.sector1];
        let mut buf = vec![0;256];
        let mut tree = json::JsonValue::new_object();
        tree["file_system"] = json::JsonValue::String("a2 dos".to_string());
        tree["files"] = json::JsonValue::new_object();
        tree["label"] = json::JsonValue::new_object();
        tree["label"]["name"] = json::JsonValue::String(vconst.vol.to_string());
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,ts[0], ts[1])?;
            self.read_sector(&mut buf, ts, 0)?;
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if entry.tsl_track>0 && entry.tsl_track<255 {
                    let name = file_name_to_string(entry.name);
                    tree["files"][&name] = json::JsonValue::new_object();
                    // file nodes must have no files object at all
                    if include_meta {
                        let sectors = u16::from_le_bytes(entry.sectors);
                        ts = [entry.tsl_track,entry.tsl_sector];
                        Self::verify_ts(&vconst,ts[0], ts[1])?;
                        self.read_sector(&mut buf,ts,0)?;
                        let tslist = TrackSectorList::from_bytes(&buf);
                        ts = [tslist.pairs[0],tslist.pairs[1]];
                        Self::verify_ts(&vconst,ts[0], ts[1])?;
                        self.read_sector(&mut buf,ts,0)?;
                        let bytes = match entry.file_type & 0x7f {
                            1 | 2 => u16::from_le_bytes([buf[0],buf[1]]),
                            4 => u16::from_le_bytes([buf[2],buf[3]]),
                            _ => sectors*256
                        };
                        tree["files"][&name]["meta"] = json::JsonValue::new_object();
                        let meta = &mut tree["files"][&name]["meta"];
                        meta["type"] = json::JsonValue::String(hex::encode_upper(vec![entry.file_type & 0x7f]));
                        meta["eof"] = json::JsonValue::Number(bytes.into());
                        meta["blocks"] = json::JsonValue::Number(sectors.into());
                        meta["read_only"] = json::JsonValue::Boolean(entry.file_type & 0x80 > 0);
                    }
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return Ok(json::stringify_pretty(tree, 4));
            }
        }
        error!("the disk image directory seems to be damaged");
        return Err(Box::new(Error::IOError));
    }
    fn create(&mut self,_path: &str) -> STDRESULT {
        error!("DOS 3.x does not support operation");
        return Err(Box::new(Error::SyntaxError));
    }
    fn delete(&mut self,name: &str) -> STDRESULT {
        let vconst = self.get_vtoc_constants()?;
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut dir_ts = [vconst.track1,vconst.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            Self::verify_ts(&vconst,dir_ts[0], dir_ts[1])?;
            self.read_sector(&mut buf, dir_ts, 0)?;
            let mut dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_mut() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    if entry.file_type > 127 {
                        return Err(Box::new(Error::WriteProtected));
                    }
                    let mut tslist_ts = [entry.tsl_track,entry.tsl_sector];
                    for _try2 in 0..types::MAX_TSLIST_REPS {
                        self.read_sector(&mut buf, tslist_ts, 0)?;
                        let tslist = TrackSectorList::from_bytes(&buf);
                        for p in 0..vconst.max_pairs as usize {
                            if tslist.pairs[p*2]>0 && tslist.pairs[p*2]<255 {
                                self.deallocate_sector(tslist.pairs[p*2], tslist.pairs[p*2+1])?;
                            }
                        }
                        self.deallocate_sector(tslist_ts[0], tslist_ts[1])?;
                        tslist_ts = [tslist.next_track,tslist.next_sector];
                        if tslist_ts==[0,0] {
                            entry.name[entry.name.len()-1] = entry.tsl_track;
                            entry.tsl_track = 255;
                            return self.write_sector(&dir.to_bytes(),dir_ts,0)
                        }
                    }
                    error!("number of track-sector list sectors is not plausible, aborting");
                    return Err(Box::new(Error::EndOfData));
                }
            }
            dir_ts = [dir.next_track,dir.next_sector];
            if dir_ts == [0,0] {
                return Err(Box::new(Error::FileNotFound));
            }
        }
        error!("number of directory sectors is not plausible, aborting");
        Err(Box::new(Error::EndOfData))
    }
    fn protect(&mut self,_path: &str,_password: &str,_read: bool,_write: bool,_delete: bool) -> STDRESULT {
        error!("DOS does not support operation");
        Err(Box::new(Error::SyntaxError))
    }
    fn unprotect(&mut self,_path: &str) -> STDRESULT {
        error!("DOS does not support operation");
        Err(Box::new(Error::SyntaxError))
    }
    fn lock(&mut self,name: &str) -> STDRESULT {
        return self.modify(name,Some(true),None,None);
    }
    fn unlock(&mut self,name: &str) -> STDRESULT {
        return self.modify(name,Some(false),None,None);
    }
    fn rename(&mut self,old_name: &str,new_name: &str) -> STDRESULT {
        self.ok_to_rename(new_name)?;
        return self.modify(old_name,None,Some(new_name),None);
    }
    fn retype(&mut self,name: &str,new_type: &str,_sub_type: &str) -> STDRESULT {
        return self.modify(name, None,None, Some(new_type));
    }
    fn bload(&mut self,name: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        match self.read_file(name) {
            Ok(fimg) => {
                let ans = types::BinaryData::from_bytes(&fimg.sequence());
                Ok((u16::from_le_bytes(ans.start),ans.data))
            },
            Err(e) => Err(e)
        }
    }
    fn bsave(&mut self,name: &str, dat: &[u8],start_addr: u16,trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let file = types::BinaryData::pack(&dat,start_addr);
        let padded = match trailing {
            Some(v) => [file.to_bytes(),v.to_vec()].concat(),
            None => file.to_bytes()
        };
        let mut fimg = Disk::new_fimg(256);
        fimg.desequence(&padded);
        fimg.fs_type = vec![FileType::Binary as u8];
        return self.write_file(name, &fimg);
    }
    fn load(&mut self,name: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        match self.read_file(name) {
            Ok(fimg) => {
                let tokens = types::TokenizedProgram::from_bytes(&fimg.sequence()).program;
                match FileType::from_u8(fimg.fs_type[0] & 0x7f) {
                    Some(FileType::Integer) => Ok((0,tokens)),
                    Some(FileType::Applesoft) => Ok((applesoft::deduce_address(&tokens),tokens)),
                    _ => Err(Box::new(Error::FileTypeMismatch))
                }
            },
            Err(e) => Err(e)
        }
    }
    fn save(&mut self,name: &str, dat: &[u8], typ: ItemType, trailing: Option<&[u8]>) -> Result<usize,DYNERR> {
        let padded = types::TokenizedProgram::pack(&dat,trailing).to_bytes();
        let fs_type = match typ {
            ItemType::ApplesoftTokens => FileType::Applesoft,
            ItemType::IntegerTokens => FileType::Integer,
            _ => return Err(Box::new(Error::FileTypeMismatch))
        };
        let mut fimg = Disk::new_fimg(256);
        fimg.desequence(&padded);
        fimg.fs_type = vec![fs_type as u8];
        return self.write_file(name, &fimg);
    }
    fn read_raw(&mut self,name: &str,_trunc: bool) -> Result<(u16,Vec<u8>),DYNERR> {
        // eof is not generally available in DOS 3.x
        match self.read_file(name) {
            Ok(fimg) => Ok((0,fimg.sequence())),
            Err(e) => Err(e)
        }
    }
    fn write_raw(&mut self,name: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        let mut fimg = Disk::new_fimg(256);
        fimg.desequence(dat);
        fimg.fs_type = vec![FileType::Text as u8];
        return self.write_file(name, &fimg);
    }
    fn read_text(&mut self,name: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        self.read_raw(name,false)
    }
    fn write_text(&mut self,name: &str, dat: &[u8]) -> Result<usize,DYNERR> {
        self.write_raw(name,dat)
    }
    fn read_records(&mut self,name: &str,record_length: usize) -> Result<super::Records,DYNERR> {
        if record_length==0 {
            error!("DOS 3.x requires specifying a non-zero record length");
            return Err(Box::new(Error::Range));
        }
        let encoder = Encoder::new(vec![0x8d]);
        match self.read_file(name) {
            Ok(fimg) => {
                match super::Records::from_fimg(&fimg,record_length,encoder) {
                    Ok(ans) => Ok(ans),
                    Err(e) => Err(e)
                }
            },
            Err(e) => return Err(e)
        }
    }
    fn write_records(&mut self,name: &str, records: &super::Records) -> Result<usize,DYNERR> {
        let encoder = Encoder::new(vec![0x8d]);
        let mut fimg = self.new_fimg(256);
        fimg.fs_type = vec![FileType::Text as u8];
        match records.update_fimg(&mut fimg, false, encoder) {
            Ok(_) => self.write_file(name,&fimg),
            Err(e) => Err(e)
        }
    }
    fn read_block(&mut self,num: &str) -> Result<(u16,Vec<u8>),DYNERR> {
        let vtoc = self.get_vtoc_ref()?;
        match usize::from_str(num) {
            Ok(sector) => {
                if sector > vtoc.tracks as usize*vtoc.sectors as usize {
                    return Err(Box::new(Error::Range));
                }
                let mut buf: Vec<u8> = vec![0;256];
                self.read_sector(&mut buf,[(sector/16) as u8,(sector%16) as u8],0)?;
                Ok((0,buf))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_block(&mut self,num: &str,dat: &[u8]) -> Result<usize,DYNERR> {
        let vconst = self.get_vtoc_constants()?;
        let bytes_per_sector = u16::from_le_bytes(vconst.bytes);
        match usize::from_str(num) {
            Ok(sector) => {
                if dat.len()>bytes_per_sector as usize || sector > vconst.tracks as usize*vconst.sectors as usize {
                    return Err(Box::new(Error::Range));
                }
                self.zap_sector(&dat,[(sector/16) as u8,(sector%16) as u8],0,bytes_per_sector)?;
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&mut self,name: &str) -> Result<super::FileImage,DYNERR> {
        return self.read_file(name);
    }
    fn write_any(&mut self,name: &str,fimg: &super::FileImage) -> Result<usize,DYNERR> {
        if fimg.file_system!="a2 dos" {
            error!("cannot write {} file image to a2 dos",fimg.file_system);
            return Err(Box::new(Error::IOError));
        }
        if fimg.chunk_len!=256 {
            error!("chunk length is incompatible with DOS 3.x");
            return Err(Box::new(Error::Range));
        }
        return self.write_file(name,fimg);
    }
    fn decode_text(&self,dat: &[u8]) -> Result<String,DYNERR> {
        let file = types::SequentialText::from_bytes(&dat.to_vec());
        Ok(file.to_string())
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,DYNERR> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(_) => {
                error!("Cannot encode, perhaps use raw type");
                Err(Box::new(Error::FileTypeMismatch))
            }
        }
    }
    fn standardize(&mut self,_ref_con: u16) -> HashMap<Block,Vec<usize>> {
        // ignore first byte of VTOC
        return HashMap::from([(self.addr([VTOC_TRACK,0]),vec![0])]);
    }
    fn compare(&mut self,path: &std::path::Path,ignore: &HashMap<Block,Vec<usize>>) {
        let vconst = self.get_vtoc_constants().expect("could not get VTOC buffer");
        self.writeback_vtoc_buffer().expect("could not write back VTOC buffer");
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        for track in 0..vconst.tracks as usize {
            for sector in 0..vconst.sectors as usize {
                let addr = self.addr([track as u8,sector as u8]);
                let mut actual = self.img.read_block(addr).expect("bad sector access");
                let mut expected = emulator_disk.get_img().read_block(addr).expect("bad sector access");
                if let Some(ignorable) = ignore.get(&addr) {
                    for offset in ignorable {
                        actual[*offset] = 0;
                        expected[*offset] = 0;
                    }
                }
                for row in 0..8 {
                    let mut fmt_actual = String::new();
                    let mut fmt_expected = String::new();
                    let offset = row*32;
                    write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
                    write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
                    assert_eq!(fmt_actual,fmt_expected," at track {}, sector {}, row {}",track,sector,row)
                }
            }
        }
    }
    fn get_img(&mut self) -> &mut Box<dyn img::DiskImage> {
        self.writeback_vtoc_buffer().expect("could not write back VTOC buffer");
        &mut self.img
    }
}