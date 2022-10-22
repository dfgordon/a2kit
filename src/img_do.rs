//! # Support for DOS ordered disk images (DO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in DOS 3.3, we have a DO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.
//! Since the file system layer works directly with either DO or PO images,
//! all this module has to do is reordering and verifications.

use crate::disk_base;
use crate::disk525;

const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

/// Although this is DOS 3.3 ordered, we allow an extended (and abstract) mapping
/// from ProDOS blocks to 16 bit track indices.  As a result even a 32MB
/// ProDOS volume can be mapped into DOS 3.3 ordering.
pub struct DO {
    tracks: u16,
    sectors: u16,
    data: Vec<u8>
}

impl disk_base::DiskImage for DO {
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
    fn is_do_or_po(&self) -> bool {
        true
    }
    fn update_from_do(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if self.data.len()!=dsk.len() {
            return Err(Box::new(disk_base::CommandError::UnknownFormat));
        }
        self.data = dsk.clone();
        return Ok(());
    }
    fn update_from_po(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if self.data.len()!=dsk.len() {
            return Err(Box::new(disk_base::CommandError::UnknownFormat));
        }
        return self.update_from_do(&disk525::reorder_po_to_do(dsk, self.sectors as usize));
    }
    fn to_do(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Ok(self.data.clone());
    }
    fn to_po(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Ok(disk525::reorder_do_to_po(&self.data, self.sectors as usize));
    }
    fn to_bytes(&self) -> Vec<u8> {
        return self.data.clone();
    }
    fn get_track_buf(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        return Err(Box::new(disk_base::CommandError::UnsupportedFormat));
    }
    fn get_track_bytes(&self,track: &str) -> Result<(u16,Vec<u8>),Box<dyn std::error::Error>> {
        return Err(Box::new(disk_base::CommandError::UnsupportedFormat));        
    }
}