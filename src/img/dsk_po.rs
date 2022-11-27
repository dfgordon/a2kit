//! # Support for ProDOS ordered disk images (PO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in ProDOS, we have a PO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.
//! Since the file system layer works directly with either DO or PO images,
//! all this module has to do is reordering and verifications.

use crate::disk_base;
use crate::img;

const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

/// Wrapper for PO data.
pub struct PO {
    blocks: u16,
    data: Vec<u8>
}

impl disk_base::DiskImage for PO {
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
    fn is_do_or_po(&self) -> bool {
        true
    }
    fn update_from_d13(&mut self,_dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn update_from_do(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if self.data.len()!=dsk.len() || self.blocks%8>0 {
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        return self.update_from_po(&img::reorder_do_to_po(&dsk, 16));
    }
    fn update_from_po(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if self.data.len()!=dsk.len() {
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        self.data = dsk.clone();
        return Ok(());
    }
    fn to_d13(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn to_do(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        if self.data.len()!=self.blocks as usize * BLOCK_SIZE || self.blocks%8>0  {
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        return Ok(img::reorder_po_to_do(&self.data, 16));
    }
    fn to_po(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Ok(self.data.clone());
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