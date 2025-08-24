use crate::img::{DiskImage,DiskKind,Track,Sector,names,quantize_block,Error};
use crate::{STDRESULT,DYNERR};
use crate::bios::skew;
use crate::bios::Block;

/// Get block number and byte offset into block corresponding to
/// track and logical sector.  Returned in tuple (block,offset)
// pub fn prodos_block_from_ts(track: usize,sector: usize) -> Result<(usize,usize),DYNERR> {
//     let block_offset: [usize;16] = [0,7,6,6,5,5,4,4,3,3,2,2,1,1,0,7];
//     let byte_offset: [usize;16] = [0,0,256,0,256,0,256,0,256,0,256,0,256,0,256,256];
//     Ok((8*track + block_offset[sector], byte_offset[sector]))
// }

/// Get vector of track and logical sector pairs corresponding to a block.
/// The returned vector is arranged in order.
/// Works for either 5.25 inch (two pairs) or 3.5 inch (one pair) disks.
pub fn ts_from_prodos_block(block: usize,kind: &DiskKind) -> Result<Vec<[usize;2]>,DYNERR> {
    const ZONED_SECS_PER_TRACK: [usize;5] = [12,11,10,9,8];
    // number of blocks occuring prior to start of zone; last element marks the end of disk.
    const ZONE_BOUNDS_1: [usize;6] = [0,192,368,528,672,800];
    const ZONE_BOUNDS_2: [usize;6] = [0,384,736,1056,1344,1600];
    match *kind {
        DiskKind::LogicalSectors(names::A2_DOS33) | names::A2_DOS33_KIND => {
            let sector1: [usize;8] = [0,13,11,9,7,5,3,1];
            let sector2: [usize;8] = [14,12,10,8,6,4,2,15];
            let [track,sec1,sec2] = [block/8,sector1[block%8],sector2[block%8]];
            log::trace!("locate block for 5.25 inch disk: track {}, sectors {},{}",track,sec1,sec2);
            Ok(vec![[track,sec1],[track,sec2]])
        },
        names::A2_400_KIND => {
            let zone = match block {
                x if x<ZONE_BOUNDS_1[1] => 0,
                x if x<ZONE_BOUNDS_1[2] => 1,
                x if x<ZONE_BOUNDS_1[3] => 2,
                x if x<ZONE_BOUNDS_1[4] => 3,
                x if x<ZONE_BOUNDS_1[5] => 4,
                _ => panic!("illegal block request")
            };
            let rel_block = block - ZONE_BOUNDS_1[zone];
            let secs_per_track = ZONED_SECS_PER_TRACK[zone];
            let track = 16 * zone + rel_block/secs_per_track;
            let sector = rel_block%secs_per_track;
            log::trace!("locate block for 3.5 inch disk: track {}, sector {}",track,sector);
            Ok(vec![[track,sector]])
        },
        names::A2_800_KIND => {
            let zone = match block {
                x if x<ZONE_BOUNDS_2[1] => 0,
                x if x<ZONE_BOUNDS_2[2] => 1,
                x if x<ZONE_BOUNDS_2[3] => 2,
                x if x<ZONE_BOUNDS_2[4] => 3,
                x if x<ZONE_BOUNDS_2[5] => 4,
                _ => panic!("illegal block request")
            };
            let rel_block = block - ZONE_BOUNDS_2[zone];
            let secs_per_track = ZONED_SECS_PER_TRACK[zone];
            let track = 32 * zone + rel_block/secs_per_track;
            let sector = rel_block%secs_per_track;
            log::trace!("locate block for 3.5 inch disk: track {}, sector {}",track,sector);
            Ok(vec![[track,sector]])
        },
        _ => {
            log::debug!("cannot map ProDOS block to {}",*kind);
            Err(Box::new(crate::bios::Error::IncompatibleDiskKind))
        }
    }
}

/// Get the ordered physical track-sector list and sector size for any block.
fn get_ts_list(addr: Block,kind: &DiskKind) -> Result<(Vec<[usize;2]>,usize),DYNERR> {
	match addr {
		Block::D13([t,s]) => Ok((vec![[t,s]],256)),
		Block::DO([t,s]) => Ok((vec![[t,skew::DOS_LSEC_TO_DOS_PSEC[s]]],256)),
		Block::PO(block) => {
			let mut ans = ts_from_prodos_block(block,kind)?;
			match *kind {
				names::A2_DOS33_KIND => {
					ans[0][1] = skew::DOS_LSEC_TO_DOS_PSEC[ans[0][1]];
					ans[1][1] = skew::DOS_LSEC_TO_DOS_PSEC[ans[1][1]];
					Ok((ans,256))
				},
				_ => Ok((ans,524))
			}
		},
		Block::CPM((_block,_bsh,_off)) => {
			let mut ans: Vec<[usize;2]> = Vec::new();
			let lsecs = addr.get_lsecs(32);
			// the following assumes blocks are aligned to even lsecs; also the list must be ordered.
			for ts in lsecs {
				if ts[1]%2==0 {
					ans.push([ts[0],skew::CPM_LSEC_TO_DOS_PSEC[ts[1]-1]]);
				}
			}
			Ok((ans,256))
		},
		Block::FAT(_) => Err(Box::new(Error::ImageTypeMismatch)),
		Block::D64(_) => Err(Box::new(Error::ImageTypeMismatch))
	}
}

/// Find the file system allocation unit given by `addr` and return the data or an error.
/// Blocks are not allowed to cross track boundaries.
/// For 3.5 inch disks, the returned data has the tag bytes stripped.
pub fn read_block<T: DiskImage>(woz: &mut T,addr: Block) -> Result<Vec<u8>,DYNERR> {
	log::trace!("reading {}",addr);
	let mut ans: Vec<u8> = Vec::new();
	let (ts_list,sec_len) = get_ts_list(addr,&woz.kind())?;
	for ts in ts_list {
		let [track,sector] = [ts[0],ts[1]];
		log::trace!("woz read track {} sector {}",track,sector);
		let mut sector_data = woz.read_sector(Track::Num(track),Sector::Num(sector))?;
		ans.append(&mut sector_data);
	}
	match sec_len {
		524 => Ok(ans[12..524].to_vec()),
		_ => Ok(ans)
	}
}

/// Write the given buffer to the file system allocation unit given by `addr`.
/// Blocks are not allowed to cross track boundaries.
/// For 3.5 inch disks, tag bytes should not be included.
pub fn write_block<T: DiskImage>(woz: &mut T,addr:Block,dat: &[u8]) -> STDRESULT {
	log::trace!("writing {}",addr);
	let (ts_list,sec_len) = get_ts_list(addr,&woz.kind())?;
	let padded = match sec_len {
		524 => {
			let mut tagged: Vec<u8> = vec![0;12];
			tagged.append(&mut dat.to_vec());
			quantize_block(&tagged, ts_list.len()*sec_len)
		},
		_ => quantize_block(dat, ts_list.len()*sec_len)
	};
	let mut offset = 0;
	for ts in ts_list {
		let [track,sector] = [ts[0],ts[1]];
		log::trace!("woz write track {} sector {}",track,sector);
		woz.write_sector(Track::Num(track),Sector::Num(sector),&padded[offset..offset+sec_len].to_vec())?;
		offset += sec_len;
	}
	return Ok(());
}
