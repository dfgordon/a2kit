//! ## Common components for WOZ1 or WOZ2 disk images
//! 
//! Limitations of WOZ support (TODO)
//! * extended 0-runs are not randomized
//! * no flux tracks allowed

use log::{debug,trace};
use std::fmt::Write;
use crate::fs::Block;
use crate::bios::skew;

use crate::{STDRESULT,DYNERR};
const RCH: &str = "unreachable was reached";

pub const INFO_ID: u32 = 0x4f464e49;
pub const TMAP_ID: u32 = 0x50414d54;
pub const TRKS_ID: u32 = 0x534b5254;
pub const WRIT_ID: u32 = 0x54495257;
pub const META_ID: u32 = 0x4154454D;
pub const ALLOWED_TRACKS_525: [usize;1] = [35];

pub struct HeadCoords {
    pub track: usize,
    pub bit_ptr: usize
}

/// Trait allowing us to write only 1 set of operations for both WOZ types.
/// We end up with some generic functions that get called by methods of the same name.
pub trait WozUnifier {
	fn kind(&self) -> super::DiskKind;
	fn num_tracks(&self) -> usize;
    /// Wrapper for track object function that works with a cached object
    fn write_sector(&mut self,dat: &[u8],track: u8,sector: u8) -> Result<(),super::NibbleError>;
    /// Wrapper for track object function that works with a cached object
    fn read_sector(&mut self,track: u8,sector: u8) -> Result<Vec<u8>,super::NibbleError>;
}

const CRC32_TAB: [u32;256] = [
	0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f,
	0xe963a535, 0x9e6495a3, 0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988,
	0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91, 0x1db71064, 0x6ab020f2,
	0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7,
	0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9,
	0xfa0f3d63, 0x8d080df5, 0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172,
	0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b, 0x35b5a8fa, 0x42b2986c,
	0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59,
	0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423,
	0xcfba9599, 0xb8bda50f, 0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924,
	0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d, 0x76dc4190, 0x01db7106,
	0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
	0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d,
	0x91646c97, 0xe6635c01, 0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e,
	0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457, 0x65b0d9c6, 0x12b7e950,
	0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65,
	0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7,
	0xa4d1c46d, 0xd3d6f4fb, 0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0,
	0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9, 0x5005713c, 0x270241aa,
	0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
	0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81,
	0xb7bd5c3b, 0xc0ba6cad, 0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a,
	0xead54739, 0x9dd277af, 0x04db2615, 0x73dc1683, 0xe3630b12, 0x94643b84,
	0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
	0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb,
	0x196c3671, 0x6e6b06e7, 0xfed41b76, 0x89d32be0, 0x10da7a5a, 0x67dd4acc,
	0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5, 0xd6d6a3e8, 0xa1d1937e,
	0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
	0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55,
	0x316e8eef, 0x4669be79, 0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236,
	0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f, 0xc5ba3bbe, 0xb2bd0b28,
	0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d,
	0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f,
	0x72076785, 0x05005713, 0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38,
	0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21, 0x86d3d2d4, 0xf1d4e242,
	0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
	0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69,
	0x616bffd3, 0x166ccf45, 0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2,
	0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db, 0xaed16a4a, 0xd9d65adc,
	0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
	0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605, 0xcdd70693,
	0x54de5729, 0x23d967bf, 0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94,
	0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d
];

/// Calculate the checksum for the WOZ data in `buf`
pub fn crc32(crc_seed: u32, buf: &Vec<u8>) -> u32
{
	let mut crc = crc_seed ^ !(0 as u32);
	for p in buf {
	    crc = CRC32_TAB[((crc ^ *p as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
	return crc ^ !(0 as u32);
}

/// Get the next WOZ metadata chunk.  Return tuple (ptr,id,Option(chunk)).
/// Here `ptr` is the index to the subsequent chunk, which can be passed back in.
/// If `ptr`=0 no more chunks. Option(chunk)=None means unknown id or incongruous size.
/// The returned chunk buffer includes the id and size in the first 8 bytes.
/// The `DiskStruct` trait can be used to unpack the chunk at higher levels.
pub fn get_next_chunk(ptr: usize,buf: &Vec<u8>) -> (usize,u32,Option<Vec<u8>>) {
	if ptr+8 > buf.len() {
		return (0,0,None);
	}
	let id = u32::from_le_bytes([buf[ptr],buf[ptr+1],buf[ptr+2],buf[ptr+3]]);
	let size = u32::from_le_bytes([buf[ptr+4],buf[ptr+5],buf[ptr+6],buf[ptr+7]]);
	let end = ptr + 8 + size as usize;
	let mut next = end;
	// if size puts us beyond end of file this is not a good chunk, and also no more chunks
	if end > buf.len() {
		return (0,0,None);
	}
	// if there is not enough room for the next chunk header then no more chunks
	if next+8 > buf.len() {
		next = 0;
	}
	if id==0 && size==0 {
		debug!("expected chunk, got nulls");
	} else {
		debug!("found chunk id {:08X}/{}, at offset {}, next offset {}",id,String::from_utf8_lossy(&u32::to_le_bytes(id)),ptr,next);
	}
	match id {
		INFO_ID | TMAP_ID | TRKS_ID | WRIT_ID | META_ID => {
			// found something
			return (next,id,Some(buf[ptr..end].to_vec()));
		}
		_ => {
			// unknown chunk type
			return (next,id,None);
		}
	}
}

/// Get the ordered physical track-sector list and sector size for any block
fn get_ts_list(addr: Block,kind: &super::DiskKind) -> Result<(Vec<[usize;2]>,usize),DYNERR> {
	match addr {
		Block::D13([t,s]) => Ok((vec![[t,s]],256)),
		Block::DO([t,s]) => Ok((vec![[t,skew::DOS_LSEC_TO_DOS_PSEC[s]]],256)),
		Block::PO(block) => {
			let mut ans = skew::ts_from_prodos_block(block,kind)?;
			match *kind {
				super::names::A2_DOS33_KIND => {
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
		Block::FAT((_s1,_secs)) => Err(Box::new(super::Error::ImageTypeMismatch))
	}
}

/// Find the file system allocation unit given by `addr` and return the data or an error.
/// Blocks are not allowed to cross track boundaries.
/// This relies on the disk kind being correct to invoke the correct nibbles.
/// For 3.5 inch disks, the returned data has the tag bytes stripped.
pub fn read_block<T: WozUnifier>(woz: &mut T,addr: Block) -> Result<Vec<u8>,DYNERR> {
	trace!("reading {}",addr);
	let mut ans: Vec<u8> = Vec::new();
	let (ts_list,sec_len) = get_ts_list(addr,&woz.kind())?;
	let track = ts_list[0][0];
	if track >= woz.num_tracks() {
		debug!("track {} out of bounds ({})",track,woz.num_tracks());
		return Err(Box::new(super::Error::TrackCountMismatch));
	}
	for ts in ts_list {
		let [track,sector] = [ts[0] as u8,ts[1] as u8];
		trace!("woz read track {} sector {}",track,sector);
		match woz.read_sector(track,sector) {
			Ok(mut v) => ans.append(&mut v),
			Err(e) => return Err(Box::new(e))
		}
	}
	match sec_len {
		524 => Ok(ans[12..524].to_vec()),
		_ => Ok(ans)
	}
}

/// Write the given buffer to the file system allocation unit given by `addr`.
/// Blocks are not allowed to cross track boundaries.
/// This relies on the disk kind being correct to invoke the correct nibbles.
/// For 3.5 inch disks, tag bytes should not be included.
pub fn write_block<T: WozUnifier>(woz: &mut T,addr:Block,dat: &[u8]) -> STDRESULT {
	trace!("writing {}",addr);
	let (ts_list,sec_len) = get_ts_list(addr,&woz.kind())?;
	let track = ts_list[0][0];
	let padded = match sec_len {
		524 => {
			let mut tagged: Vec<u8> = vec![0;12];
			tagged.append(&mut dat.to_vec());
			super::quantize_block(&tagged, ts_list.len()*sec_len)
		},
		_ => super::quantize_block(dat, ts_list.len()*sec_len)
	};
	if track >= woz.num_tracks() {
		debug!("track {} out of bounds ({})",track,woz.num_tracks());
		return Err(Box::new(super::Error::TrackCountMismatch));
	}
	let mut offset = 0;
	for ts in ts_list {
		let [track,sector] = [ts[0] as u8,ts[1] as u8];
		trace!("woz write track {} sector {}",track,sector);
		woz.write_sector(&padded[offset..offset+sec_len].to_vec(),track,sector)?;
		offset += sec_len;
	}
	return Ok(());
}

pub fn cyl_head_to_track<T: WozUnifier>(woz: &T,cyl: usize,head: usize) -> Result<usize,DYNERR> {
	let (track,heads) = match woz.kind() {
		super::names::A2_400_KIND => (cyl,1),
		super::names::A2_800_KIND => (2*cyl+head,2),
		_ => (cyl,1)
	};
	if head >= heads {
		debug!("requested head {}, max {}",head,heads-1);
		return Err(Box::new(super::Error::SectorAccess));
	}
	if track >= woz.num_tracks() {
		debug!("requested track {}, max {}",track,woz.num_tracks()-1);
		return Err(Box::new(super::Error::TrackCountMismatch));
	}
	Ok(track)
}

/// Read the physical track and sector.
/// This relies on the disk kind being correct to invoke the correct nibbles.
/// For 3.5 inch disks, the returned data has the tag bytes stripped.
pub fn read_sector<T: WozUnifier>(woz: &mut T,cyl: usize,head: usize,sector: usize) -> Result<Vec<u8>,DYNERR> {
	let track = cyl_head_to_track(woz,cyl,head)?;
	trace!("woz read track {} sector {}",track,sector);
	let ans = woz.read_sector(track as u8,sector as u8)?;
	if ans.len()==524 {
		return Ok(ans[12..524].to_vec());
	}
	Ok(ans)
}

/// Write the physical track and sector.
/// This relies on the disk kind being correct to invoke the correct nibbles.
/// For 3.5 inch disks, tag bytes should not be included.
pub fn write_sector<T: WozUnifier>(woz: &mut T,cyl: usize,head: usize,sector: usize,dat: &[u8]) -> STDRESULT {
	let track = cyl_head_to_track(woz, cyl, head)?;
	let padded = match woz.kind() {
		super::names::A2_400_KIND | super::names::A2_800_KIND => {
			let mut tagged: Vec<u8> = vec![0;12];
			tagged.append(&mut dat.to_vec());
			super::quantize_block(&tagged, 524)
		},
		_ => super::quantize_block(dat, 256)
	};
	trace!("woz write track {} sector {}",track,sector);
	woz.write_sector(&padded,track as u8,sector as u8)?;
	return Ok(());
}

/// Display aligned track nibbles to stdout in columns of hex, track mnemonics
pub fn display_track<T: WozUnifier>(woz: &T,start_addr: u16,trk: &[u8]) -> String {
	let mut ans = String::new();
    let mut slice_start = 0;
    let mut addr_count = 0;
    let mut err_count = 0;
	let [apro,aepi,dpro,depi]: [[u8;3];4] = match woz.kind() {
		super::names::A2_DOS32_KIND => [[0xd5,0xaa,0xb5],[0xde,0xaa,0xeb],[0xd5,0xaa,0xad],[0xde,0xaa,0x00]],
		super::names::A2_400_KIND => [[0xd5,0xaa,0x96],[0xde,0xaa,0x00],[0xd5,0xaa,0xad],[0xde,0xaa,0x00]],
		super::names::A2_800_KIND => [[0xd5,0xaa,0x96],[0xde,0xaa,0x00],[0xd5,0xaa,0xad],[0xde,0xaa,0x00]],
		_ => [[0xd5,0xaa,0x96],[0xde,0xaa,0xeb],[0xd5,0xaa,0xad],[0xde,0xaa,0xeb]]
	};
	let nib_table = match woz.kind() {
		super::names::A2_DOS32_KIND => super::disk525::DISK_BYTES_53.to_vec(),
		super::names::A2_400_KIND => super::disk525::DISK_BYTES_62.to_vec(),
		super::names::A2_800_KIND => super::disk525::DISK_BYTES_62.to_vec(),
		_ => super::disk525::DISK_BYTES_62.to_vec()
	};
	let addr_nib_count = match woz.kind() {
		super::names::A2_DOS33_KIND => 8,
		super::names::A2_400_KIND => 5,
		super::names::A2_800_KIND => 5,
		_ => 8
	};
	let inv = super::disk35::invert_62();
    loop {
        let row_label = start_addr as usize + slice_start;
        let mut slice_end = slice_start + 16;
        if slice_end > trk.len() {
            slice_end = trk.len();
        }
        let mut mnemonics = String::new();
        for i in slice_start..slice_end {
            let bak = match i {
                x if x>0 => trk[x-1],
                _ => 0
            };
            let fwd = match i {
                x if x+1<trk.len() => trk[x+1],
                _ => 0
            };
            if !nib_table.contains(&trk[i]) && trk[i]!=0xaa && trk[i]!=0xd5 {
                mnemonics += "?";
                err_count += 1;
            } else if addr_count>0 {
				match (addr_count%2,woz.kind()) {
					(_,super::names::A2_400_KIND | super::names::A2_800_KIND) => {
						let val = super::disk35::decode_62(trk[i], inv);
						match val {
							Ok(x) if x<16 => write!(&mut mnemonics,"{:X}",x).expect(RCH),
							Ok(_) => write!(&mut mnemonics,"^").expect(RCH),
							Err(_) => write!(&mut mnemonics,"?").expect(RCH)
						}
					},
					(1,_) => write!(&mut mnemonics,"{:X}",super::disk525::decode_44([trk[i],fwd]) >> 4).expect(RCH),
					_ => write!(&mut mnemonics,"{:X}",super::disk525::decode_44([bak,trk[i]]) & 0x0f).expect(RCH)
				};
                addr_count += 1;
            } else {
                mnemonics += match (bak,trk[i],fwd) {
                    (0xff,0xff,_) => ">",
                    (_,0xff,0xff) => ">",
					// address prolog
                    (_,a0,a1) if [a0,a1]==apro[0..2] => "(",
                    (a0,a1,a2) if [a0,a1,a2]==apro => "A",
                    (a1,a2,_) if [a1,a2]==apro[1..3] => {addr_count=1;":"},
					// data prolog
                    (_,a0,a1) if [a0,a1]==dpro[0..2] => "(",
                    (a0,a1,a2) if [a0,a1,a2]==dpro => "D",
                    (a1,a2,_) if [a1,a2]==dpro[1..3] => ":",
					// address epilog
                    (_,a0,a1) if [a0,a1]==aepi[0..2] => ":",
                    (a0,a1,a2) if [a0,a1,a2]==aepi || [a0,a1]==aepi[0..2] && aepi[2]==0 => ")",
                    (a1,a2,_) if [a1,a2]==aepi[1..3] => ")",
					// data epilog
                    (_,a0,a1) if [a0,a1]==depi[0..2] => ":",
                    (a0,a1,a2) if [a0,a1,a2]==depi || [a0,a1]==depi[0..2] && depi[2]==0 => ")",
                    (a1,a2,_) if [a1,a2]==depi[1..3] => ")",
                    (_,0xd5,_) => "R",
                    (_,0xaa,_) => "R",
                    _ => "."
                };
            }
            if addr_count>addr_nib_count {
                addr_count = 0;
            }
        }
        for _i in mnemonics.len()..16 {
            mnemonics += " ";
        }
        write!(ans,"{:04X} : ",row_label).expect(RCH);
        for byte in trk[slice_start..slice_end].to_vec() {
            write!(ans,"{:02X} ",byte).expect(RCH);
        }
        for _blank in slice_end..slice_start+16 {
            write!(ans,"   ").expect(RCH);
        }
        writeln!(ans,"|{}|",mnemonics).expect(RCH);
        slice_start += 16;
        if slice_end==trk.len() {
            break;
        }
    }
    if err_count > 0 {
        writeln!(ans).expect(RCH);
        writeln!(ans,"Encountered {} invalid bytes",err_count).expect(RCH);
    }
	ans
}
