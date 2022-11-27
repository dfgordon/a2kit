//! # Support for 13 sector disk images (D13,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If there are 13 sectors in physical order, we have a D13 variant.
//! For D13 we refuse any alternative orderings.

use crate::disk_base;
use crate::img;

const TRACK_SIZE: usize = 13*256;
const MIN_TRACKS: usize = 35;

/// Wrapper for D13 data
pub struct D13 {
    data: Vec<u8>
}

impl disk_base::DiskImage for D13 {
    fn from_bytes(data: &Vec<u8>) -> Option<Self> {
        // reject anything that can be neither a DOS 3.3 nor a ProDOS volume
        if data.len()%TRACK_SIZE > 0 || data.len()/TRACK_SIZE < MIN_TRACKS {
            return None;
        }
        Some(Self {
            data: data.clone()
        })
    }
    fn is_do_or_po(&self) -> bool {
        false
    }
    fn update_from_d13(&mut self,dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if self.data.len()!=dsk.len() {
            return Err(Box::new(img::Error::ImageSizeMismatch));
        }
        self.data = dsk.clone();
        return Ok(());
    }
    fn update_from_do(&mut self,_dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn update_from_po(&mut self,_dsk: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn to_d13(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Ok(self.data.clone());
    }
    fn to_do(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn to_po(&self) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        return Err(Box::new(img::Error::ImageTypeMismatch));
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