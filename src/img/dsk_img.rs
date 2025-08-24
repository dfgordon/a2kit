//! ## Support for IBM sector dumps
//! 
//! DSK images are a simple sequential dump of the already-decoded sector data.
//! Alternative extensions for IBM disks include IMG, IMA, and others.
//! N.b. the ordering cannot be verified until we get up to the file system layer.

use a2kit_macro::DiskStructError;
use log::{trace,debug,error};
use crate::img;
use crate::bios::Block;
use crate::bios::blocks;
use crate::{STDRESULT,DYNERR};
use super::names::*;

pub fn file_extensions() -> Vec<String> {
    vec!["img".to_string(),"ima".to_string(),"dsk".to_string()]
}

/// Wrapper for IMG data.
pub struct Img {
    kind: img::DiskKind,
    sec_size: usize,
    cylinders: usize,
    heads: usize,
    sectors: usize,
    data: Vec<u8>
}

impl Img {
    pub fn create(kind: img::DiskKind) -> Self {
        let layout = match kind {
            img::DiskKind::D35(layout) => layout,
            img::DiskKind::D525(layout) => layout,
            _ => panic!("unsupported track layout")
        };
        if layout.zones() > 1 {
            panic!("layout has multiple zones");
        }
        let sec_size = layout.sector_size[0];
        let cylinders = layout.cylinders[0];
        let heads = layout.sides();
        let sectors = layout.sectors[0];
        let img_size = layout.byte_capacity();
        Self {
            kind,
            sec_size,
            cylinders,
            heads,
            sectors,
            data: vec![0;img_size]
        }
    }
}

impl img::DiskImage for Img {
    fn track_count(&self) -> usize {
        return self.cylinders * self.heads;
    }
    fn end_track(&self) -> usize {
        return self.cylinders * self.heads;
    }
    fn num_heads(&self) -> usize {
        return self.heads;
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
            Block::FAT((_sec1,_secs)) => {
                let secs_per_track = self.sectors;
                let mut ans: Vec<u8> = Vec::new();
                let deblocked_ts_list = addr.get_lsecs(secs_per_track as usize);
                let chs_list = blocks::fat::std_blocking(deblocked_ts_list,self.heads)?;
                for (trk,lsec) in chs_list {
                    match self.read_sector(trk,lsec) {
                        Ok(mut slice) => {
                            ans.append(&mut slice);
                        },
                        Err(e) => return Err(e)
                    }
                }
                Ok(ans)
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn write_block(&mut self, addr: Block, dat: &[u8]) -> STDRESULT {
        trace!("write {}",addr);
        match addr {
            Block::FAT((_sec1,_secs)) => {
                let secs_per_track = self.sectors;
                let sec_size = self.sec_size;
                let deblocked_ts_list = addr.get_lsecs(secs_per_track as usize);
                let chs_list = blocks::fat::std_blocking(deblocked_ts_list,self.heads)?;
                let mut src_offset = 0;
                let padded = super::quantize_block(dat, chs_list.len()*sec_size);
                for (trk,lsec) in chs_list {
                    match self.write_sector(trk,lsec,&padded[src_offset..src_offset+sec_size].to_vec()) {
                        Ok(_) => src_offset += sec_size,
                        Err(e) => return Err(e)
                    }
                }
                Ok(())
            },
            _ => Err(Box::new(img::Error::ImageTypeMismatch))
        }
    }
    fn read_sector(&mut self,trk: super::Track,sec: super::Sector) -> Result<Vec<u8>,DYNERR> {
        let track = self.get_track(trk.clone())?;
        let [cyl,head,sec] = self.get_rzq(trk,sec)?;
        trace!("reading {}/{}/{}",cyl,head,sec);
        if track>=self.end_track() || sec<1 || sec>self.sectors as usize {
            error!("track/sector range should be 0-{}/1-{}",self.end_track()-1,self.sectors);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (track*self.sectors as usize + sec - 1)*self.sec_size;
        Ok(self.data[offset..offset+self.sec_size].to_vec())
    }
    fn write_sector(&mut self,trk: super::Track,sec: super::Sector,dat: &[u8]) -> STDRESULT {
        let track = self.get_track(trk.clone())?;
        let [cyl,head,sec] = self.get_rzq(trk,sec)?;
        trace!("writing {}/{}/{}",cyl,head,sec);
        if track>=self.end_track() || sec<1 || sec>self.sectors as usize {
            error!("track/sector range should be 0-{}/1-{}",self.end_track()-1,self.sectors);
            return Err(Box::new(img::Error::SectorAccess));
        }
        let offset = (track*self.sectors as usize + sec - 1)*self.sec_size;
        let padded = super::quantize_block(dat, self.sec_size);
        self.data[offset..offset+self.sec_size].copy_from_slice(&padded);
        Ok(())
    }
    fn from_bytes(data: &[u8]) -> Result<Self,DiskStructError> {
        // try to match known sizes
        let kind = match data.len() {
            l if l==CPM_1.byte_capacity() => img::DiskKind::D8(CPM_1),
            l if l==DSDD_77.byte_capacity() => img::DiskKind::D8(DSDD_77),
            l if l==IBM_SSDD_8.byte_capacity() => img::DiskKind::D525(IBM_SSDD_8),
            l if l==IBM_SSDD_9.byte_capacity() => img::DiskKind::D525(IBM_SSDD_9),
            l if l==IBM_DSDD_8.byte_capacity() => img::DiskKind::D525(IBM_DSDD_8),
            l if l==IBM_DSDD_9.byte_capacity() => img::DiskKind::D525(IBM_DSDD_9),
            l if l==IBM_SSQD.byte_capacity() => img::DiskKind::D525(IBM_SSQD),
            l if l==IBM_DSQD.byte_capacity() => img::DiskKind::D525(IBM_DSQD),
            l if l==IBM_DSHD.byte_capacity() => img::DiskKind::D525(IBM_DSHD),
            l if l==IBM_720.byte_capacity() => img::DiskKind::D35(IBM_720),
            l if l==IBM_1440.byte_capacity() => img::DiskKind::D35(IBM_1440),
            l if l==IBM_1680.byte_capacity() => img::DiskKind::D35(IBM_1680),
            l if l==IBM_1720.byte_capacity() => img::DiskKind::D35(IBM_1720),
            l if l==IBM_2880.byte_capacity() => img::DiskKind::D35(IBM_2880),
            _ => return Err(DiskStructError::IllegalValue)
        };
        let layout = match kind {
            img::DiskKind::D35(l) => l,
            img::DiskKind::D525(l) => l,
            img::DiskKind::D8(l) => l,
            _ => return Err(DiskStructError::UnexpectedValue)
        };
        debug!("IMG size matches {}",kind);
        let sec_size = layout.sector_size[0];
        let cylinders = layout.cylinders[0];
        let heads = layout.sides();
        let sectors = layout.sectors[0];
        Ok(Self {
            kind,
            sec_size,
            cylinders,
            heads,
            sectors,
            data: data.to_vec()
        })
    }
    fn what_am_i(&self) -> img::DiskImageType {
        img::DiskImageType::IMG
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
        error!("IMG images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn set_track_buf(&mut self,_trk: super::Track,_dat: &[u8]) -> STDRESULT {
        error!("IMG images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));
    }
    fn get_track_solution(&mut self,trk: super::Track) -> Result<img::TrackSolution,DYNERR> {        
        let [cylinder,head] = self.get_rz(trk)?;
        let mut addr_map: Vec<[u8;5]> = Vec::new();
        for i in 0..self.sectors {
            addr_map.push([cylinder.try_into()?,head.try_into()?,i.try_into()?,super::highest_bit(self.sec_size >> 8),0]);
        }
        let (flux_code,speed_kbps) = match self.kind {
            img::DiskKind::D35(l) => (l.flux_code[0],l.speed_kbps[0]),
            img::DiskKind::D525(l) => (l.flux_code[0],l.speed_kbps[0]),
            img::DiskKind::D8(l) => (l.flux_code[0],l.speed_kbps[0]),
            _ => (img::FluxCode::None,0)
        };
        return Ok(img::TrackSolution {
            cylinder,
            fraction: [0,1],
            head,
            speed_kbps,
            flux_code,
            addr_code: img::FieldCode::None,
            data_code: img::FieldCode::None,
            addr_type: "CHSFK".to_string(),
            addr_mask: [0,0,255,0,0],
            addr_map,
            size_map: vec![self.sec_size;self.sectors]
        });
    }
    fn get_track_nibbles(&mut self,_trk: super::Track) -> Result<Vec<u8>,DYNERR> {
        error!("IMG images have no track bits");
        return Err(Box::new(img::Error::ImageTypeMismatch));        
    }
    fn display_track(&self,_bytes: &[u8]) -> String {
        String::from("IMG images have no track bits to display")
    }
}