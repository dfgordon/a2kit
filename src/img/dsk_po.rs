//! # Support for ProDOS ordered disk images (PO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in ProDOS, we have a PO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use crate::disk_base;
use crate::img;
use crate::fs::ChunkSpec;

const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

/// Wrapper for PO data.
pub struct PO {
    blocks: u16,
    data: Vec<u8>
}

impl PO {
    pub fn create(blocks: u16) -> Self {
        let mut data: Vec<u8> = Vec::new();
        for _i in 0..blocks as usize {
            data.append(&mut [0;BLOCK_SIZE].to_vec());
        }
        Self {
            blocks,
            data
        }
    }
}

impl disk_base::DiskImage for PO {
    fn track_count(&self) -> usize {
        return self.blocks as usize/8;
    }
    fn byte_capacity(&self) -> usize {
        return self.data.len();
    }
    fn read_chunk(&self,addr: ChunkSpec) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        match addr {
            ChunkSpec::PO(block) => Ok(self.data[block*BLOCK_SIZE..(block+1)*BLOCK_SIZE].to_vec()),
            ChunkSpec::DO([t,s]) => {
                let (block,offset) = crate::fs::block_from_ts(t, s);
                let beg = block*BLOCK_SIZE + offset;
                Ok(self.data[beg..beg+256].to_vec())
            }
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_chunk(&mut self, addr: ChunkSpec, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        match addr {
            ChunkSpec::PO(block) => {
                for i in 0..dat.len() {
                    self.data[block*BLOCK_SIZE+i] = dat[i];
                }
                Ok(())
            },
            ChunkSpec::DO([t,s]) => {
                let (block,offset) = crate::fs::block_from_ts(t, s);
                let beg = block*BLOCK_SIZE + offset;
                for i in 0..dat.len() {
                    self.data[beg+i] = dat[i];
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
        Some(Self {
            blocks: (data.len()/BLOCK_SIZE) as u16,
            data: data.clone()
        })
    }
    fn what_am_i(&self) -> disk_base::DiskImageType {
        disk_base::DiskImageType::PO
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