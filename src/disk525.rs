//! # Low level treatment of 5.25 inch floppy disks
//! 
//! This handles the detailed track layout of a real floppy disk.
//! This module is only needed at the disk image implementation level.
//! Acknowledgment: some of this module is adapted from CiderPress.

use thiserror;
use log::{info,warn,error};

const INVALID_NIB_BYTE: u8 = 0xff;
const CHUNK62: usize = 0x56;

const DISK_BYTES_53: [u8;32] = [
    0xab, 0xad, 0xae, 0xaf, 0xb5, 0xb6, 0xb7, 0xba,
    0xbb, 0xbd, 0xbe, 0xbf, 0xd6, 0xd7, 0xda, 0xdb,
    0xdd, 0xde, 0xdf, 0xea, 0xeb, 0xed, 0xee, 0xef,
    0xf5, 0xf6, 0xf7, 0xfa, 0xfb, 0xfd, 0xfe, 0xff
];

const DISK_BYTES_62: [u8;64] = [
    0x96, 0x97, 0x9a, 0x9b, 0x9d, 0x9e, 0x9f, 0xa6,
    0xa7, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb2, 0xb3,
    0xb4, 0xb5, 0xb6, 0xb7, 0xb9, 0xba, 0xbb, 0xbc,
    0xbd, 0xbe, 0xbf, 0xcb, 0xcd, 0xce, 0xcf, 0xd3,
    0xd6, 0xd7, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde,
    0xdf, 0xe5, 0xe6, 0xe7, 0xe9, 0xea, 0xeb, 0xec,
    0xed, 0xee, 0xef, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6,
    0xf7, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff
];

#[derive(thiserror::Error,Debug)]
pub enum NibbleError {
    #[error("could not interpret track data")]
    BadTrack,
    #[error("invalid byte while decoding")]
    InvalidByte,
    #[error("bad checksum found in a sector")]
    BadChecksum,
    #[error("could not find bit pattern")]
    BitPatternNotFound
}

/// This represents a track at the level of bits, and only bit-level access is allowed.
/// The underlying `Vec<u8>` is exposed only upon construction, any padding is determined at this stage.
/// This will also behave as a cyclic buffer to reflect a circular track.
pub struct TrackBits {
    bit_ptr: usize,
    buf: Vec<u8>
}

impl TrackBits {
    pub fn new(buf: Vec<u8>) -> Self {
        Self {
            bit_ptr: 0,
            buf
        }
    }
    pub fn len(&self) -> usize {
        return self.buf.len();
    }
    pub fn reset(&mut self) {
        self.bit_ptr = 0;
    }
    pub fn shift_fwd(&mut self,bit_shift: usize) {
        let bits = self.buf.len() * 8;
        let mut ptr = self.bit_ptr;
        ptr += bit_shift;
        while ptr >= bits {
            ptr -= bits;
        }
        self.bit_ptr = ptr;
    }
    pub fn shift_rev(&mut self,bit_shift: usize) {
        let bits = self.buf.len() as i64 * 8;
        let mut ptr = self.bit_ptr as i64;
        ptr -= bit_shift as i64;
        while ptr < 0 {
            ptr += bits;
        }
        self.bit_ptr = ptr as usize;
    }
    /// Read the current bit, return in LSB of a byte; perhaps more efficient than `read` for matching bit patterns
    pub fn next(&mut self) -> u8 {
        let i = self.bit_ptr/8;
        let b = 7 - (self.bit_ptr%8) as u8;
        self.shift_fwd(1);
        return (self.buf[i] >> b) & 1;
    }
    /// Bits are loaded into a slice of packed bytes, only `num_bits` of them loaded,
    /// the remaining are left untouched.  Bit order is MSB to LSB.
    pub fn read(&mut self,data: &mut [u8],num_bits: usize) {
        for i in 0..num_bits {
            let src_idx = self.bit_ptr/8;
            let src_rel_bit = 7 - (self.bit_ptr%8) as u8;
            let dst_idx = i/8;
            let dst_rel_bit = 7 - (i%8) as u8;
            let term = ((self.buf[src_idx] >> src_rel_bit) & 1) << dst_rel_bit;
            data[dst_idx] &= (1 << dst_rel_bit) ^ u8::MAX;
            data[dst_idx] |= term;
            self.shift_fwd(1);
        }
    }
    /// Bits are packed into a slice of bytes, only `num_bits` of them are unpacked and written,
    /// the rest are padding that is ignored.  Bit order is MSB to LSB.
    pub fn write(&mut self,data: &[u8],num_bits: usize) {
        for i in 0..num_bits {
            let dst_idx = self.bit_ptr/8;
            let dst_rel_bit = 7 - (self.bit_ptr%8) as u8;
            let src_idx = i/8;
            let src_rel_bit = 7 - (i%8) as u8;
            let term = ((data[src_idx] >> src_rel_bit) & 1) << dst_rel_bit;
            self.buf[dst_idx] &= (1 << dst_rel_bit) ^ u8::MAX;
            self.buf[dst_idx] |= term;
            self.shift_fwd(1);
        }
    }
    /// Retrieve the bytes in which the bits are packed
    pub fn to_buffer(&self) -> Vec<u8> {
        return self.buf.clone();
    }
}

#[derive(PartialEq)]
enum NibbleType {
    Enc44,
    Enc53,
    Enc62
}

#[derive(PartialEq)]
pub enum NibbleSpecial {
    None,
    Muse,
    SkipFirstAddrByte
}

pub struct SectorAddressFormat {
    prolog: [u8;3],
    epilog: [u8;3],
    chk_seed: u8,
    verify_chk: bool,
    verify_track: bool,
    verify_epilog_count: usize,
    nib: NibbleType
}

impl SectorAddressFormat {
    pub fn create_std() -> Self {
        Self {
            prolog: [0xd5,0xaa,0x96],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            verify_track: true,
            verify_epilog_count: 2,
            nib: NibbleType::Enc62
        }
    }
}

pub struct SectorDataFormat {
    prolog: [u8;3],
    epilog: [u8;3],
    chk_seed: u8,
    verify_chk: bool,
    verify_epilog_count: usize,
    nib: NibbleType
}

impl SectorDataFormat {
    pub fn create_std() -> Self {
        Self {
            prolog: [0xd5,0xaa,0xad],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            verify_epilog_count: 2,
            nib: NibbleType::Enc62
        }
    }
}
fn invert_53() -> [u8;256] {
    let mut ans: [u8;256] = [INVALID_NIB_BYTE;256];
    for i in 0..32 {
        ans[DISK_BYTES_53[i] as usize] = i as u8;
    }
    return ans;
}

fn invert_62() -> [u8;256] {
    let mut ans: [u8;256] = [INVALID_NIB_BYTE;256];
    for i in 0..64 {
        ans[DISK_BYTES_62[i] as usize] = i as u8;
    }
    return ans;
}

/// encode two nibbles into two disk-friendly u8's
fn encode_44(val: u8) -> [u8;2] {
    return [(val >> 1) | 0xaa, val | 0xaa];
}

/// decode two bytes, returning the nibbles in a single u8
fn decode_44(nibs: [u8;2]) -> u8 {
    return ((nibs[0] << 1) | 0x01) & nibs[1]
}

/// encode a 5-bit nibble as a disk-friendly u8
fn encode_53(nib5: u8) -> u8 {
    return DISK_BYTES_53[(nib5 & 0x1f) as usize];
}

/// decode a byte, returning a 5-bit nibble in a u8
fn decode_53(byte: u8,inv: [u8;256]) -> u8 {
    return inv[byte as usize];
}

/// encode a 6-bit nibble as a disk-friendly u8
fn encode_62(nib6: u8) -> u8 {
    return DISK_BYTES_62[(nib6 & 0x3f) as usize];
}

/// decode a byte, returning a 6-bit nibble in a u8
fn decode_62(byte: u8,inv: [u8;256]) -> u8 {
    return inv[byte as usize];
}

/// Get physical sector from logical sector
pub fn physical_sector(logical_sector: u8) -> u8 {
    let phys_sec: [u8;16] = [0,7,14,6,13,5,12,4,11,3,10,2,9,1,8,15];
    return phys_sec[logical_sector as usize];
}
/// Get logical sector from physical sector
pub fn logical_sector(physical_sector: u8) -> u8 {
    let log_sec: [u8;16] = [0,13,11,9,7,5,3,1,14,12,10,8,6,4,2,15];
    return log_sec[physical_sector as usize];
}

/// Get block number and byte offset into block corresponding to
/// a given track and sector.  Returned in tuple (block,offset)
pub fn block_from_ts(track: u8,sector: u8) -> (u8,usize) {
    let block_offset: [u8;16] = [0,7,6,6,5,5,4,4,3,3,2,2,1,1,0,7];
    let byte_offset: [usize;16] = [0,0,256,0,256,0,256,0,256,0,256,0,256,0,256,256];
    return (8*track + block_offset[sector as usize], byte_offset[sector as usize]);
}

/// Get the two track and sector pairs corresponding to a block.
/// The returned tuple is arranged in order.
pub fn ts_from_block(block: u16) -> ([u8;2],[u8;2]) {
    let sector1: [u8;8] = [0,13,11,9,7,5,3,1];
    let sector2: [u8;8] = [14,12,10,8,6,4,2,15];
    return (
        [(block/8) as u8, sector1[block as usize % 8]],
        [(block/8) as u8, sector2[block as usize % 8]]
    );
}

/// Return tuple with (vol,track,sector,chksum)
fn decode_addr(track: &mut TrackBits) -> (u8,u8,u8,u8) {
    let mut buf: [u8;8] = [0;8];
    track.read(&mut buf,64);
    return (
        decode_44([buf[0],buf[1]]),
        decode_44([buf[2],buf[3]]),
        decode_44([buf[4],buf[5]]),
        decode_44([buf[6],buf[7]])
    );
}

/// Advance to bit pointer until a given pattern is matched, pattern can be up to 32 bits
/// If pattern is found return the number of bits by which pointer advanced, otherwise return None.
fn find_bit_pattern(buf: &mut TrackBits,patt: u32,patt_len: usize) -> Option<usize> {
    if patt_len==0 {
        return Some(0);
    }
    let mut matches = 0;
    for tries in 0..buf.len()*8 {
        if buf.next()==((patt >> (31-matches)) & 1) as u8 {
            matches += 1;
        } else {
            matches = 0;
        }
        if matches==patt_len {
            return Some(tries+1);
        }
    }
    return None;
}

/// Advance the bit pointer to the sector data, and return the volume, or an error.
/// This accounts for a couple special format variants per the `special` argument.
fn find_sector_data(
buf: &mut TrackBits,
ts: [u8;2],
adr_fmt: &SectorAddressFormat,
dat_fmt: &SectorDataFormat,
special: &NibbleSpecial) -> Result<u8,NibbleError> {
    // Set up the search patterns
    let pro = &adr_fmt.prolog;
    let epi = &adr_fmt.epilog;
    let dpro = &dat_fmt.prolog;
    let (prolog_bit_len,prolog_patt) = match special {
        NibbleSpecial::SkipFirstAddrByte => (16, u32::from_le_bytes([pro[1],pro[2],0,0])),
        _ => (24, u32::from_le_bytes([pro[0],pro[1],pro[2],0]))
    };
    let (epilog_bit_len,epilog_patt) = (
        adr_fmt.verify_epilog_count*8, u32::from_le_bytes([epi[0],epi[1],epi[2],0])
    );
    let (data_bit_len,data_patt) = (
        24, u32::from_le_bytes([dpro[0],dpro[1],dpro[2],0])
    );
    // Loop over attempts to read a sector
    for _try in 0..32 {
        if let Some(shift) = find_bit_pattern(buf,prolog_patt,prolog_bit_len) {
            let (vol,track,mut sector,chksum) = decode_addr(buf);
            let chk = adr_fmt.chk_seed ^ vol ^ track ^ sector ^ chksum;
            if adr_fmt.verify_track && track!=ts[0] {
                info!("track mismatch (want {}, got {})",ts[0],track);
                continue;
            }
            if adr_fmt.verify_chk && chk != 0 {
                info!("checksum nonzero ({})",chk);
                continue;
            }
            if find_bit_pattern(buf, epilog_patt, epilog_bit_len)==None {
                continue;
            }
            // we have a good header
            if *special==NibbleSpecial::Muse {
                // e.g. original Castle Wolfenstein
                if ts[0] > 2 {
                    if (sector & 0x01) != 0 {
                        continue;
                    }
                    sector /= 2;
                }
            }
            if ts[1] != sector {
                continue;
            }
            if let Some(shift) = find_bit_pattern(buf,data_patt,data_bit_len) {
                return Ok(vol);
            } else {
                return Err(NibbleError::BitPatternNotFound);
            }
        } else {
            // After circumnavigating the whole track, no prolog ever found
            return Err(NibbleError::BitPatternNotFound);
        }
    }
    // We tried as many times as there could be sectors, must be a bad track
    return Err(NibbleError::BadTrack);
}

/// Assuming the bit pointer is at sector data, write the sector
fn encode_sector(buf: &mut TrackBits,dat: &Vec<u8>,dat_fmt: &SectorDataFormat) {
    if dat_fmt.nib!=NibbleType::Enc62 {
        panic!("only 6 bit nibbles allowed");
    }
    // first work with bytes; direct adaptation from CiderPress `EncodeNibble62`
    let mut bak_buf: [u8;343] = [0;343];
    let mut top: [u8;256] = [0;256];
    let mut twos: [u8;CHUNK62] = [0;CHUNK62];
    let mut two_shift = 0;
    let mut two_pos_n = CHUNK62-1;
    for i in 0..256 {
        let val = dat[i];
        top[i] = val >> 2;
        twos[two_pos_n] |= ((val & 1) << 1 | (val & 2) >> 1) << two_shift;
        if two_pos_n==0 {
            two_pos_n = CHUNK62;
            two_shift += 2;
        }
        two_pos_n -= 1;
    }
    let mut chksum = dat_fmt.chk_seed;
    let mut idx = 0;
    for i in (0..CHUNK62).rev() {
        bak_buf[idx] = encode_62(twos[i] ^ chksum);
        chksum = twos[i];
        idx += 1;
    }
    for i in 0..256 {
        bak_buf[idx] = encode_62(top[i] ^ chksum);
        chksum = top[i];
        idx += 1;
    }
    bak_buf[idx] = encode_62(chksum);
    // now copy the bits into the track from the backing buffer
    buf.write(&bak_buf,343*8);
}

/// Assuming the bit pointer is at sector data, decode and return the sector
fn decode_sector(buf: &mut TrackBits, dat_fmt: &SectorDataFormat) -> Result<Vec<u8>,NibbleError> {
    if dat_fmt.nib!=NibbleType::Enc62 {
        panic!("only 6 bit nibbles allowed");
    }
    let mut ans: Vec<u8> = Vec::new();
    // First get the bits into an ordinary byte-aligned buffer
    let mut bak_buf: [u8;343] = [0;343];
    buf.read(&mut bak_buf,343*8);
    // Now decode; direct adaptation from CiderPress `DecodeNibble62`
    let mut twos: [u8;CHUNK62 as usize*3] = [0;CHUNK62 as usize*3];
    let mut chksum = dat_fmt.chk_seed;
    let inv = invert_62();
    let mut idx = 0;
    for i in 0..CHUNK62 {
        let val = decode_62(bak_buf[idx],inv);
        if val==INVALID_NIB_BYTE {
            return Err(NibbleError::InvalidByte);
        }
        chksum ^= val;
        twos[i] = ((chksum & 0x01) << 1) | ((chksum & 0x02) >> 1);
        twos[i + CHUNK62] = ((chksum & 0x04) >> 1) | ((chksum & 0x08) >> 3);
        twos[i + CHUNK62*2] = ((chksum & 0x10) >> 3) | ((chksum & 0x20) >> 5);
        idx += 1;
    }
    for i in 0..256 {
        let val = decode_62(bak_buf[idx],inv);
        if val==INVALID_NIB_BYTE {
            return Err(NibbleError::InvalidByte);
        }
        chksum ^= val;
        ans.push((chksum << 2) | twos[i]);
        idx += 1;
    }
    // we have the sector, now verify checksum
    let val = decode_62(bak_buf[idx],inv);
    if val==INVALID_NIB_BYTE {
        return Err(NibbleError::InvalidByte);
    }
    chksum ^= val;
    if dat_fmt.verify_chk && chksum!=0 {
        return Err(NibbleError::BadChecksum)
    }
    return Ok(ans);
}

/// Add `num` 10-bit sync-bytes to the track
fn write_sync_gap(buf: &mut TrackBits,num: usize) {
    for _i in 0..num {
        buf.write(&[0xff,0x00],10);
    }
}

/// This creates a track including sync bytes, address fields, nibbles, checksums, etc..
/// The data fields are all empty.
pub fn create_track(vol: u8,track: u8,adr_fmt: &SectorAddressFormat, dat_fmt: &SectorDataFormat, special: &NibbleSpecial) -> TrackBits {
    if dat_fmt.nib!=NibbleType::Enc62 {
        panic!("only 6 bit nibbles allowed");
    }
    let mut ans = TrackBits::new([0;13*512].to_vec());
    let mut linear: Vec<u8> = Vec::new();
    write_sync_gap(&mut ans, 40);
    for sector in 0..16 {
        // address field
        ans.write(&adr_fmt.prolog,24);
        ans.write(&encode_44(vol),16);
        ans.write(&encode_44(track),16);
        ans.write(&encode_44(sector),16);
        let chksum = adr_fmt.chk_seed ^ vol ^ track ^ sector;
        ans.write(&encode_44(chksum),16);
        ans.write(&adr_fmt.epilog,24);
        // sync gap
        write_sync_gap(&mut ans, 10);
        // data field
        ans.write(&dat_fmt.prolog,16);
        encode_sector(&mut ans,&[0;256].to_vec(),dat_fmt);
        ans.write(&dat_fmt.epilog,24);
        //sync gap
        write_sync_gap(&mut ans, 20);
    }
    return ans;
}

/// Create track bits using the data in a DOS ordered image
pub fn track_from_do(do_img: &Vec<u8>,track: u8) -> TrackBits {
    let vol = do_img[17*4096 + 6];
    let special = NibbleSpecial::None;
    let adr_fmt = SectorAddressFormat::create_std();
    let dat_fmt = SectorDataFormat::create_std();
    let mut ans = create_track(vol,track,&adr_fmt,&dat_fmt,&special);
    for logical_sector in 0..16 {
        let ts = [track,physical_sector(logical_sector)];
        if let Ok(_vol) = find_sector_data(&mut ans, ts, &adr_fmt, &dat_fmt, &special) {
            let dos_offset = track as usize * 4096 + logical_sector as usize * 256;
            let sbuf = do_img[dos_offset..dos_offset+256].to_vec();
            encode_sector(&mut ans,&sbuf,&dat_fmt);
        }
    }
    return ans;
}

/// Write track bits into a DOS ordered image
pub fn track_to_do(do_img: &mut Vec<u8>,track: u8,buf: &mut TrackBits,adr_fmt: &SectorAddressFormat,dat_fmt: &SectorDataFormat,special: &NibbleSpecial) -> Result<usize,NibbleError> {
    for logical_sector in 0..16 {
        let dos_offset = track as usize * 4096 + logical_sector as usize * 256;
        let ts = [track,physical_sector(logical_sector)];
        match find_sector_data(buf, ts, &adr_fmt, &dat_fmt, &special) {
            Ok(_vol) => {
                match decode_sector(buf,dat_fmt) {
                    Ok(sec_data) => {
                        for i in 0..256 {
                            do_img[dos_offset+i] = sec_data[i];
                        }
                    },
                    Err(e) => return Err(e)
                }
            },
            Err(e) => return Err(e)
        }
    }
    return Ok(0);
}