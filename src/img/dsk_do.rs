//! # Support for DOS ordered disk images (DO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in DOS 3.3, we have a DO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use log::trace;
use crate::disk_base;
use crate::img;
use crate::fs::ChunkSpec;

const SECTOR_SIZE: usize = 256;
const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

/// Wrapper for DO data.
/// Although this is DOS 3.3 ordered, we allow an extended (and abstract) mapping
/// from ProDOS blocks to 16 bit track indices.  As a result even a 32MB
/// ProDOS volume can be mapped into DOS 3.3 ordering.
pub struct DO {
    tracks: u16,
    sectors: u16,
    data: Vec<u8>
}

impl DO {
    pub fn create(tracks: u16,sectors: u16) -> Self {
        let mut data: Vec<u8> = Vec::new();
        for _i in 0..tracks as usize*sectors as usize {
            data.append(&mut [0;SECTOR_SIZE].to_vec());
        }
        Self {
            tracks,
            sectors,
            data
        }
    }
}

impl disk_base::DiskImage for DO {
    fn track_count(&self) -> usize {
        return self.tracks as usize;
    }
    fn byte_capacity(&self) -> usize {
        return self.data.len();
    }
    fn read_chunk(&self,addr: ChunkSpec) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        match addr {
            ChunkSpec::DO([t,s]) => {
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                Ok(self.data[offset..offset+SECTOR_SIZE].to_vec())
            },
            ChunkSpec::PO(block) => {
                let ts = crate::fs::ts_from_block(block);
                let offset = ts[0][0]*self.sectors as usize*SECTOR_SIZE + ts[0][1]*SECTOR_SIZE;
                let sec1 = self.data[offset..offset+SECTOR_SIZE].to_vec();
                let offset = ts[1][0]*self.sectors as usize*SECTOR_SIZE + ts[1][1]*SECTOR_SIZE;
                let sec2 = self.data[offset..offset+SECTOR_SIZE].to_vec();
                Ok([sec1,sec2].concat())
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_chunk(&mut self, addr: ChunkSpec, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        match addr {
            ChunkSpec::DO([t,s]) => {
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                for i in 0..dat.len() {
                    self.data[offset+i] = dat[i];
                }
                Ok(())
            },
            ChunkSpec::PO(block) => {
                let ts = crate::fs::ts_from_block(block);
                trace!("block write to ts {},{} and {},{}",ts[0][0],ts[0][1],ts[1][0],ts[1][1]);
                let end = match dat.len() {
                    x if x<SECTOR_SIZE => x,
                    _ => SECTOR_SIZE
                };
                let offset = ts[0][0]*self.sectors as usize*SECTOR_SIZE + ts[0][1]*SECTOR_SIZE;
                for i in 0..end {
                    self.data[offset+i] = dat[i];
                }
                if dat.len()>SECTOR_SIZE {
                    let offset = ts[1][0]*self.sectors as usize*SECTOR_SIZE + ts[1][1]*SECTOR_SIZE;
                    for i in SECTOR_SIZE..dat.len() {
                        self.data[offset+i-SECTOR_SIZE] = dat[i];
                    }
                }
                Ok(())
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        // reject anything that can be neither a DOS 3.3 nor a ProDOS volume
        if data.len()%BLOCK_SIZE > 0 || data.len()/BLOCK_SIZE > MAX_BLOCKS || data.len()/BLOCK_SIZE < MIN_BLOCKS {
            return None;
        }
        // further demand integral number of tracks
        if (data.len()/BLOCK_SIZE)%8 >0 {
            return None;
        }
        Some(Self {
            tracks: (data.len()/BLOCK_SIZE/8) as u16,
            sectors: 16,
            data: data.clone()
        })
    }
    fn what_am_i(&self) -> disk_base::DiskImageType {
        disk_base::DiskImageType::DO
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