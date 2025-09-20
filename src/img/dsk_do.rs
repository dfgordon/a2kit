//! ## Support for Apple DOS ordered disk images (DO,DSK)
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! If the sector sequence is ordered as in DOS 3.3, we have a DO variant.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use a2kit_macro::DiskStructError;
use log::{trace,error};
use crate::img;
use crate::bios::{skew,Block};
use crate::bios::blocks::apple;
use crate::{STDRESULT,DYNERR};

const CPM_RECORD: usize = 128;
const SECTOR_SIZE: usize = 256;
const BLOCK_SIZE: usize = 512;
const MAX_BLOCKS: usize = 65535;
const MIN_BLOCKS: usize = 280;

pub fn file_extensions() -> Vec<String> {
    vec!["do".to_string(),"dsk".to_string()]
}

/// Wrapper for DO data.
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
    fn end_track(&self) -> usize {
        return self.tracks as usize;
    }
    fn num_heads(&self) -> usize {
        1
    }
    fn nominal_capacity(&self) -> Option<usize> {
        Some(self.data.len())
    }
    fn actual_capacity(&mut self) -> Result<usize,DYNERR> {
        Ok(self.data.len())
    }
    fn read_block(&mut self,addr: Block) -> Result<Vec<u8>,DYNERR> {
        trace!("read {}",addr);
        match addr {
            Block::D13(_) => Err(Box::new(img::Error::ImageTypeMismatch)),
            Block::DO([t,s]) => {
                let mut ans: Vec<u8> = Vec::new();
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                ans.append(&mut self.data[offset..offset+SECTOR_SIZE].to_vec());
                Ok(ans) 
            },
            Block::PO(block) => {
                let mut ans: Vec<u8> = Vec::new();
                let ts_list = apple::ts_from_prodos_block(block,&self.kind)?;
                for [t,s] in ts_list {
                    let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                    ans.append(&mut self.data[offset..offset+SECTOR_SIZE].to_vec());    
                }
                Ok(ans) 
            },
            Block::CPM((_block,_bsh,_off)) => {
                let mut ans: Vec<u8> = Vec::new();
                let ts_list = addr.get_lsecs(32);
                for ts in ts_list {
                    trace!("track {} lsec {}",ts[0],ts[1]);
                    let track = ts[0];
                    let dsec = skew::CPM_LSEC_TO_DOS_LSEC[ts[1]-1];
                    let offset = track*self.sectors as usize*SECTOR_SIZE + dsec*SECTOR_SIZE + skew::CPM_LSEC_TO_DOS_OFFSET[ts[1]-1];
                    ans.append(&mut self.data[offset..offset+CPM_RECORD].to_vec());
                }
                Ok(ans)
            },
            Block::FAT(_) => Err(Box::new(super::Error::ImageTypeMismatch)),
            Block::D64(_) => Err(Box::new(super::Error::ImageTypeMismatch)),
        }
    }
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT {
        trace!("write {}",addr);
        match addr {
            Block::D13(_) => Err(Box::new(img::Error::ImageTypeMismatch)),
            Block::DO([t,s]) => {
                let padded = super::quantize_block(dat, SECTOR_SIZE);
                let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded);
                Ok(())
            },
            Block::PO(block) => {
                let padded = super::quantize_block(dat, BLOCK_SIZE);
                let ts_list = apple::ts_from_prodos_block(block,&self.kind)?;
                let mut src_offset = 0;
                for [t,s] in ts_list {
                    let offset = t*self.sectors as usize*SECTOR_SIZE + s*SECTOR_SIZE;
                    self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded[src_offset..src_offset+SECTOR_SIZE]);
                    src_offset += SECTOR_SIZE;
                }
                Ok(())
            },
            Block::CPM((_block,bsh,_off)) => {
                let padded = super::quantize_block(dat, CPM_RECORD << bsh);
                let ts_list = addr.get_lsecs(32);
                let mut src_offset = 0;
                for ts in ts_list {
                    trace!("track {} lsec {}",ts[0],ts[1]);
                    let track = ts[0];
                    let dsec = skew::CPM_LSEC_TO_DOS_LSEC[ts[1]-1];
                    let offset = track*self.sectors as usize*SECTOR_SIZE + dsec*SECTOR_SIZE + skew::CPM_LSEC_TO_DOS_OFFSET[ts[1]-1];
                    self.data[offset..offset+CPM_RECORD].copy_from_slice(&padded[src_offset..src_offset+CPM_RECORD]);
                    src_offset += CPM_RECORD;
                }
                Ok(())
            },
            Block::FAT(_) => Err(Box::new(super::Error::ImageTypeMismatch)),
            Block::D64(_) => Err(Box::new(super::Error::ImageTypeMismatch)),
        }
    }
    fn read_sector(&mut self,trk: super::Track,sec: super::Sector) -> Result<Vec<u8>,DYNERR> {
        let [cyl,head,sec] = self.get_rzq(trk,sec)?;
        if cyl>=self.end_track() || head>0 || sec>=self.sectors as usize {
            error!("exceeded bounds: maxima are cyl {}, head {}, sector {}",self.end_track()-1,0,self.sectors-1);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (cyl*self.sectors as usize + skew::DOS_PSEC_TO_DOS_LSEC[sec])*SECTOR_SIZE;
        Ok(self.data[offset..offset+SECTOR_SIZE].to_vec())
    }
    fn write_sector(&mut self,trk: super::Track,sec: super::Sector,dat: &[u8]) -> STDRESULT {
        let [cyl,head,sec] = self.get_rzq(trk,sec)?;
        if cyl>=self.end_track() || head>0 || sec>=self.sectors as usize {
            error!("exceeded bounds: maxima are cyl {}, head {}, sector {}",self.end_track()-1,0,self.sectors-1);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (cyl*self.sectors as usize + skew::DOS_PSEC_TO_DOS_LSEC[sec])*SECTOR_SIZE;
        let padded = super::quantize_block(dat, SECTOR_SIZE);
        self.data[offset..offset+SECTOR_SIZE].copy_from_slice(&padded);
        Ok(())
    }
    fn from_bytes(data: &[u8]) -> Result<Self,DiskStructError> {
        // reject anything that can be neither a DOS 3.3 nor a ProDOS volume
        if data.len()%BLOCK_SIZE > 0 || data.len()/BLOCK_SIZE > MAX_BLOCKS || data.len()/BLOCK_SIZE < MIN_BLOCKS {
            return Err(DiskStructError::UnexpectedSize);
        }
        // further demand integral number of tracks
        if (data.len()/BLOCK_SIZE)%8 >0 {
            return Err(DiskStructError::UnexpectedSize);
        }
        let tracks = (data.len()/BLOCK_SIZE/8) as u16;
        Ok(Self {
            kind: match tracks {
                35 => img::names::A2_DOS33_KIND,
                _ => img::DiskKind::Unknown
            },
            tracks,
            sectors: 16,
            data: data.to_vec()
        })
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::DO
    }
    fn file_extensions(&self) -> Vec<String> {
        file_extensions()
    }
    fn kind(&self) -> img::DiskKind {
        self.kind
    }
    fn change_kind(&mut self,kind: img::DiskKind) {
        self.kind = kind;
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        return self.data.clone();
    }
    fn get_track_buf(&mut self,_trk: super::Track) -> Result<Vec<u8>,DYNERR> {
        error!("DO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn set_track_buf(&mut self,_trk: super::Track,_dat: &[u8]) -> STDRESULT {
        error!("DO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_solution(&mut self,trk: super::Track) -> Result<img::TrackSolution,DYNERR> {        
        let [c,_] = self.get_rz(trk)?;
        let mut addr_map: Vec<[u8;6]> = Vec::new();
        for i in 0..16 {
            let mut addr = [254,c.try_into()?,i,0,0,0];
            addr[3] = addr[0] ^ addr[1] ^ addr[2];
            addr_map.push(addr);
        }
        return Ok(img::TrackSolution::Solved(img::SolvedTrack {
            speed_kbps: 250,
            density: None,
            flux_code: img::FluxCode::GCR,
            addr_code: img::FieldCode::WOZ((4,4)),
            data_code: img::FieldCode::WOZ((6,2)),
            addr_type: "VTSK".to_string(),
            addr_mask: [0,0,255,0,0,0],
            addr_map,
            size_map: vec![256;16]
        }));
    }
    fn get_track_nibbles(&mut self,_trk: super::Track) -> Result<Vec<u8>,DYNERR> {
        error!("DO images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &[u8]) -> String {
        String::from("DO images have no track bits to display")
    }
}