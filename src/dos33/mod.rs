//! # DOS 3.3 disk image library
//! This manipulates disk images containing one standard bootable
//! or non-bootable DOS 3.3 small volume (140K).
//! 
//! * Image types: DOS ordered images (.DO,.DSK)
//! * Analogues of BASIC commands like SAVE, BSAVE etc. are exposed through the `A2Disk` trait
//! * The library will try to emulate the order in which DOS would access sectors, but
//! this is not intended to be exact.

pub mod types;
mod boot;
mod directory;

use std::collections::HashMap;
use std::str::FromStr;
use std::fmt::Write;
use a2kit_macro::DiskStruct;

use types::*;
use crate::disk_base::TextEncoder;
use directory::*;
use crate::disk_base;
use crate::create_disk_from_file;

fn file_name_to_string(fname: [u8;30]) -> String {
    // fname is negative ASCII padded to the end with spaces
    // UTF8 failure will cause panic
    let mut copy = fname.clone();
    for i in 0..30 {
        copy[i] -= 128;
    }
    if let Ok(result) = String::from_utf8(copy.to_vec()) {
        return result.trim_end().to_string();
    }
    panic!("encountered a bad file name");
}

fn string_to_file_name(s: &str) -> [u8;30] {
    // this assumes the String contains only ASCII characters, if not panic
    let mut ans: [u8;30] = [32;30]; // load with ascii spaces
    let mut i = 0;
    if s.len() > 30 {
        panic!("file name too long");
    }
    for char in s.to_uppercase().chars() {
        if !char.is_ascii() {
            panic!("encountered non-ascii while forming file name");
        }
        char.encode_utf8(&mut ans[i..]);
        i += 1;
    }
    // Put it all in negative ascii
    for i in 0..30 {
        ans[i] += 128;
    }
    return ans;
}


/// The primary interface for disk operations.
/// At present only BASIC-like commands (SAVE, BLOAD, etc.) are exposed.
pub struct Disk
{
    // 16 sectors hard coded here
    vtoc: VTOC,
    tracks: [[[u8;256];16];35]
}

impl Disk
{
    /// Create an empty disk, every byte of every sector of every track is 0
    pub fn new() -> Self {
        return Self {
            vtoc: VTOC::new(),
            tracks: [[[0;256];16];35]
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
        let vtoc_bytes = self.vtoc.to_bytes();
        for i in 0..vtoc_bytes.len() {
            self.tracks[17][0][i] = vtoc_bytes[i];
        }
    }
    fn update_last_track(&mut self,track: u8) {
        // The last_direction and last_track fields are not discussed in DOS manual.
        // This way of setting them is a guess based on emulator outputs.
        // If/how they are used in the free sector search is yet another question.
        if track<17 {
            self.vtoc.last_direction = 255;
            self.vtoc.last_track = track;
        }
        if track>17 {
            self.vtoc.last_direction = 1;
            self.vtoc.last_track = track;
        }
        // save it in the actual VTOC image
        let vtoc_bytes = self.vtoc.to_bytes();
        for i in 0..vtoc_bytes.len() {
            self.tracks[17][0][i] = vtoc_bytes[i];
        }
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
    fn read_sector(&self,data: &mut Vec<u8>,ts: [u8;2], offset: usize) {
        // copy data from track and sector
        let bytes = u16::from_le_bytes(self.vtoc.bytes) as i32;
        let actual_len = match data.len() as i32 - offset as i32 {
            x if x<0 => panic!("invalid offset in read sector"),
            x if x<=bytes => x,
            _ => bytes
        };
        let [track,sector] = ts;
        for i in 0..actual_len as usize {
            data[offset + i] = self.tracks[track as usize][sector as usize][i];
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
            x if x<=bytes => x,
            _ => bytes
        };
        let [track,sector] = ts;
        for i in 0..actual_len as usize {
            self.tracks[track as usize][sector as usize][i] = data[offset + i];
        }
    }
    /// Create a standard DOS 3.3 small volume (140K)
    pub fn format(&mut self,vol:u8,bootable:bool,last_track_written:u8) {
        // First write the Volume Table of Contents (VTOC)
        assert!(vol>0 && vol<255);
        assert!(last_track_written>0 && last_track_written<35);

        self.vtoc.pad1 = 4;
        self.vtoc.vol = vol;
        self.vtoc.last_track = last_track_written;
        self.vtoc.last_direction = 1;
        self.vtoc.max_pairs = 0x7a;
        self.vtoc.track1 = 17;
        self.vtoc.sector1 = 15;
        self.vtoc.version = 3;
        self.vtoc.bytes = [0,1];
        self.vtoc.sectors = 16;
        self.vtoc.tracks = 35;
        // Mark as free except track 0
        for track in 1..35 {
            self.vtoc.bitmap[track*4] = 255; // sectors 8-F
            self.vtoc.bitmap[track*4+1] = 255; // sectors 0-7
        }
        // If bootable mark DOS tracks as entirely used
        if bootable {
            self.vtoc.bitmap[4] = 0;
            self.vtoc.bitmap[5] = 0;
            self.vtoc.bitmap[8] = 0;
            self.vtoc.bitmap[9] = 0;
        }
        // Mark track 17 as entirely used (VTOC and directory)
        self.vtoc.bitmap[17*4] = 0;
        self.vtoc.bitmap[17*4+1] = 0;
        // write the sector and save the records in this object
        self.write_sector(&self.vtoc.to_bytes(),[17,0],0);

        // Next write the directory tracks

        let mut dir = DirectorySector::new();
        self.write_sector(&dir.to_bytes(),[17,1],0);
        for sec in 2 as u8..16 as u8 {
            dir.next_track = 17;
            dir.next_sector = sec - 1;
            self.write_sector(&dir.to_bytes(),[17,sec],0);
        }

        if bootable {
            let flat = boot::DOS33_TRACKS;
            for track in 0..3 {
                for sector in 0..16 {
                    for byte in 0..256 {
                        self.tracks[track][sector][byte] = flat[byte + sector*256 + track*256*16]
                    }
                }
            }
        }
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
    /// Read any file into the sparse file format.  Use `SparseData.sequence()` to flatten the result
    /// when it is expected to be sequential.
    fn read_file(&self,name: &str) -> Result<disk_base::SparseData,Box<dyn std::error::Error>> {
        // resulting vector will be padded modulo 256
        let (mut next_tslist,ftype) = self.get_tslist_sector(name);
        if next_tslist==[0,0] {
            return Err(Box::new(Error::FileNotFound));
        }
        let mut ans = disk_base::SparseData::new(256);
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
                ans.new_type(&ftype.to_string());
                return Ok(ans);
            }
            next_tslist = [tslist.next_track,tslist.next_sector];
        }
        panic!("the disk image track sector list seems to be damaged");
    }
    /// Write any sparse or sequential file.  Use `SparseData::desequence` to put sequential data
    /// into the sparse file format, with no loss of generality.
    /// Unlike DOS, nothing is written unless there is enough space for all the data.
    fn write_file(&mut self,name: &str, dat: &disk_base::SparseData) -> Result<usize,Box<dyn std::error::Error>> {
        let (named_ts,_ftype) = self.get_tslist_sector(name);
        if named_ts==[0,0] {
            // this is a new file
            // unlike DOS, we do not write anything unless there is room
            assert!(dat.chunks.len()>0);
            let data_sectors = dat.chunks.len();
            let tslist_sectors = 1 + (dat.end()-1)/self.vtoc.max_pairs as usize;
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
            match Type::from_str(&dat.fs_type) {
                Ok(t) => dir.entries[e as usize].file_type = t as u8,
                Err(e) => return Err(Box::new(e))
            } 
            dir.entries[e as usize].name = string_to_file_name(name);
            dir.entries[e as usize].sectors = [tslist_sectors as u8 + data_sectors as u8 ,0];
            self.write_sector(&dir.to_bytes(), ts, 0);

            // write the data and TS list as we go
            for s in 0..dat.end() {
                if let Some(chunk) = dat.chunks.get(&s) {
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
                if p==self.vtoc.max_pairs as usize  && s+1!=dat.end() {
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
    /// modify a file entry, optionally lock, unlock, and/or rename; attempt to rename already locked file will fail.
    fn modify(&mut self,name: &str,maybe_lock: Option<bool>,maybe_new_name: Option<&str>) -> Result<(),Box<dyn std::error::Error>> {
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
    /// Test the image for compatibility and return Some(disk) or None.
    pub fn from_img(img: &Vec<u8>) -> Option<Self> {
        let mut disk = Self::new();
        let tlen = 35 as usize;
        let slen = 16 as usize;
        let blen = 256 as usize;
        if img.len()!=tlen*slen*blen {
            return None;
        }
        for track in 0..tlen {
            for sector in 0..slen {
                for byte in 0..blen {
                    disk.tracks[track][sector][byte] = img[byte+sector*blen+track*slen*blen];
                }
            }
        }
        disk.vtoc = VTOC::from_bytes(&disk.tracks[17][0].to_vec());
        if disk.vtoc.bytes != [0,1] || disk.vtoc.track1 != 17 || disk.vtoc.sector1 != 15 || disk.vtoc.sectors != 16 || disk.vtoc.tracks != 35 {
            return None;
        }
        return Some(disk);
    }
}

impl disk_base::A2Disk for Disk {
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
        return self.modify(name,Some(true),None);
    }
    fn unlock(&mut self,name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(name,Some(false),None);
    }
    fn rename(&mut self,old_name: &str,new_name: &str) -> Result<(),Box<dyn std::error::Error>> {
        return self.modify(old_name,None,Some(new_name));
    }
    fn bload(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(v) => {
                let ans = types::BinaryData::from_bytes(&v.sequence());
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
        return self.write_file(name, &disk_base::SparseData::desequence(256, &padded).new_type("bin"));
    }
    fn load(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(v) => Ok((0,types::TokenizedProgram::from_bytes(&v.sequence()).program)),
            Err(e) => Err(e)
        }
    }
    fn save(&mut self,name: &str, dat: &Vec<u8>, typ: disk_base::ItemType, trailing: Option<&Vec<u8>>) -> Result<usize,Box<dyn std::error::Error>> {
        let file = types::TokenizedProgram::pack(&dat,trailing);
        let fs_type = match typ {
            disk_base::ItemType::ApplesoftTokens => "atok",
            disk_base::ItemType::IntegerTokens => "itok",
            _ => panic!("attempt to SAVE non-BASIC data type")
        };
        return self.write_file(name, &disk_base::SparseData::desequence(256,&file.to_bytes()).new_type(fs_type));
    }
    fn read_text(&self,name: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        match self.read_file(name) {
            Ok(sd) => {
                Ok((0,types::SequentialText::from_bytes(&sd.sequence()).text))
            },
            Err(e) => Err(e)
        }
    }
    fn write_text(&mut self,name: &str, dat: &Vec<u8>) -> Result<usize,Box<dyn std::error::Error>> {
        let file = types::SequentialText::pack(&dat);
        return self.write_file(name, &disk_base::SparseData::desequence(256,&file.to_bytes()).new_type("txt"));
    }
    fn read_records(&self,name: &str,record_length: usize) -> Result<disk_base::Records,Box<dyn std::error::Error>> {
        if record_length==0 {
            eprintln!("DOS 3.3 requires specifying a non-zero record length");
            return Err(Box::new(Error::Range));
        }
        let encoder = Encoder::new(Some(0x8d));
        match self.read_file(name) {
            Ok(sd) => {
                match disk_base::Records::from_sparse_data(&sd,record_length,encoder) {
                    Ok(ans) => Ok(ans),
                    Err(e) => Err(e)
                }
            },
            Err(e) => return Err(e)
        }
    }
    fn write_records(&mut self,name: &str, records: &disk_base::Records) -> Result<usize,Box<dyn std::error::Error>> {
        let encoder = Encoder::new(Some(0x8d));
        if let Ok(sparse_data) = records.to_sparse_data(256,false,encoder) {
            return self.write_file(name, &sparse_data);
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
    fn read_any(&self,name: &str) -> Result<disk_base::SparseData,Box<dyn std::error::Error>> {
        return self.read_file(name);
    }
    fn write_any(&mut self,name: &str,dat: &disk_base::SparseData) -> Result<usize,Box<dyn std::error::Error>> {
        if dat.chunk_len!=256 {
            eprintln!("chunk length {} is incompatible with DOS 3.3",dat.chunk_len);
            return Err(Box::new(Error::Range));
        }
        return self.write_file(name,dat);
    }
    fn decode_text(&self,dat: &Vec<u8>) -> String {
        let file = types::SequentialText::pack(&dat);
        return file.to_string();
    }
    fn encode_text(&self,s: &str) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        let file = types::SequentialText::from_str(&s);
        match file {
            Ok(txt) => Ok(txt.to_bytes()),
            Err(e) => Err(Box::new(e))
        }
    }
    fn standardize(&self,_ref_con: u16) -> Vec<usize> {
        return vec![17*16*256];
    }
    fn compare(&self,path: &std::path::Path,ignore: &Vec<usize>) {
        let emulator_disk = create_disk_from_file(&path.to_str().expect("could not unwrap path"));
        let mut expected = emulator_disk.to_img();
        let mut actual = self.to_img();
        for ignorable in ignore {
            expected[*ignorable] = 0;
            actual[*ignorable] = 0;
        }
        for track in 0..self.vtoc.tracks as usize {
            for sector in 0..self.vtoc.sectors as usize {
                for row in 0..8 {
                    let mut fmt_actual = String::new();
                    let mut fmt_expected = String::new();
                    let offset = track*16*256 + sector*256 + row*32;
                    write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
                    write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
                    assert_eq!(fmt_actual,fmt_expected," at track {}, sector {}, row {}",track,sector,row)
                }
            }
        }
    }
    fn to_img(&self) -> Vec<u8> {
        let mut result : Vec<u8> = Vec::new();
        for track in 0..self.vtoc.tracks as usize {
            for sector in 0..self.vtoc.sectors as usize {
                for byte in 0..u16::from_le_bytes(self.vtoc.bytes) as usize {
                    result.push(self.tracks[track][sector][byte]);
                }
            }
        }
        return result;
    }
}