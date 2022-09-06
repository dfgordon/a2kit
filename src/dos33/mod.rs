//! # DOS 3.3 disk image library
//! This manipulates disk images containing one standard bootable
//! or non-bootable DOS 3.3 small volume (140K).
//! 
//! * Image types: DOS ordered images (.DO)
//! * The library mimics the workings of DOS, i.e., there is something akin to the RWTS
//! subroutine, and there are functions that mirror commands such as SAVE, BSAVE etc..
//! * The library will try to emulate the order in which DOS would access sectors, but
//! this is not intended to be exact.

use thiserror::Error;
use std::collections::HashMap;
mod boot;
pub mod types;

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;

/// Enumerates DOS errors.  The `Display` trait will print equivalent DOS message such as `FILE NOT FOUND`.  Following DOS errors are omitted:
/// LANGUAGE NOT AVAILABLE, WRITE PROTECTED, SYNTAX ERROR, NO BUFFERS AVAILABLE, PROGRAM TOO LARGE, NOT DIRECT COMMAND
#[derive(Error,Debug)]
pub enum DOS33Error {
    #[error("RANGE ERROR")]
    Range,
    #[error("END OF DATA")]
    EndOfData,
    #[error("FILE NOT FOUND")]
    FileNotFound,
    #[error("VOLUME MISMATCH")]
    VolumeMismatch,
    #[error("I/O ERROR")]
    IOError,
    #[error("DISK FULL")]
    DiskFull,
    #[error("FILE LOCKED")]
    FileLocked,
    #[error("FILE TYPE MISMATCH")]
    FileTypeMismatch
}

/// Enumerates the four basic file types, the byte code can be obtained from an instance, e.g. `my_type as u8`
pub enum Type {
    Text = 0x00,
    Integer = 0x01,
    Applesoft = 0x02,
    Binary = 0x04
}

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

fn string_to_file_name(s: &String) -> [u8;30] {
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



// Following are representations of disk directory structures
// these are mostly fixed length structures where the DiskStruct
// trait can be automatically derived.

// Note on large volumes:
// We can extend VTOC.bitmap to 200 bytes, allowing for VTOC.tracks = 50.
// We can extend VTOC.sectors to 32, because the bitmap allocates 32 bits per track.
// This gives 50*32*256 = 409600, i.e., a 400K disk.
// Large DOS volumes were supported on 800K floppies and hard drives by a few third parties.

#[derive(DiskStruct)]
struct VTOC {
    pad1: u8,
    track1: u8,
    sector1: u8,
    version: u8,
    pad2: [u8;2],
    vol: u8,
    pad3: [u8;32],
    max_pairs: u8,
    pad4: [u8;8],
    last_track: u8,
    last_direction: u8,
    pad5: [u8;2],
    tracks: u8,
    sectors: u8,
    bytes: [u8;2],
    bitmap: [u8;140]
}

#[derive(DiskStruct)]
struct TrackSectorList {
    pad1: u8,
    next_track: u8,
    next_sector: u8,
    pad2: [u8;2],
    sector_base: [u8;2],
    pad3: [u8;5],
    pairs: [u8;244]
}

#[derive(DiskStruct)]
struct DirectoryEntry {
    tsl_track: u8,
    tsl_sector: u8,
    file_type: u8,
    name: [u8;30],
    sectors: [u8;2]
}

struct DirectorySector {
    pad1: u8,
    next_track: u8,
    next_sector: u8,
    pad2: [u8;8],
    entries: [DirectoryEntry;7]
}

impl DiskStruct for DirectorySector {
    fn new() -> Self {
        Self {
            pad1: 0,
            next_track: 0,
            next_sector: 0,
            pad2: [0;8],
            entries: [
                DirectoryEntry::new(),
                DirectoryEntry::new(),
                DirectoryEntry::new(),
                DirectoryEntry::new(),
                DirectoryEntry::new(),
                DirectoryEntry::new(),
                DirectoryEntry::new()
            ]
        }
    }
    fn to_bytes(&self) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        ans.push(self.pad1);
        ans.push(self.next_track);
        ans.push(self.next_sector);
        ans.append(&mut self.pad2.to_vec());
        for i in 0..7 {
            ans.append(&mut self.entries[i].to_bytes());
        }
        return ans;
    }
    fn update_from_bytes(&mut self,bytes: &Vec<u8>) {
        self.pad1 = bytes[0];
        self.next_track = bytes[1];
        self.next_sector = bytes[2];
        for i in 0..8 {
            self.pad2[i] = bytes[i+3];
        }
        let mut offset = 0;
        for i in 0..7 {
            self.entries[i].update_from_bytes(&bytes[11+offset..11+offset+self.entries[i].len()].to_vec());
            offset += self.entries[i].len();
        }
    }
    fn from_bytes(bytes: &Vec<u8>) -> Self {
        let mut ans = Self::new();
        ans.update_from_bytes(bytes);
        return ans;
    }
    fn len(&self) -> usize {
        return 256;
    }
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
    fn write_sector(&mut self,data: &Vec<u8>,ts: [u8;2], offset: usize) {
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
        // update the VTOC
        self.allocate_sector(track,sector);
    }
    /// Create a standard DOS 3.3 small volume (140K)
    pub fn format(&mut self,vol:u8,bootable:bool) {
        // First write the Volume Table of Contents (VTOC)

        self.vtoc.pad1 = 4;
        self.vtoc.vol = vol;
        self.vtoc.last_track = 18;
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
        assert!(self.vtoc.last_track!=self.vtoc.track1);
        let tvtoc: u8 = self.vtoc.track1;
        let tstart = match self.vtoc.last_track {
            x if x>=self.vtoc.tracks => tvtoc-1,
            x if x>tvtoc && prefer_jump => x+1,
            x if x<tvtoc && prefer_jump => x-1,
            x => x
        };
        assert!(tstart!=self.vtoc.track1);
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
    fn get_next_directory_slot(&self) -> ([u8;2],u8) {
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        let mut buf = vec![0;256];
        for _try in 0..100 {
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
    fn get_tslist_sector(&self,name: &String) -> [u8;2] {
        let mut buf: Vec<u8> = vec![0;256];
        let fname = string_to_file_name(name);
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        for _try in 0..100 {
            self.panic_if_ts_bad(ts[0], ts[1]);
            self.read_sector(&mut buf, ts, 0);
            let dir = DirectorySector::from_bytes(&buf);
            for entry in dir.entries.as_ref() {
                if fname==entry.name && entry.tsl_track>0 && entry.tsl_track<255 {
                    return [entry.tsl_track,entry.tsl_sector];
                }
            }
            ts = [dir.next_track,dir.next_sector];
            if ts == [0,0] {
                return ts;
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
    fn sequential_read(&self,name: &String) -> Result<Vec<u8>,DOS33Error> {
        // resulting vector will be padded modulo 256
        let mut next_tslist = self.get_tslist_sector(name);
        if next_tslist==[0,0] {
            return Err(DOS33Error::FileNotFound);
        }
        let mut ans: Vec<u8> = Vec::new();
        let mut buf = vec![0;256];
        // loop over up to 10 track sector list sectors, if more something likely wrong
        for _try in 0..10 {
            self.read_sector(&mut buf,next_tslist,0);
            let tslist = TrackSectorList::from_bytes(&buf);
            for p in 0..self.vtoc.max_pairs as usize {
                let next = [tslist.pairs[p*2],tslist.pairs[p*2+1]];
                if next[0]==0 {
                    break;
                }
                let mut full_buf: Vec<u8> = vec![0;256];
                self.read_sector(&mut full_buf,next,0);
                ans.append(&mut full_buf);
            }
            if tslist.next_track==0 {
                return Ok(ans);
            }
            next_tslist = [tslist.next_track,tslist.next_sector];
        }
        panic!("the disk image directory seems to be damaged");
    }
    fn sequential_write(&mut self,name: &String, file_bytes: &Vec<u8>, type_code: u8) -> Result<usize,DOS33Error> {
        let named_ts = self.get_tslist_sector(name);
        if named_ts==[0,0] {
            // this is a new file
            let bytes = u16::from_le_bytes(self.vtoc.bytes) as usize;
            let data_sectors = 1 + file_bytes.len()/bytes;
            let tslist_sectors = 1 + data_sectors/self.vtoc.max_pairs as usize;
            if data_sectors + tslist_sectors > self.num_free_sectors() {
                return Err(DOS33Error::DiskFull);
            }

            // we are doing this
            let mut offset = 0;
            let mut sec_base = 0; // in units of pairs
            let mut p = 0; // pairs written in current tslist sector
            let mut tslist: Vec<TrackSectorList> = Vec::new();
            let tslist_start = self.get_next_free_sector(true);
            self.allocate_sector(tslist_start[0], tslist_start[1]); // preallocate it without writing

            // write the data while building TS list data and preallocating its spillover sectors
            tslist.push(TrackSectorList::new());
            for s in 0..data_sectors {
                let next = self.get_next_free_sector(false);
                let tssec = tslist.last_mut().unwrap();
                tssec.sector_base = u16::to_le_bytes(sec_base as u16); // redundancy here
                tssec.pairs[p*2] = next[0];
                tssec.pairs[p*2+1] = next[1];
                self.write_sector(&file_bytes,next,offset);
                p += 1;
                if p==self.vtoc.max_pairs as usize && s+1<data_sectors {
                    // tslist spilled over to another sector
                    let next_tslist_ts = self.get_next_free_sector(false);
                    self.allocate_sector(next_tslist_ts[0], next_tslist_ts[1]); // preallocate it without writing
                    tssec.next_track = next_tslist_ts[0];
                    tssec.next_sector = next_tslist_ts[1];
                    tslist.push(TrackSectorList::new());
                    sec_base += self.vtoc.max_pairs as usize;
                    p = 0;
                }
                offset += bytes;
            }
            
            // write the track sector list
            // we manipulate structures first, then write all at once
            assert!(tslist.len()==tslist_sectors);
            for s in 0..tslist_sectors {
                let ts = match s {
                    x if x==0 => tslist_start,
                    _ => [tslist[s-1].next_track,tslist[s-1].next_sector]
                };
                self.write_sector(&tslist[s].to_bytes(), ts, 0);
            }

            // write the directory entry
            let (ts,e) = self.get_next_directory_slot();
            let mut dir_buf = vec![0;256];
            self.read_sector(&mut dir_buf, ts, 0);
            let mut dir = DirectorySector::from_bytes(&dir_buf);
            dir.entries[e as usize].tsl_track = tslist_start[0];
            dir.entries[e as usize].tsl_sector = tslist_start[1];
            dir.entries[e as usize].file_type = type_code;
            dir.entries[e as usize].name = string_to_file_name(name);
            dir.entries[e as usize].sectors = [tslist_sectors as u8 + data_sectors as u8 ,0];
            self.write_sector(&dir.to_bytes(), ts, 0);

            return Ok(data_sectors + tslist_sectors);
        } else {
            panic!("file exists, overwriting is not supported");
        }
    }
    /// List all the files on disk to standard output, mirrors `CATALOG`
    pub fn catalog_to_stdout(&self) {
        let typ_map: HashMap<u8,&str> = HashMap::from([(0," T"),(1," I"),(2," A"),(4," B"),(128,"*T"),(129,"*I"),(130,"*A"),(132,"*B")]);
        let mut ts = [self.vtoc.track1,self.vtoc.sector1];
        let mut buf = vec![0;256];
        println!("DISK VOLUME {}",self.vtoc.vol);
        println!();
        for _try in 0..100 {
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
                return;
            }
        }
        panic!("the disk image directory seems to be damaged");
    }
    /// Read a binary file from the disk, mirrors `BLOAD`.  Returns the starting address and data in a tuple.
    pub fn bload(&self,name: &String) -> Result<(u16,Vec<u8>),DOS33Error> {
        match self.sequential_read(name) {
            Ok(v) => {
                let ans = types::BinaryData::from_bytes(&v);
                Ok((u16::from_le_bytes(ans.start),ans.data))
            },
            Err(e) => Err(e)
        }
    }
    /// Write a binary file to the disk, mirrors `BSAVE`
    pub fn bsave(&mut self,name: &String, dat: &Vec<u8>,start_addr: u16) -> Result<usize,DOS33Error> {
        let file = types::BinaryData::pack(&dat,start_addr);
        return self.sequential_write(name, &file.to_bytes(), Type::Binary as u8);
    }
    /// Read a BASIC program file from the disk, mirrors `LOAD`, program is in tokenized form.
    /// Detokenization is handled in a different module.
    pub fn load(&self,name: &String) -> Result<Vec<u8>,DOS33Error> {
        match self.sequential_read(name) {
            Ok(v) => Ok(types::TokenizedProgram::from_bytes(&v).program),
            Err(e) => Err(e)
        }
    }
    /// Write a BASIC program to the disk, mirrors `SAVE`, program must already be tokenized.
    /// Tokenization is handled in a different module.
    pub fn save(&mut self,name: &String, dat: &Vec<u8>, typ: Type) -> Result<usize,DOS33Error> {
        let file = types::TokenizedProgram::pack(&dat);
        return self.sequential_write(name, &file.to_bytes(), typ as u8);
    }
    /// Read sequential text file from the disk, mirrors `READ`, text remains in A2 format.  `SequentialText` can be used for conversions.
    pub fn read_text(&self,name: &String) -> Result<Vec<u8>,DOS33Error> {
        match self.sequential_read(name) {
            Ok(v) => {
                Ok(types::SequentialText::from_bytes(&v).text)
            },
            Err(e) => Err(e)
        }
    }
    /// Write sequential text file to the disk, mirrors `WRITE`, text must already be in A2 format.  `SequentialText` can be used for conversions.
    pub fn write_text(&mut self,name: &String, dat: &Vec<u8>) -> Result<usize,DOS33Error> {
        let file = types::SequentialText::pack(&dat);
        return self.sequential_write(name, &file.to_bytes(), Type::Text as u8);
    }
    /// Create a disk from a DOS ordered disk image buffer
    pub fn from_do_img(do_img: &Vec<u8>) -> Result<Self,DOS33Error> {
        let mut disk = Self::new();
        let tlen = 35 as usize;
        let slen = 16 as usize;
        let blen = 256 as usize;
        if do_img.len()!=tlen*slen*blen {
            return Err(DOS33Error::EndOfData);
        }
        for track in 0..tlen {
            for sector in 0..slen {
                for byte in 0..blen {
                    disk.tracks[track][sector][byte] = do_img[byte+sector*blen+track*slen*blen];
                }
            }
        }
        disk.vtoc = VTOC::from_bytes(&disk.tracks[17][0].to_vec());
        if disk.vtoc.bytes != [0,1] || disk.vtoc.track1 != 17 || disk.vtoc.sector1 != 15 || disk.vtoc.sectors != 16 || disk.vtoc.tracks != 35 {
            return Err(DOS33Error::VolumeMismatch);
        }
        return Ok(disk);
    }
    /// Return a DOS ordered disk image buffer of this disk
    pub fn to_do_img(&self) -> Vec<u8> {
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
