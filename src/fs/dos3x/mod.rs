//! # DOS 3.x file system module
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
use log::{debug,error};

use types::*;
use directory::*;
use super::{Chunk,TextEncoder};
use crate::img;
use crate::commands::ItemType;

fn file_name_to_string(fname: [u8;30]) -> String {
    // fname is negative ASCII padded to the end with spaces
    // non-ASCII will go as hex escapes
    return String::from(crate::escaped_ascii_from_bytes(&fname.to_vec(),true,true).trim_end());
}

fn string_to_file_name(s: &str) -> [u8;30] {
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
    vtoc: VTOC,
    img: Box<dyn img::DiskImage>
}

impl Disk
{
    /// Create a disk file system using the given image as storage.
    /// The DiskFS takes ownership of the image.
    pub fn from_img(img: Box<dyn img::DiskImage>) -> Self {
        if let Ok(dat) = img.read_chunk(Chunk::D13([17,0])) {
            return Self {
                vtoc: VTOC::from_bytes(&dat),
                img
            };
        }
        if let Ok(dat) = img.read_chunk(Chunk::DO([17,0])) {
            return Self {
                vtoc: VTOC::from_bytes(&dat),
                img
            };
        }
        panic!("unexpected failure to read chunk");
    }
    fn test_img_13(img: &Box<dyn img::DiskImage>) -> bool {
        if let Ok(dat) = img.read_chunk(Chunk::D13([17,0])) {
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
    fn test_img_16(img: &Box<dyn img::DiskImage>) -> bool {
        if let Ok(dat) = img.read_chunk(Chunk::DO([17,0])) {
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
    pub fn test_img(img: &Box<dyn img::DiskImage>) -> bool {
        let tlen = img.track_count();
        if tlen!=35 {
            debug!("track count is unexpected");
            return false;
        }
        if Self::test_img_13(img) {
            return true;
        }
        if Self::test_img_16(img) {
            return true;
        }
        return false;
    }
    fn addr(&self,ts: [u8;2]) -> Chunk {
        match self.vtoc.sectors {
            13 => Chunk::D13([ts[0] as usize,ts[1] as usize]),
            _ => Chunk::DO([ts[0] as usize,ts[1] as usize])
        }
    }
    fn panic_if_ts_bad(&self,track: u8,sector: u8) {
        if track>=self.vtoc.tracks || sector>=self.vtoc.sectors {
            panic!("attempt to access outside disk bounds, image may be damaged");
        }
    }
    fn get_track_map(&self,track: u8) -> u32 {
        let bm = &self.vtoc.bitmap;
        let i = (track*4) as usize;
        return u32::from_be_bytes([bm[i],bm[i+1],bm[i+2],bm[i+3]]);
    }
    fn save_track_map(&mut self,track: u8,map: u32) {
        let i = (track*4) as usize;
        let slice: [u8;4] = u32::to_be_bytes(map);
        // save it in the auxiliary structure
        self.vtoc.bitmap[i] = slice[0];
        self.vtoc.bitmap[i+1] = slice[1];
        self.vtoc.bitmap[i+2] = slice[2];
        self.vtoc.bitmap[i+3] = slice[3];
        // save it in the actual VTOC image
        self.img.write_chunk(self.addr([VTOC_TRACK,0]), &self.vtoc.to_bytes()).expect("write error");
    }
    fn update_last_track(&mut self,track: u8) {
        // The last_direction and last_track fields are not discussed in DOS manual.
        // This way of setting them is a guess based on emulator outputs.
        // If/how they are used in the free sector search is yet another question.
        if track<VTOC_TRACK {
            self.vtoc.last_direction = 255;
            self.vtoc.last_track = track;
        }
        if track>VTOC_TRACK {
            self.vtoc.last_direction = 1;
            self.vtoc.last_track = track;
        }
        // save it in the actual VTOC image
        self.img.write_chunk(self.addr([VTOC_TRACK,0]), &self.vtoc.to_bytes()).expect("write error");
    }
    fn allocate_sector(&mut self,track: u8,sector: u8) {
        let mut map = self.get_track_map(track);
        let eff_sec: u32 = (sector + 32 - self.vtoc.sectors) as u32;
        map &= (1 << eff_sec) ^ u32::MAX;
        self.save_track_map(track,map);
    }
    fn deallocate_sector(&mut self,track: u8,sector: u8) {
        let mut map = self.get_track_map(track);
        let eff_sec: u32 = (sector + 32 - self.vtoc.sectors) as u32;
        map |= 1 << eff_sec;
        self.save_track_map(track,map);
    }
    fn is_sector_free(&self,track: u8,sector: u8) -> bool {
        let map = self.get_track_map(track);
        let eff_sec: u32 = (sector + 32 - self.vtoc.sectors) as u32;
        return (map & (1 << eff_sec)) > 0;
    }
    /// Read a sector of data into buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the sector, the partial sector is copied.
    fn read_sector(&self,data: &mut Vec<u8>,ts: [u8;2], offset: usize) {
        let bytes = u16::from_le_bytes(self.vtoc.bytes) as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read sector"),
            x if x<=bytes => x,
            _ => bytes
        };
        if let Ok(buf) = self.img.read_chunk(self.addr(ts)) {
            for i in 0..actual_len as usize {
                data[offset + i] = buf[i];
            }
        } else {
            panic!("read failed for track {} sector {}",ts[0],ts[1]);
        }
    }
    /// Zap and allocate the sector in one step.
    fn write_sector(&mut self,data: &Vec<u8>,ts: [u8;2], offset: usize) {
        self.zap_sector(data,ts,offset);
        self.allocate_sector(ts[0],ts[1]);
    }
    /// Writes a sector of data from buffer `data`, starting at `offset` within the buffer.
    /// If `data` is shorter than the sector, trailing bytes are unaffected.
    fn zap_sector(&mut self,data: &Vec<u8>,ts: [u8;2], offset: usize) {
        // copy data to track and sector
        let bytes = u16::from_le_bytes(self.vtoc.bytes) as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in write sector"),
            x if x<=bytes => x as usize,
            _ => bytes as usize
        };
        self.img.write_chunk(self.addr(ts), &data[offset..offset+actual_len].to_vec()).
            expect("write failed");
    }
    /// Create any DOS 3.x volume
    pub fn init(&mut self,vol:u8,bootable:bool,last_track_written:u8,tracks:u8,sectors:u8) {
        assert!(vol>0 && vol<255);
        assert!(tracks>VTOC_TRACK && tracks<=50);
        assert!(sectors==13 || sectors==16 || sectors==32);
        assert!(last_track_written>0 && last_track_written<tracks);

        // First write the Volume Table of Contents (VTOC)
        self.vtoc.pad1 = match sectors {
            13 => 2,
            16 | 32 => 4,
            _ => panic!("unexpected sector count")
        };
        self.vtoc.vol = vol;
        self.vtoc.last_track = last_track_written;
        self.vtoc.last_direction = 1;
        self.vtoc.max_pairs = 0x7a;
        self.vtoc.track1 = VTOC_TRACK;
        self.vtoc.sector1 = sectors-1;
        self.vtoc.version = match sectors {
            13 => 2,
            16 | 32 => 3,
            _ => panic!("unexpected sector count")
        };
        self.vtoc.bytes = [0,1];
        self.vtoc.sectors = sectors;
        self.vtoc.tracks = tracks;
        // Mark as free except track 0
        let all_free: [u8;4] = match sectors {
            13 => u32::to_be_bytes(0xfff80000),
            16 => u32::to_be_bytes(0xffff0000),
            32 => u32::to_be_bytes(0xffffffff),
            _ => panic!("unexpected sector count")
        };
        for track in 1..tracks as usize {
            self.vtoc.bitmap[track*4+0] = all_free[0];
            self.vtoc.bitmap[track*4+1] = all_free[1];
            self.vtoc.bitmap[track*4+2] = all_free[2];
            self.vtoc.bitmap[track*4+3] = all_free[3];
        }
        // If bootable mark DOS tracks as entirely used
        if bootable {
            for i in 1*4..3*4 {
                self.vtoc.bitmap[i] = 0;
            }
        }
        // Mark track VTOC_TRACK as entirely used (VTOC and directory)
        for i in VTOC_TRACK*4..(VTOC_TRACK+1)*4 {
            self.vtoc.bitmap[i as usize] = 0;
        }
        // write the sector and save the records in this object
        self.write_sector(&self.vtoc.to_bytes(),[VTOC_TRACK,0],0);
        // Write the directory sectors
        let mut dir = DirectorySector::new();
        self.write_sector(&dir.to_bytes(),[VTOC_TRACK,1],0);
        for sec in 2 as u8..sectors as u8 {
            dir.next_track = VTOC_TRACK;
            dir.next_sector = sec - 1;
            self.write_sector(&dir.to_bytes(),[VTOC_TRACK,sec],0);
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
                    self.write_sector(&flat,[track as u8,sector as u8],offset)
                }
            }
        }
    }
    /// Create a standard DOS 3.2 volume (116K)
    pub fn init32(&mut self,vol:u8,bootable:bool) {
        self.init(vol,bootable,17,35,13);
    }
    /// Create a standard DOS 3.3 small volume (140K)
    pub fn init33(&mut self,vol:u8,bootable:bool) {
        self.init(vol,bootable,17,35,16);
    }
    fn num_free_sectors(&self) -> usize {
        let mut ans: usize = 0;
        for track in 0..self.vtoc.tracks {
            for sector in 0..self.vtoc.sectors {
                if self.is_sector_free(track, sector) {
                    ans += 1;
                }
            }
        }
        return ans;
    }
    fn get_next_free_sector(&self,prefer_jump: bool) -> [u8;2] {
        // Search algorithm outlined in DOS manual seems inconsistent with actual results from emulators.
        // This algorithm is a guess at how DOS is doing it, based on emulator outputs.
        // Fortunately we don't have to emulate this exactly for the disk to work.
        let tvtoc: u8 = self.vtoc.track1;
        let tstart = match self.vtoc.last_track {
            x if x>=self.vtoc.tracks => tvtoc-1,
            x if x>tvtoc && prefer_jump => x+1,
            x if x<tvtoc && prefer_jump => x-1,
            x => x
        };
        let tend = self.vtoc.tracks;
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
            for sector in (0..self.vtoc.sectors).rev() {
                if self.is_sector_free(track,sector) {
                    return [track,sector];
                }
            }
        }
        return [0,0];
    }
    /// Return a tuple with ([track,sector],entry index)
    fn get_next_directory_slot(&self) -> ([u8;2],u8) {
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        let mut buf = vec![0;256];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            self.panic_if_ts_bad(ts[0], ts[1]);
            self.read_sector(&mut buf, ts, 0);
            let dir = DirectorySector::from_bytes(&buf);
            for e in 0..7 {
                if dir.entries[e].tsl_track==0 || dir.entries[e].tsl_track==255 {
                    return (ts,e as u8);
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return (ts,0);
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
    /// Scan the directory sectors to find the tslist of the named file and the file type
    fn get_tslist_sector(&self,name: &str) -> ([u8;2],u8) {
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            self.panic_if_ts_bad(ts[0], ts[1]);
            self.read_sector(&mut buf, ts, 0);
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    return ([entry.tsl_track,entry.tsl_sector],entry.file_type);
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return (ts,0);
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
    /// Read any file into the sparse file format.  Use `FileImage.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&self,name: &str) -> Result<super::FileImage,Box<dyn std::error::Error>> {
        let (mut next_tslist,ftype) = self.get_tslist_sector(name);
        if next_tslist==[0,0] {
            return Err(Box::new(Error::FileNotFound));
        }
        let mut ans = super::FileImage::new(256);
        ans.file_system = String::from("a2 dos");
        ans.version = self.vtoc.version as u32;
        let mut buf = vec![0;256];
        let mut count: usize = 0;
        // loop up to a maximum, if it is reached panic
        for _try in 0..types::MAX_TSLIST_REPS {
            self.read_sector(&mut buf,next_tslist,0);
            let tslist = TrackSectorList::from_bytes(&buf);
            for p in 0..self.vtoc.max_pairs as usize {
                let next = [tslist.pairs[p*2],tslist.pairs[p*2+1]];
                if next[0]>0 {
                    let mut full_buf: Vec<u8> = vec![0;256];
                    self.read_sector(&mut full_buf,next,0);
                    ans.chunks.insert(count,full_buf);
                }
                count += 1;
            }
            if tslist.next_track==0 {
                ans.fs_type = ftype as u32;
                return Ok(ans);
            }
            next_tslist = [tslist.next_track,tslist.next_sector];
        }
        panic!("the disk image track sector list seems to be damaged");
    }
    /// Write any sparse or sequential file.  Use `FileImage::desequence` to put sequential data
    /// into the sparse file format, with no loss of generality.
    /// Unlike DOS, nothing is written unless there is enough space for all the data.
    fn write_file(&mut self,name: &str, fimg: &super::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if fimg.chunks.len()==0 {
            error!("empty data is not allowed for DOS 3.x file images");
            return Err(Box::new(Error::EndOfData));
        }
        let (named_ts,_ftype) = self.get_tslist_sector(name);
        if named_ts==[0,0] {
            // this is a new file
            // unlike DOS, we do not write anything unless there is room
            assert!(fimg.chunks.len()>0);
            let data_sectors = fimg.chunks.len();
            let tslist_sectors = 1 + (fimg.end()-1)/self.vtoc.max_pairs as usize;
            if data_sectors + tslist_sectors > self.num_free_sectors() {
                return Err(Box::new(Error::DiskFull));
            }

            // we are doing this
            let mut sec_base = 0; // in units of pairs
            let mut p = 0; // pairs written in current tslist sector
            let mut tslist = TrackSectorList::new();
            let mut tslist_ts = self.get_next_free_sector(true);
            self.allocate_sector(tslist_ts[0],tslist_ts[1]); // reserve this sector
            self.update_last_track(tslist_ts[0]);

            // write the directory entry
            let (ts,e) = self.get_next_directory_slot();
            let mut dir_buf = vec![0;256];
            self.read_sector(&mut dir_buf, ts, 0);
            let mut dir = DirectorySector::from_bytes(&dir_buf);
            dir.entries[e as usize].tsl_track = tslist_ts[0];
            dir.entries[e as usize].tsl_sector = tslist_ts[1];
            match FileType::from_u32(fimg.fs_type) {
                Some(t) => dir.entries[e as usize].file_type = t as u8,
                None => return Err(Box::new(Error::Range))
            } 
            dir.entries[e as usize].name = string_to_file_name(name);
            dir.entries[e as usize].sectors = [tslist_sectors as u8 + data_sectors as u8 ,0];
            self.write_sector(&dir.to_bytes(), ts, 0);

            // write the data and TS list as we go
            for s in 0..fimg.end() {
                if let Some(chunk) = fimg.chunks.get(&s) {
                    let data_ts = self.get_next_free_sector(false);
                    tslist.pairs[p*2] = data_ts[0];
                    tslist.pairs[p*2+1] = data_ts[1];
                    self.write_sector(&tslist.to_bytes(), tslist_ts, 0);
                    self.write_sector(chunk,data_ts,0);
                    self.update_last_track(data_ts[0]);
                } else {
                    tslist.pairs[p*2] = 0;
                    tslist.pairs[p*2+1] = 0;
                    self.write_sector(&tslist.to_bytes(), tslist_ts, 0);
                }
                p += 1;
                if p==self.vtoc.max_pairs as usize  && s+1!=fimg.end() {
                    // tslist spilled over to another sector
                    let next_tslist_ts = self.get_next_free_sector(false);
                    tslist.next_track = next_tslist_ts[0];
                    tslist.next_sector = next_tslist_ts[1];
                    self.write_sector(&tslist.to_bytes(),tslist_ts,0);
                    self.update_last_track(tslist_ts[0]);
                    tslist_ts = next_tslist_ts;
                    sec_base += self.vtoc.max_pairs as usize;
                    tslist = TrackSectorList::new();
                    tslist.sector_base = u16::to_le_bytes(sec_base as u16);
                    p = 0;
                }
            }
            
            return Ok(data_sectors + tslist_sectors);
        } else {
            return Err(Box::new(Error::WriteProtected));
        }
    }
    /// modify a file entry, optionally lock, unlock, rename, retype; attempt to change already locked file will fail.
    fn modify(&mut self,name: &str,maybe_lock: Option<bool>,maybe_new_name: Option<&str>,maybe_ftype: Option<&str>) -> Result<(),Box<dyn std::error::Error>> {
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut dir_ts = [self.vtoc.track1,self.vtoc.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            self.panic_if_ts_bad(dir_ts[0], dir_ts[1]);
            self.read_sector(&mut buf, dir_ts, 0);
            let mut dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_mut() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    if entry.file_type > 127 && maybe_new_name!=None {
                        return Err(Box::new(Error::WriteProtected));
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
                    self.write_sector(&dir.to_bytes(),dir_ts,0);
                    return Ok(());
                }
            }
            dir_ts = [dir.next_track,dir.next_sector];
            if dir_ts == [0,0] {
                return Err(Box::new(Error::FileNotFound));
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
}

impl super::DiskFS for Disk {
    fn catalog_to_stdout(&self, _path: &str) -> Result<(),Box<dyn std::error::Error>> {
        let typ_map: HashMap<u8,&str> = HashMap::from([(0," T"),(1," I"),(2," A"),(4," B"),(128,"*T"),(129,"*I"),(130,"*A"),(132,"*B")]);
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        let mut buf = vec![0;256];
        println!();
        println!("DISK VOLUME {}",self.vtoc.vol);
        println!();
        for _try in 0..types::MAX_DIRECTORY_REPS {
            self.panic_if_ts_bad(ts[0], ts[1]);
            self.read_sector(&mut buf, ts, 0);
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if entry.tsl_track>0 && entry.tsl_track<255 {
                    let name = file_name_to_string(entry.name);
                    let sectors = u16::from_le_bytes(entry.sectors);
                    
                    // TODO: if we actually read the file here we can write out the exact length
                    // and starting address (if applicable)

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
        eprintln!("the disk image directory seems to be damaged");
        return Err(Box::new(Error::IOError));
    }
    fn create(&mut self,_path: &str) -> Result<(),Box<dyn std::error::Error>> {
        eprintln!("DOS 3.x does not support operation");
        return Err(Box::new(Error::SyntaxError));
    }
    fn delete(&mut self,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut dir_ts = [self.vtoc.track1,self.vtoc.sector1];
        for _try in 0..types::MAX_DIRECTORY_REPS {
            self.panic_if_ts_bad(dir_ts[0], dir_ts[1]);
            self.read_sector(&mut buf, dir_ts, 0);
            let mut dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_mut() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    if entry.file_type > 127 {
                        return Err(Box::new(Error::WriteProtected));
                    }
                    let mut tslist_ts = [entry.tsl_track,entry.tsl_sector];
                    for _try2 in 0..types::MAX_TSLIST_REPS {
                        self.read_sector(&mut buf, tslist_ts, 0);
                        let tslist = TrackSectorList::from_bytes(&buf);
                        for p in 0..self.vtoc.max_pairs as usize {
                            if tslist.pairs[p*2]>0 && tslist.pairs[p*2]<255 {
                                self.deallocate_sector(tslist.pairs[p*2], tslist.pairs[p*2+1]);
                            }
                        }
                        self.deallocate_sector(tslist_ts[0], tslist_ts[1]);
                        tslist_ts = [tslist.next_track,tslist.next_sector];
                        if tslist_ts==[0,0] {
                            entry.name[entry.name.len()-1] = entry.tsl_track;
                            entry.tsl_track = 255;
                            self.write_sector(&dir.to_bytes(),dir_ts,0);
                            return Ok(());
                        }
                    }
                    panic!("the disk image track sector list seems to be damaged");
                }
            }
            dir_ts = [dir.next_track,dir.next_sector];
            if dir_ts == [0,0] {
                return Err(Box::new(Error::FileNotFound));
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
    fn lock(&mut self,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(name,Some(true),None,None);
    }
    fn unlock(&mut self,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(name,Some(false),None,None);
    }
    fn rename(&mut self,old_name: &str,new_name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(old_name,None,Some(new_name),None);
    }
    fn retype(&mut self,name: &str,new_type: &str,_sub_type: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(name, None,None, Some(new_type));
    }
    fn bload(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(fimg) => {
                let ans = types::BinaryData::from_bytes(&fimg.sequence());
                Ok((u16::from_le_bytes(ans.start),ans.data))
            },
            Err(e) => Err(e)
        }
    }
    fn bsave(&mut self,name: &str, dat: &Vec<u8>,start_addr: u16,trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let file = types::BinaryData::pack(&dat,start_addr);
        let padded = match trailing {
            Some(v) => [file.to_bytes(),v.clone()].concat(),
            None => file.to_bytes()
        };
        let mut fimg = super::FileImage::desequence(256, &padded);
        fimg.fs_type = FileType::Binary as u32;
        return self.write_file(name, &fimg);
    }
    fn load(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(fimg) => Ok((0,types::TokenizedProgram::from_bytes(&fimg.sequence()).program)),
            Err(e) => Err(e)
        }
    }
    fn save(&mut self,name: &str, dat: &Vec<u8>, typ: ItemType, trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let padded = types::TokenizedProgram::pack(&dat,trailing).to_bytes();
        let fs_type = match typ {
            ItemType::ApplesoftTokens => FileType::Applesoft,
            ItemType::IntegerTokens => FileType::Integer,
            _ => return Err(Box::new(Error::FileTypeMismatch))
        };
        let mut fimg = super::FileImage::desequence(256, &padded);
        fimg.fs_type = fs_type as u32;
        return self.write_file(name, &fimg);
    }
    fn read_text(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(fimg) => Ok((0,fimg.sequence())),
            Err(e) => Err(e)
        }
    }
    fn write_text(&mut self,name: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        let mut fimg = super::FileImage::desequence(256, dat);
        fimg.fs_type = FileType::Text as u32;
        return self.write_file(name, &fimg);
    }
    fn read_records(&self,name: &str,record_length: usize) -> Result<super::Records,Box<dyn std::error::Error>> {
        if record_length==0 {
            eprintln!("DOS 3.x requires specifying a non-zero record length");
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
    fn write_records(&mut self,name: &str, records: &super::Records) -> Result<usize,Box<dyn std::error::Error>> {
        let encoder = Encoder::new(vec![0x8d]);
        if let Ok(fimg) = records.to_fimg(256,FileType::Text as u32,false,encoder) {
            return self.write_file(name, &fimg);
        } else {
            Err(Box::new(Error::SyntaxError))
        }
    }
    fn read_chunk(&self,num: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(sector) => {
                if sector > self.vtoc.tracks as usize*self.vtoc.sectors as usize {
                    return Err(Box::new(Error::Range));
                }
                let mut buf: Vec<u8> = vec![0;256];
                self.read_sector(&mut buf,[(sector/16) as u8,(sector%16) as u8],0);
                Ok((0,buf))
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn write_chunk(&mut self,num: &str,dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        match usize::from_str(num) {
            Ok(sector) => {
                if dat.len()>256 || sector > self.vtoc.tracks as usize*self.vtoc.sectors as usize {
                    return Err(Box::new(Error::Range));
                }
                self.zap_sector(&dat,[(sector/16) as u8,(sector%16) as u8],0);
                Ok(dat.len())
            },
            Err(e) => Err(Box::new(e))
        }
    }
    fn read_any(&self,name: &str) -> Result<super::FileImage,Box<dyn std::error::Error>> {
        return self.read_file(name);
    }
    fn write_any(&mut self,name: &str,fimg: &super::FileImage) -> Result<usize,Box<dyn std::error::Error>> {
        if fimg.chunk_len!=256 {
            eprintln!("chunk length {} is incompatible with DOS 3.x",fimg.chunk_len);
            return Err(Box::new(Error::Range));
        }
        return self.write_file(name,fimg);
    }
    fn decode_text(&self,dat: &Vec<u8>) -> String {
        let file = types::SequentialText::from_bytes(&dat);
        return file.to_string();
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&self,_ref_con: u16) -> HashMap<Chunk,Vec<usize>> {
        // ignore first byte of VTOC
        return HashMap::from([(self.addr([VTOC_TRACK,0]),vec![0])]);
    }
    fn compare(&self,path: &std::path::Path,ignore: &HashMap<Chunk,Vec<usize>>) {
        let mut emulator_disk = crate::create_fs_from_file(&path.to_str().unwrap()).expect("read error");
        for track in 0..self.vtoc.tracks as usize {
            for sector in 0..self.vtoc.sectors as usize {
                let addr = self.addr([track as u8,sector as u8]);
                let mut actual = self.img.read_chunk(addr).expect("bad sector access");
                let mut expected = emulator_disk.get_img().read_chunk(addr).expect("bad sector access");
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
        &mut self.img
    }
}