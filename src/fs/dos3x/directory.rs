
//! # DOS 3.3 directory structures
//! These are fixed length structures, with the DiskStruct trait.

// a2kit_macro automatically derives `new`, `to_bytes`, `from_bytes`, and `length` from a DiskStruct.
// This spares us having to manually write code to copy bytes in and out for every new structure.
// The auto-derivation is not used for structures with variable length fields (yet).
// For fixed length structures, update_from_bytes will panic if lengths do not match.
use a2kit_macro::DiskStruct;
use a2kit_macro_derive::DiskStruct;

// Following are representations of disk directory structures
// these are mostly fixed length structures where the DiskStruct
// trait can be automatically derived.

// Note on large volumes:
// We can extend VTOC.bitmap to 200 bytes, allowing for VTOC.tracks = 50.
// We can extend VTOC.sectors to 32, because the bitmap allocates 32 bits per track.
// This gives 50*32*256 = 409600, i.e., a 400K disk.
// Large DOS volumes were supported on 800K floppies and hard drives by a few third parties.

#[derive(DiskStruct)]
pub struct VTOC {
    pub pad1: u8,
    pub track1: u8,
    pub sector1: u8,
    pub version: u8,
    pub pad2: [u8;2],
    pub vol: u8,
    pub pad3: [u8;32],
    pub max_pairs: u8,
    pub pad4: [u8;8],
    pub last_track: u8,
    pub last_direction: u8,
    pub pad5: [u8;2],
    pub tracks: u8,
    pub sectors: u8,
    pub bytes: [u8;2],
    pub bitmap: [u8;140]
}

#[derive(DiskStruct)]
pub struct TrackSectorList {
    pub pad1: u8,
    pub next_track: u8,
    pub next_sector: u8,
    pub pad2: [u8;2],
    pub sector_base: [u8;2],
    pub pad3: [u8;5],
    pub pairs: [u8;244]
}

#[derive(DiskStruct)]
pub struct DirectoryEntry {
    pub tsl_track: u8,
    pub tsl_sector: u8,
    pub file_type: u8,
    pub name: [u8;30],
    pub sectors: [u8;2]
}

pub struct DirectorySector {
    pub pad1: u8,
    pub next_track: u8,
    pub next_sector: u8,
    pub pad2: [u8;8],
    pub entries: [DirectoryEntry;7]
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
