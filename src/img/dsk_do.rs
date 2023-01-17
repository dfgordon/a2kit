//! ## Support for DOS ordered disk images (DO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in DOS 3.3, we have a DO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use log::{trace,error};
use crate::img;
use crate::fs::Chunk;
use crate::bios::skew;

const CPM_RECORD: usize = 128;
const SECTOR_SIZE: usize = 256;
const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

/// Wrapper for DO data.
/// Although this is DOS 3.3 ordered, we allow an extended (and abstract) mapping
/// from ProDOS blocks to 16 bit track indices.  As a result even a 32MB
/// ProDOS volume can be mapped into DOS 3.3 ordering.
pub struct DO {
    kind: img::DiskKind,
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
            kind: match (tracks,sectors) {
                (35,13) => panic!("DO refusing to create a D13"),
                (35,16) => img::names::A2_DOS33_KIND,
                _ => img::DiskKind::Unknown
            },
            tracks,
            sectors,
            data
        }
    }
}

impl img::DiskImage for DO {
    fn track_count(&self) -> usize {
        return self.tracks as usize;
    }
    fn byte_capacity(&self) -> usize {
        return self.data.len();
    }
    fn read_chunk(&self,addr: Chunk) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        trace!("reading {}",addr);
        match addr {
            Chunk::D13(_) => Err(Box::new(img::Error::ImageTypeMismatch)),
            Chunk::DO([t,s]) => {
                let mut ans: Vec<u8> = Vec::new();
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                ans.append(&mut self.data[offset..offset+SECTOR_SIZE].to_vec());
                Ok(ans) 
            },
            Chunk::PO(block) => {
                let mut ans: Vec<u8> = Vec::new();
                let ts_list = skew::ts_from_prodos_block(block,&self.kind);
                for [t,s] in ts_list {
                    let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                    ans.append(&mut self.data[offset..offset+SECTOR_SIZE].to_vec());    
                }
                Ok(ans) 
            },
            Chunk::CPM((_block,_bsh,_off)) => {
                let mut ans: Vec<u8> = Vec::new();
                let ts_list = addr.get_lsecs(32);
                for ts in ts_list {
                    trace!("track {} lsec {}",ts[0],ts[1]);
                    let track = ts[0];
                    let dsec = skew::CPM_LSEC_TO_DOS_LSEC[ts[1]];
                    let offset = track*self.sectors as usize*SECTOR_SIZE + dsec*SECTOR_SIZE + skew::CPM_LSEC_TO_DOS_OFFSET[ts[1]];
                    ans.append(&mut self.data[offset..offset+CPM_RECORD].to_vec());
                }
                Ok(ans)
            }
        }
    }
    fn write_chunk(&mut self, addr: Chunk, dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        trace!("writing {}",addr);
        match addr {
            Chunk::D13(_) => Err(Box::new(img::Error::ImageTypeMismatch)),
            Chunk::DO([t,s]) => {
                let padded = super::quantize_chunk(dat, SECTOR_SIZE);
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded);
                Ok(())
            },
            Chunk::PO(block) => {
                let padded = super::quantize_chunk(dat, BLOCK_SIZE);
                let ts_list = skew::ts_from_prodos_block(block,&self.kind);
                let mut src_offset = 0;
                for [t,s] in ts_list {
                    let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                    self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded[src_offset..src_offset+SECTOR_SIZE]);
                    src_offset += SECTOR_SIZE;
                }
                Ok(())
            },
            Chunk::CPM((_block,bsh,_off)) => {
                let padded = super::quantize_chunk(dat, CPM_RECORD << bsh);
                let ts_list = addr.get_lsecs(32);
                let mut src_offset = 0;
                for ts in ts_list {
                    trace!("track {} lsec {}",ts[0],ts[1]);
                    let track = ts[0];
                    let dsec = skew::CPM_LSEC_TO_DOS_LSEC[ts[1]];
                    let offset = track*self.sectors as usize*SECTOR_SIZE + dsec*SECTOR_SIZE + skew::CPM_LSEC_TO_DOS_OFFSET[ts[1]];
                    self.data[offset..offset+CPM_RECORD].copy_from_slice(&padded[src_offset..src_offset+CPM_RECORD]);
                    src_offset += CPM_RECORD;
                }
                Ok(())
            }
        }
    }
    fn read_sector(&self,cyl: usize,head: usize,sec: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        if cyl>=self.track_count() || head>0 || sec>=self.sectors as usize {
            error!("exceeded bounds: maxima are cyl {}, head {}, sector {}",self.track_count()-1,0,self.sectors-1);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (cyl*self.sectors as usize + skew::DOS_PSEC_TO_DOS_LSEC[sec])*SECTOR_SIZE;
        Ok(self.data[offset..offset+SECTOR_SIZE].to_vec())
    }
    fn write_sector(&mut self,cyl: usize,head: usize,sec: usize,dat: &Vec<u8>) -> Result<(),Box<dyn std::error::Error>> {
        if cyl>=self.track_count() || head>0 || sec>=self.sectors as usize {
            error!("exceeded bounds: maxima are cyl {}, head {}, sector {}",self.track_count()-1,0,self.sectors-1);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (cyl*self.sectors as usize + skew::DOS_PSEC_TO_DOS_LSEC[sec])*SECTOR_SIZE;
        let padded = super::quantize_chunk(dat, SECTOR_SIZE);
        self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded);
        Ok(())
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
        let tracks = (data.len()/BLOCK_SIZE/8) as u16;
        Some(Self {
            kind: match tracks {
                35 => img::names::A2_DOS33_KIND,
                _ => img::DiskKind::Unknown
            },
            tracks,
            sectors: 16,
            data: data.clone()
        })
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::DO
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
        error!("DO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_nibbles(&self,_cyl: usize,_head: usize) -> Result<Vec<u8>,Box<dyn std::error::Error>> {
        error!("DO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &Vec<u8>) -> String {
        String::from("DO images have no track bits to display")
    }
}