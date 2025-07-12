use crate::fs::Block;
use crate::img::{DiskImage,DiskKind,names,quantize_block,Error};
use crate::img::tracks::TrackKey;
use crate::{STDRESULT,DYNERR};
use crate::bios::skew;

/// Get the ordered physical track-sector list and sector size for any block.
/// This might become private when major version advances.
pub fn get_ts_list(addr: Block,kind: &DiskKind) -> Result<(Vec<[usize;2]>,usize),DYNERR> {
	match addr {
		Block::D13([t,s]) => Ok((vec![[t,s]],256)),
		Block::DO([t,s]) => Ok((vec![[t,skew::DOS_LSEC_TO_DOS_PSEC[s]]],256)),
		Block::PO(block) => {
			let mut ans = skew::ts_from_prodos_block(block,kind)?;
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
		Block::FAT((_s1,_secs)) => Err(Box::new(Error::ImageTypeMismatch))
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
		let mut sector_data = woz.read_pro_sector(TrackKey::Track(track),sector)?;
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
		woz.write_pro_sector(TrackKey::Track(track),sector,&padded[offset..offset+sec_len].to_vec())?;
		offset += sec_len;
	}
	return Ok(());
}
