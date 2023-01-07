//! ## Support for 13 sector disk images (D13,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If there are 13 sectors in physical order, we have a D13 variant.
//! For D13 we refuse any alternative orderings.

use crate::img;
use crate::fs::Chunk;
use log::debug;

const SECTOR_SIZE: usize = 256;
const TRACK_SIZE: usize = 13*SECTOR_SIZE;
const MIN_TRACKS: usize = 35;

/// Wrapper for D13 data
pub struct D13 {
    tracks: u16,
    data: Vec<u8>
}

impl D13 {
    pub fn create(tracks: u16) -> Self {
        let mut data: Vec<u8> = Vec::new();
        for _i in 0..tracks as usize*13 {
            data.append(&mut [0;SECTOR_SIZE].to_vec());
        }
        Self {
            tracks,
            data
        }
    }
}

impl img::DiskImage for D13 {
    fn track_count(&self) -> usize {
        return self.tracks as usize;
    }
    fn byte_capacity(&self) -> usize {
        return self.data.len();
    }
    fn read_chunk(&self,addr: Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        match addr {
            Chunk::D13([t,s]) => {
                let offset = t*TRACK_SIZE + s*SECTOR_SIZE;
                Ok(self.data[offset..offset+SECTOR_SIZE].to_vec())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_chunk(&mut self, addr: Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        match addr {
            Chunk::D13([t,s]) => {
                let offset = t*TRACK_SIZE + s*SECTOR_SIZE;
                let padded = super::quantize_chunk(dat, SECTOR_SIZE);
                self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded);
                Ok(())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        // reject anything that cannot be a DOS 3.2 volume
        if data.len()%TRACK_SIZE > 0 || data.len()/TRACK_SIZE < MIN_TRACKS {
            return None;
        }
        Some(Self {
            tracks: (data.len()/TRACK_SIZE) as u16,
            data: data.clone()
        })
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::D13
    }
    fn kind(&self) -> img::DiskKind {
        img::names::A2_DOS32_KIND
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        debug!("ignoring change of D13 to {}",kind);
    }
    fn to_bytes(&self) -> Vec<u8> {
        return self.data.clone();
    }
    fn get_track_buf(&self,_track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_bytes(&self,_track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
}