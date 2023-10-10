//! ## Sector Skewing Module
//! 
//! This contains all the sector skew tables.  This includes any non-trivial transformations
//! between blocks and sectors.
//! 
//! The sector skews are kept separate from file systems and disk images because multiple
//! submodules of either can use the same tables.

use log::{trace,debug,info,error};
use crate::img::disk35;
use crate::img::{names,DiskKind};
use crate::DYNERR;

/// Get the interleave ratio from a *physical* skew table.
/// Will panic if the largest id occurs first or table is bad.
pub fn get_phys_interleave(table: &[usize]) -> usize {
    let first = table[0];
    for rep in 1..table.len() {
        if table[rep]==first+1 {
            return rep;
        }
    }
    panic!("bad physical skew table");
}

/// Skew table for native 8 inch CP/M v1 disks
pub const CPM_1_LSEC_TO_PSEC: [u8;26] = [1,7,13,19,25,5,11,17,23,3,9,15,21,2,8,14,20,26,6,12,18,24,4,10,16,22];
/// Skew table for Nabu 8 inch CP/M disks
pub const CPM_LSEC_TO_NABU_PSEC: [u8;26] = [1,8,15,22,3,10,17,24,5,12,19,26,7,14,21,2,9,16,23,4,11,18,25,6,13,20];
/// Skew table for Osborne 5.25 inch SSSD disks
pub const CPM_LSEC_TO_OSB1_PSEC: [u8;10] = [1,3,5,7,9,2,4,6,8,10];
/// Take CP/M logical sector to DOS logical sector; the offset within the DOS sector is obtained by another table.
pub const CPM_LSEC_TO_DOS_LSEC: [usize;32] = [0,0,6,6,12,12,3,3,9,9,15,15,14,14,5,5,11,11,2,2,8,8,7,7,13,13,4,4,10,10,1,1];
/// Take CP/M logical sector to DOS physical sector; the offset within the DOS sector is obtained by another table.
pub const CPM_LSEC_TO_DOS_PSEC: [usize;32] = [0,0,3,3,6,6,9,9,12,12,15,15,2,2,5,5,8,8,11,11,14,14,1,1,4,4,7,7,10,10,13,13];
/// Take CP/M logical sector to offset within DOS logical sector
pub const CPM_LSEC_TO_DOS_OFFSET: [usize;32] = [0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128,0,128];

/// 3.5 inch disk physical sector skew by zone; inner zone tables are padded to 12 entries
pub const D35_PHYSICAL: [[usize;12];5] = [
    [0,3,6,9,1,4,7,10,2,5,8,11],
    [0,3,6,9,1,4,7,10,2,5,8,0xff],
    [0,5,3,8,1,6,4,9,2,7,0xff,0xff],
    [0,7,5,3,1,8,6,4,2,0xff,0xff,0xff],
    [0,2,4,6,1,3,5,7,0xff,0xff,0xff,0xff],
];

/// Physical sector skew used by DOS 3.2
pub const DOS32_PHYSICAL: [usize;13] = [0,10,7,4,1,11,8,5,2,12,9,6,3];
/// Translate DOS 3.3 logical sector to physical sector
pub const DOS_LSEC_TO_DOS_PSEC: [usize;16] = [0,13,11,9,7,5,3,1,14,12,10,8,6,4,2,15];
/// Translate DOS 3.3 physical sector to logical sector
pub const DOS_PSEC_TO_DOS_LSEC: [usize;16] = [0,7,14,6,13,5,12,4,11,3,10,2,9,1,8,15];

/// Get block number and byte offset into block corresponding to
/// track and logical sector.  Returned in tuple (block,offset)
pub fn prodos_block_from_ts(track: usize,sector: usize) -> Result<(usize,usize),DYNERR> {
    let block_offset: [usize;16] = [0,7,6,6,5,5,4,4,3,3,2,2,1,1,0,7];
    let byte_offset: [usize;16] = [0,0,256,0,256,0,256,0,256,0,256,0,256,0,256,256];
    Ok((8*track + block_offset[sector], byte_offset[sector]))
}

/// Get vector of track and logical sector pairs corresponding to a block.
/// The returned vector is arranged in order.
/// Works for either 5.25 inch (two pairs) or 3.5 inch (one pair) disks.
pub fn ts_from_prodos_block(block: usize,kind: &DiskKind) -> Result<Vec<[usize;2]>,DYNERR> {
    match *kind {
        DiskKind::LogicalSectors(names::A2_DOS33) | names::A2_DOS33_KIND => {
            let sector1: [usize;8] = [0,13,11,9,7,5,3,1];
            let sector2: [usize;8] = [14,12,10,8,6,4,2,15];
            let [track,sec1,sec2] = [block/8,sector1[block%8],sector2[block%8]];
            trace!("locate block for 5.25 inch disk: track {}, sectors {},{}",track,sec1,sec2);
            Ok(vec![[track,sec1],[track,sec2]])
        },
        names::A2_400_KIND => {
            let zone = match block {
                x if x<disk35::ZONE_BOUNDS_1[1] => 0,
                x if x<disk35::ZONE_BOUNDS_1[2] => 1,
                x if x<disk35::ZONE_BOUNDS_1[3] => 2,
                x if x<disk35::ZONE_BOUNDS_1[4] => 3,
                x if x<disk35::ZONE_BOUNDS_1[5] => 4,
                _ => panic!("illegal block request")
            };
            let rel_block = block - disk35::ZONE_BOUNDS_1[zone];
            let secs_per_track = disk35::ZONED_SECS_PER_TRACK[zone];
            let track = 16 * zone + rel_block/secs_per_track;
            let sector = rel_block%secs_per_track;
            trace!("locate block for 3.5 inch disk: track {}, sector {}",track,sector);
            Ok(vec![[track,sector]])
        },
        names::A2_800_KIND => {
            let zone = match block {
                x if x<disk35::ZONE_BOUNDS_2[1] => 0,
                x if x<disk35::ZONE_BOUNDS_2[2] => 1,
                x if x<disk35::ZONE_BOUNDS_2[3] => 2,
                x if x<disk35::ZONE_BOUNDS_2[4] => 3,
                x if x<disk35::ZONE_BOUNDS_2[5] => 4,
                _ => panic!("illegal block request")
            };
            let rel_block = block - disk35::ZONE_BOUNDS_2[zone];
            let secs_per_track = disk35::ZONED_SECS_PER_TRACK[zone];
            let track = 32 * zone + rel_block/secs_per_track;
            let sector = rel_block%secs_per_track;
            trace!("locate block for 3.5 inch disk: track {}, sector {}",track,sector);
            Ok(vec![[track,sector]])
        },
        _ => {
            debug!("cannot map ProDOS block to {}",*kind);
            Err(Box::new(super::Error::IncompatibleDiskKind))
        }
    }
}

/// Take a logical track-sector list and produce a hybrid cylinder-head-sector list.
/// Hybrid means the sector order is logical while the size is physical.
/// Remember CP/M logical sectors are numbered from 1.
/// This assumes the mapping track = cyl*heads + head.
pub fn cpm_blocking(ts_list: Vec<[usize;2]>,sec_shift: u8,heads: usize) -> Result<Vec<[usize;3]>,super::Error> {
    trace!("ts list {:?} (logical deblocked)",ts_list);
    if (ts_list.len() % (1 << sec_shift) != 0) || ((ts_list[0][1]-1) % (1 << sec_shift) != 0) {
        info!("CP/M blocking was misaligned, start {}, length {}",ts_list[0][1],ts_list.len());
        return Err(super::Error::SectorAccess);
    }
    if heads<1 {
        error!("CP/M blocking was passed 0 heads");
        return Err(super::Error::SectorAccess);
    }
    let mut ans: Vec<[usize;3]> = Vec::new();
    let mut track = 0;
    for i in 0..ts_list.len() {
        let lsec = ts_list[i][1];
        if (lsec-1)%(1<<sec_shift) == 0 {
            track = ts_list[i][0];
        }
        if lsec%(1<<sec_shift) == 0 {
            let cyl = track/heads;
            let head = match heads { 1 => 0, _ => track%heads };
            ans.push([cyl,head,1+(lsec-1)/(1<<sec_shift)]);
        } else if ts_list[i][0]!=track {
            info!("CP/M blocking failed, sector crossed track {}",track);
            return Err(super::Error::SectorAccess);
        }
    }
    trace!("ts list {:?} (logical blocked)",ans);
    return Ok(ans);
}

/// Take a logical track-sector list and produce a hybrid cylinder-head-sector list.
/// Hybrid means the sector order is logical while the size is physical.
/// This assumes the mapping track = cyl*heads + head.
pub fn fat_blocking(ts_list: Vec<[usize;2]>,heads: usize) -> Result<Vec<[usize;3]>,super::Error> {
    trace!("ts list {:?} (logical deblocked)",ts_list);
    if heads<1 {
        error!("FAT blocking was passed 0 heads");
        return Err(super::Error::SectorAccess);
    }
    let mut ans: Vec<[usize;3]> = Vec::new();
    for i in 0..ts_list.len() {
        let cyl = ts_list[i][0]/heads;
        let head = match heads { 1 => 0, _ => ts_list[i][0]%heads };
        let lsec = ts_list[i][1];
        ans.push([cyl,head,1+lsec]);
    }
    trace!("ts list {:?} (logical blocked)",ans);
    return Ok(ans);
}
