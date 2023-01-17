//! ## Support for ProDOS ordered disk images (PO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in ProDOS, we have a PO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use crate::img;
use crate::fs::Chunk;

use log::error;
use super::BlockLayout;

const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

fn select_kind(blocks: u16) -> img::DiskKind {
    match blocks {
        280 => img::names::A2_DOS33_KIND,
        _ => img::DiskKind::LogicalBlocks(BlockLayout {block_count: blocks as usize, block_size: BLOCK_SIZE})
    }
}

/// Wrapper for PO data.
pub struct PO {
    kind: img::DiskKind,
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
            kind: select_kind(blocks),
            blocks,
            data
        }
    }
}

impl img::DiskImage for PO {
    fn track_count(&self) -> usize {
        return self.blocks as usize/8;
    }
    fn byte_capacity(&self) -> usize {
        return self.data.len();
    }
    fn read_chunk(&self,addr: Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        match addr {
            Chunk::PO(block) => Ok(self.data[block*BLOCK_SIZE..(block+1)*BLOCK_SIZE].to_vec()),
            _ => Err(Box::new(img::Error::ImageTypeMismatch)),
        }
    }
    fn write_chunk(&mut self, addr: Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        match addr {
            Chunk::PO(block) => {
                let padded = super::quantize_chunk(dat, BLOCK_SIZE);
                self.data[block*BLOCK_SIZE..(block+1)*BLOCK_SIZE].copy_from_slice(&padded);
                Ok(())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch)),
        }
    }
    fn read_sector(&self,_cyl: usize,_head: usize,_sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("logical disk cannot access sectors");
        Err(Box::new(img::Error::ImageTypeMismatch))
    }
    fn write_sector(&mut self,_cyl: usize,_head: usize,_sec: usize,_dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        error!("logical disk cannot access sectors");
        Err(Box::new(img::Error::ImageTypeMismatch))
    }
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        // reject anything that can be neither a DOS 3.3 nor a ProDOS volume
        if data.len()%BLOCK_SIZE > 0 || data.len()/BLOCK_SIZE > MAX_BLOCKS || data.len()/BLOCK_SIZE < MIN_BLOCKS {
            return None;
        }
        let blocks = (data.len()/BLOCK_SIZE) as u16;
        Some(Self {
            kind: select_kind(blocks),
            blocks,
            data: data.clone()
        })
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::PO
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn to_bytes(&self) -> Vec<u8> {
        return self.data.clone();
    }
    fn get_track_buf(&self,_cyl: usize,_head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("PO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_nibbles(&self,_cyl: usize,_head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("PO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &Vec<u8>) -> String {
        String::from("PO images have no track bits to display")
    }
}