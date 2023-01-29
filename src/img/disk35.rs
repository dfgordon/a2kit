//! ## Apple 3.5 inch disk module
//! 
//! This handles bit-level processing of a 3.5 inch GCR disk track.
//! The logic state sequencer is approximated by a simple model.
//! The module can handle either 400K or 800K disks.
//! 
//! Acknowledgment: some of this module is adapted from CiderPress.

// TODO: eliminate some of the overlap with disk525
// TODO: what are these tag bytes for?

use super::NibbleError;
use log::{debug,trace,warn};
use crate::bios::skew;

const INVALID_NIB_BYTE: u8 = 0xff;
const CHUNK62: usize = 175;
/// There are 5 zones on the disk.  Zones are characterized by number of sectors per track.
/// The number of cylinders per zone is fixed at 16 (32 tracks per zone).
/// Sectors are always 524 bytes, consisting of 12 "tag bytes" followed by 512 data bytes.
pub const ZONED_SECS_PER_TRACK: [usize;5] = [12,11,10,9,8];
/// number of blocks occuring prior to start of zone (1 side, zone indexes array); last element marks the end of disk.
pub const ZONE_BOUNDS_1: [usize;6] = [0,192,368,528,672,800];
/// number of blocks occuring prior to start of zone (2 sides, zone indexes array); last element marks the end of disk.
pub const ZONE_BOUNDS_2: [usize;6] = [0,384,736,1056,1344,1600];
const SECTOR_SIZE: usize = 524; // 12 tag byte header + 512 data bytes
const DATA_NIBS: usize = 699; // nibbles of data, checksum follows 
const CHK_NIBS: usize = 4; // how many checksum nibbles after data

// Following constants give the layout of the bits on a track
const ADDRESS_FULL_SEGMENT: usize = 3 + 5 + 2; // prolog,cyl,sec,side,format,chk,epilog
const DATA_FULL_SEGMENT: usize = 3 + 1 + DATA_NIBS + CHK_NIBS + 2; // prolog,sec,data+chk,epilog
const SYNC_TRACK_HEADER: usize = 36;
const SYNC_GAP: usize = 6;
const SYNC_CLOSE: usize = 36;
const SECTOR_BITS: usize = ADDRESS_FULL_SEGMENT*8 + SYNC_GAP*10 + DATA_FULL_SEGMENT*8 + SYNC_CLOSE*10;
/// bits needed for a track in the given zone:
pub const TRACK_BITS: [usize;5] = [
    SYNC_TRACK_HEADER*10 + ZONED_SECS_PER_TRACK[0]*SECTOR_BITS,
    SYNC_TRACK_HEADER*10 + ZONED_SECS_PER_TRACK[1]*SECTOR_BITS,
    SYNC_TRACK_HEADER*10 + ZONED_SECS_PER_TRACK[2]*SECTOR_BITS,
    SYNC_TRACK_HEADER*10 + ZONED_SECS_PER_TRACK[3]*SECTOR_BITS,
    SYNC_TRACK_HEADER*10 + ZONED_SECS_PER_TRACK[4]*SECTOR_BITS
];

pub const DISK_BYTES_62: [u8;64] = [
    0x96, 0x97, 0x9a, 0x9b, 0x9d, 0x9e, 0x9f, 0xa6,
    0xa7, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb2, 0xb3,
    0xb4, 0xb5, 0xb6, 0xb7, 0xb9, 0xba, 0xbb, 0xbc,
    0xbd, 0xbe, 0xbf, 0xcb, 0xcd, 0xce, 0xcf, 0xd3,
    0xd6, 0xd7, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde,
    0xdf, 0xe5, 0xe6, 0xe7, 0xe9, 0xea, 0xeb, 0xec,
    0xed, 0xee, 0xef, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6,
    0xf7, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff
];

/// How to find and read the sector address fields
#[derive(Clone,Copy)]
pub struct SectorAddressFormat {
    prolog: [u8;3],
    epilog: [u8;2],
    chk_seed: u8,
    verify_chk: bool,
    verify_track: bool,
    prolog_mask: [u8;3],
    epilog_mask: [u8;2]
}

impl SectorAddressFormat {
    pub fn create_std() -> Self {
        Self {
            prolog: [0xd5,0xaa,0x96],
            epilog: [0xde,0xaa],
            chk_seed: 0x00,
            verify_chk: true,
            verify_track: true,
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff]
        }
    }
}

/// How to find and read the sector data
#[derive(Clone,Copy)]
pub struct SectorDataFormat {
    prolog: [u8;3],
    epilog: [u8;2],
    prolog_mask: [u8;3],
    epilog_mask: [u8;2]
}

impl SectorDataFormat {
    pub fn create_std() -> Self {
        Self {
            prolog: [0xd5,0xaa,0xad],
            epilog: [0xde,0xaa],
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff]
        }
    }
}

/// This is the main interface for interacting with 3.5 inch disk tracks.
/// This represents a track at the level of bits.
/// N.b. the number of sides is stored within since it affects the address fields.
/// Writing to the track is at the bit stream level, any bit pattern will be accepted.
/// Reading can be done by direct bit stream consumption, or through a soft latch.
/// The underlying `Vec<u8>` is exposed only upon construction, any padding is determined at this stage.
/// This will also behave as a cyclic buffer to reflect a circular track.
pub struct TrackBits {
    sides: usize,
    adr_fmt: SectorAddressFormat,
    dat_fmt: SectorDataFormat,
    bit_count: usize,
    bit_ptr: usize,
    buf: Vec<u8>
}
impl TrackBits {
    /// Create an empty track with default formatting protocol (but no actual format).
    /// Use `disk525::create_track`, or variants, to actually format the track.
    pub fn create(buf: Vec<u8>,bit_count: usize,sides: usize) -> Self {
        if bit_count > buf.len()*8 {
            panic!("buffer cannot hold requested bits");
        }
        Self {
            sides,
            adr_fmt: SectorAddressFormat::create_std(),
            dat_fmt: SectorDataFormat::create_std(),
            bit_count,
            bit_ptr: 0,
            buf
        }
    }
    /// Change the formatting protocol (but not the actual format).
    /// This is used when we are given track bits but don't yet know the format.
    /// One may then try a strategy of supposing various formats in sequence until the track is successfully decoded.
    pub fn set_format_protocol(&mut self,adr_fmt: SectorAddressFormat,dat_fmt: SectorDataFormat) {
        self.adr_fmt = adr_fmt;
        self.dat_fmt = dat_fmt;
    }
    /// Rotate the disk ahead by one bit
    pub fn shift_fwd(&mut self,bit_shift: usize) {
        let mut ptr = self.bit_ptr;
        ptr += bit_shift;
        while ptr >= self.bit_count {
            ptr -= self.bit_count;
        }
        self.bit_ptr = ptr;
    }
    /// Rotate the disk back by one bit
    pub fn shift_rev(&mut self,bit_shift: usize) {
        let mut ptr = self.bit_ptr as i64;
        ptr -= bit_shift as i64;
        while ptr < 0 {
            ptr += self.bit_count as i64;
        }
        self.bit_ptr = ptr as usize;
    }
    /// Read bytes through a soft latch, this is a shortcut that takes the place of
    /// the logic state sequencer, and simplifies the process of retrieving nibbles.
    /// The number of track bits that passed by is returned (not necessarily 8*bytes)
    pub fn read_latch(&mut self,data: &mut [u8],num_bytes: usize) -> usize {
        let mut bit_count: usize = 0;
        for byte in 0..num_bytes {
            for _try in 0..self.bit_count {
                bit_count += 1;
                if self.next()==1 {
                    break;
                }
            }
            let mut val: u8 = 1;
            for _bit in 0..7 {
                val = val*2 + self.next();
            }
            data[byte] = val;
            bit_count += 7;
        }
        return bit_count;
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
    /// Only use to copy tracks or track segments, decodable bits must go through the latch.
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
    /// Retrieve a copy of the bytes in which the bits are packed
    pub fn to_buffer(&self) -> Vec<u8> {
        return self.buf.clone();
    }
    /// Assuming bit pointer is at an address, return tuple with (cyl%64,sector,side,format,checksum).
    /// side LSB indicates if cylinder >= 64.
    fn decode_addr(&mut self) -> Result<(u8,u8,u8,u8,u8),NibbleError> {
        let mut buf: [u8;8] = [0;8];
        self.read_latch(&mut buf,5);
        return Ok((
            decode_62(buf[0],invert_62())?,
            decode_62(buf[1],invert_62())?,
            decode_62(buf[2],invert_62())?,
            decode_62(buf[3],invert_62())?,
            decode_62(buf[4],invert_62())?,
        ));
    }
    /// Collect bytes through the soft latch until a given pattern is matched, or `cap` bytes have been collected.
    /// Low bits in `mask` will cause corresponding bits in `patt` to automatically match.
    /// If `cap` is `None` the entire track will be searched.  `mask` must be as long as `patt`.
    /// If pattern is found return the number of bits by which pointer advanced, otherwise return None.
    fn find_byte_pattern(&mut self,patt: &[u8],mask: &[u8],cap: Option<usize>) -> Option<usize> {
        if patt.len()==0 {
            return Some(0);
        }
        let mut bit_count: usize = 0;
        let mut matches = 0;
        let mut test_byte: [u8;1] = [0;1];
        for tries in 0..self.buf.len() {
            if let Some(max) = cap {
                if tries>=max {
                    return None;
                }
            }
            bit_count += self.read_latch(&mut test_byte,1);
            if test_byte[0] & mask[matches] == patt[matches] & mask[matches] {
                matches += 1;
            } else {
                matches = 0;
            }
            if matches==patt.len() {
                return Some(bit_count);
            }
        }
        return None;
    }
    /// Find the sector as identified by the track's address field value.
    /// Advance the bit pointer to the end of the address epilog, and return the sector number, or an error.
    /// We do not go looking for the data prolog at this stage, sticking with DOS 3.2 strategy of overwriting
    /// the data prolog every time.
    fn find_sector(&mut self,ts: [u8;2]) -> Result<u8,NibbleError> {
        trace!("seeking sector {}",ts[1]);
        // Copy search patterns
        let adr_pro = self.adr_fmt.prolog.clone();
        let adr_pro_mask = self.adr_fmt.prolog_mask.clone();
        let adr_epi = self.adr_fmt.epilog.clone();
        let adr_epi_mask = self.adr_fmt.epilog_mask.clone();
        // Loop over attempts to read a sector
        for _try in 0..32 {
            if let Some(_shift) = self.find_byte_pattern(&adr_pro,&adr_pro_mask,None) {
                let (cyl,sector,side,format,chksum) = self.decode_addr()?;
                trace!("found cyl {}, sec {}, side {}, format {}, chksum {}",cyl,sector,side,format,chksum);
                let chk = self.adr_fmt.chk_seed ^ cyl ^ sector ^ side ^ format;
                // get track from cylinder and side data
                let track = match self.sides {
                    1 => cyl + 64 * (side & 0x01),
                    2 => 2*(cyl + 64 * (side & 0x01)) + (side >> 5),
                    _ => panic!("unexpected sides {}",self.sides)
                };
                if self.adr_fmt.verify_track && track!=ts[0] {
                    warn!("track mismatch (want {}, got {})",ts[0],track);
                    continue;
                }
                if self.adr_fmt.verify_chk && chk != chksum {
                    warn!("checksum mismatch ({},{})",chk,chksum);
                    continue;
                }
                if self.find_byte_pattern(&adr_epi,&adr_epi_mask,Some(10))==None {
                    warn!("missed address epilog");
                    continue;
                }
                // we have a good header
                if ts[1] != sector {
                    trace!("skip sector {}",sector);
                    continue;
                }
                trace!("found sector {}",sector);
                return Ok(sector);
            } else {
                debug!("no address prolog found on track");
                return Err(NibbleError::BadTrack);
            }
        }
        // We tried as many times as there could be sectors, sector is missing
        debug!("the sector address was never matched");
        return Err(NibbleError::SectorNotFound);
    }
    /// Assuming the bit pointer is at sector data, write a 6-2 encoded sector.
    /// The sector data should include the 12 byte header.
    /// Should be called only by encode_sector.
    fn encode_sector_62(&mut self,dat: &Vec<u8>) {
        assert!(dat.len()>=SECTOR_SIZE);
        // first work with bytes; direct adaptation from CiderPress `EncodeNibbleSector35`
        let mut bak_buf: [u8;DATA_NIBS+CHK_NIBS] = [0;DATA_NIBS+CHK_NIBS];
        let mut part0: [u8;CHUNK62] = [0;CHUNK62];
        let mut part1: [u8;CHUNK62] = [0;CHUNK62];
        let mut part2: [u8;CHUNK62] = [0;CHUNK62];
        let [mut chk0,mut chk1,mut chk2]: [usize;3] = [0;3];
        let [mut val,mut twos]: [u8;2];

        let mut i: usize = 0;
        let mut s: usize = 0;
        loop {
            chk0 = (chk0 & 0xff) << 1;
            if chk0 & 0x100 > 0 {
                chk0 += 1;
            }
            val = dat[s];
            chk2 += val as usize;
            if chk0 & 0x100 > 0 {
                chk2 += 1;
                chk0 &= 0xff;
            }
            part0[i] = ((val as usize ^ chk0) & 0xff) as u8;

            val = dat[s+1];
            chk1 += val as usize;
            if chk2 > 0xff {
                chk1 += 1;
                chk2 &= 0xff;
            }
            part1[i] = ((val as usize ^ chk2) & 0xff) as u8;

            if s + 2 >= SECTOR_SIZE {
                break;
            }

            val = dat[s+2];
            chk0 += val as usize;
            if chk1 > 0xff {
                chk0 += 1;
                chk1 &= 0xff;
            }
            part2[i] = ((val as usize ^ chk1) & 0xff) as u8;
            i += 1;
            s += 3;
        }
        assert!(i==CHUNK62-1);

        // nibble data plus an extra byte; extra will be overwritten
        for i in 0..CHUNK62 {
            twos = ((part0[i] & 0xc0) >> 2) | ((part1[i] & 0xc0) >> 4) | ((part2[i] & 0xc0) >> 6);
            bak_buf[i*4+0] = encode_62(twos);
            bak_buf[i*4+1] = encode_62(part0[i] & 0x3f);
            bak_buf[i*4+2] = encode_62(part1[i] & 0x3f);
            if i*4 + 3 < DATA_NIBS+CHK_NIBS {
                bak_buf[i*4+3] = encode_62(part2[i] & 0x3f);
            }
        }

        // checksum
        twos = (((chk0 & 0xc0) >> 6) | ((chk1 & 0xc0) >> 4) | ((chk2 & 0xc0) >> 2)) as u8;
        bak_buf[DATA_NIBS+0] = encode_62(twos);
        bak_buf[DATA_NIBS+1] = encode_62(chk2 as u8 & 0x3f);
        bak_buf[DATA_NIBS+2] = encode_62(chk1 as u8 & 0x3f);
        bak_buf[DATA_NIBS+3] = encode_62(chk0 as u8 & 0x3f);

        // now copy the bits into the track from the backing buffer
        self.write(&bak_buf,(DATA_NIBS+CHK_NIBS)*8);
    }
    /// This writes sync bytes, prolog, data, and epilog.
    /// Assumes bit pointer is at the end of the address epilog.
    /// The `dat` should include the 12 byte header.
    /// This function is allowed to panic.
    fn encode_sector(&mut self,sec: u8,dat: &Vec<u8>) {
        trace!("encoding sector");
        let dat_fmt = self.dat_fmt;
        self.write_sync_gap(SYNC_GAP,10);
        self.write(&dat_fmt.prolog,24);
        // sector number is written here as well as in address fields
        self.write(&[encode_62(sec)],8);
        self.encode_sector_62(dat);
        self.write(&dat_fmt.epilog,16);
    }
    /// Assuming the bit pointer is at sector data, decode from 6-2 and return the sector.
    /// The returned sector data includes 12 byte header.
    /// Should only be called by decode_sector.
    fn decode_sector_62(&mut self) -> Result<Vec<u8>,NibbleError> {
        let mut ans: Vec<u8> = Vec::new();
        // First get the bits into an ordinary byte-aligned buffer
        let mut bak_buf: [u8;DATA_NIBS+CHK_NIBS] = [0;DATA_NIBS+CHK_NIBS];
        self.read_latch(&mut bak_buf,DATA_NIBS+CHK_NIBS);
        // Now decode; direct adaptation from CiderPress `DecodeNibbleSector35`
        let [mut val,mut nib0,mut nib1,mut nib2,mut twos]: [u8;5];
        let mut part0: [u8;CHUNK62] = [0;CHUNK62];
        let mut part1: [u8;CHUNK62] = [0;CHUNK62];
        let mut part2: [u8;CHUNK62] = [0;CHUNK62];
        let mut idx = 0;
        let inv = invert_62();
        for i in 0..CHUNK62 {
            twos = decode_62(bak_buf[idx+0], inv)?;
            nib0 = decode_62(bak_buf[idx+1], inv)?;
            nib1 = decode_62(bak_buf[idx+2], inv)?;
            idx += 3;
            if i != CHUNK62-1 {
                nib2 = decode_62(bak_buf[idx], inv)?;
                idx += 1;
            } else {
                nib2 = 0;
            }
            part0[i] = nib0 | ((twos << 2) & 0xc0);
            part1[i] = nib1 | ((twos << 4) & 0xc0);
            part2[i] = nib2 | ((twos << 6) & 0xc0);
        }

        let [mut chk0,mut chk1,mut chk2]: [usize;3] = [0;3];
        let mut i = 0;
        loop {
            chk0 = (chk0 & 0xff) << 1;
            if chk0 & 0x100 > 0 {
                chk0 += 1;
            }
            val = (part0[i] as usize ^ chk0) as u8;
            chk2 += val as usize;
            if chk0 & 0x100 > 0 {
                chk2 += 1;
                chk0 &= 0xff;
            }
            ans.push(val);

            val = (part1[i] as usize ^ chk2) as u8;
            chk1 += val as usize;
            if chk2 > 0xff {
                chk1 += 1;
                chk2 &= 0xff;
            }
            ans.push(val);

            if ans.len()>=524 {
                break;
            }

            val = (part2[i] as usize ^ chk1) as u8;
            chk0 += val as usize;
            if chk1 > 0xff {
                chk0 += 1;
                chk1 &= 0xff;
            }
            ans.push(val);

            i+= 1;
        }
        // we have the sector, now verify checksum
        assert!(idx==DATA_NIBS);
        twos = decode_62(bak_buf[idx+0], inv)?;
        nib2 = decode_62(bak_buf[idx+1], inv)?;
        nib1 = decode_62(bak_buf[idx+2], inv)?;
        nib0 = decode_62(bak_buf[idx+3], inv)?;
        let rdchk0 = (nib0 | ((twos << 6) & 0xc0)) as usize;
        let rdchk1 = (nib1 | ((twos << 4) & 0xc0)) as usize;
        let rdchk2 = (nib2 | ((twos << 2) & 0xc0)) as usize;
        if chk0 != rdchk0 || chk1 != rdchk1 || chk2 != rdchk2 {
            debug!("expect checksum {},{},{} got {},{},{}",chk0,chk1,chk2,rdchk0,rdchk1,rdchk2);
            return Err(NibbleError::BadChecksum);
        }
        return Ok(ans);
    }
    /// Decode the sector using the scheme for this track.
    /// Assumes bit pointer is at the end of the address epilog.
    /// Returned data includes 12 byte header.
    fn decode_sector(&mut self) -> Result<Vec<u8>,NibbleError> {
        trace!("decoding sector");
        // Find data prolog without looking ahead too far, for if it does not exist, we
        // are to interpret the sector as empty.
        let prolog = self.dat_fmt.prolog.clone();
        let pmask = self.dat_fmt.prolog_mask.clone();
        let epilog = self.dat_fmt.epilog.clone();
        let emask = self.dat_fmt.epilog_mask.clone();
        let mut sec: [u8;1] = [0;1];
        if let Some(_shift) = self.find_byte_pattern(&prolog,&pmask,Some(40)) {
            trace!("data field found");
            // sector number occurs here as well as in address fields
            self.read_latch(&mut sec,1);
            let ans = self.decode_sector_62()?;
            if self.find_byte_pattern(&epilog,&emask,Some(10))==None {
                // emit a warning, but still accept the data
                warn!("data epilog not found");
            }
            return Ok(ans);
        } else {
            return Ok([0;SECTOR_SIZE].to_vec());
        }
    }
    /// Add `num` n-bit sync-bytes to the track, where n = `num_bits`.
    /// For DOS 3.3 and compatible track formats, n=10, for DOS 3.2 and compatible n=9.
    fn write_sync_gap(&mut self,num: usize,num_bits: usize) {
        for _i in 0..num {
            self.write(&[0xff,0x00],num_bits);
        }
    }
}

impl super::TrackBits for TrackBits {
    fn len(&self) -> usize {
        self.buf.len()
    }
    fn bit_count(&self) -> usize {
        self.bit_count
    }
    fn reset(&mut self) {
        self.bit_ptr = 0;
    }
    fn get_bit_ptr(&self) -> usize {
        self.bit_ptr
    }
    fn read_sector(&mut self,track: u8,sector: u8) -> Result<Vec<u8>,NibbleError> {
        match self.find_sector([track,sector]) {
            Ok(_vol) => self.decode_sector(),
            Err(e) => Err(e)
        }
    }
    fn write_sector(&mut self,dat: &Vec<u8>,track: u8,sector: u8) -> Result<(),NibbleError> {
        match self.find_sector([track,sector]) {
            Ok(_vol) => Ok(self.encode_sector(sector,dat)),
            Err(e) => Err(e)
        }
    }
    fn to_buf(&self) -> Vec<u8> {
        self.buf.clone()
    }
    fn to_nibbles(&mut self) -> Vec<u8> {
        // dump exactly one revolution starting on an address prolog
        let mut ans: Vec<u8> = Vec::new();
        let mut byte: [u8;1] = [0;1];
        if self.find_byte_pattern(&self.adr_fmt.prolog.clone(), &self.adr_fmt.prolog_mask.clone(), None) == None {
            self.reset();
        } else {
            self.shift_rev(self.adr_fmt.prolog.len()*8);
        }
        let mut bit_count = 0;
        for _try in 0..self.buf.len()*2 {
            bit_count += self.read_latch(&mut byte,1);
            ans.push(byte[0]);
            if bit_count >= self.bit_count {
                break;
            }
        }
        return ans;
    }
}

/// create the inverse to the encoding table
pub fn invert_62() -> [u8;256] {
    let mut ans: [u8;256] = [INVALID_NIB_BYTE;256];
    for i in 0..64 {
        ans[DISK_BYTES_62[i] as usize] = i as u8;
    }
    return ans;
}

/// encode a 6-bit nibble as a disk-friendly u8
fn encode_62(nib6: u8) -> u8 {
    return DISK_BYTES_62[(nib6 & 0x3f) as usize];
}

/// decode a byte, returning a 6-bit nibble in a u8
pub fn decode_62(byte: u8,inv: [u8;256]) -> Result<u8,NibbleError> {
    match inv[byte as usize] {
        INVALID_NIB_BYTE => Err(NibbleError::InvalidByte),
        x => Ok(x)
    }
}

/// This creates a track including sync bytes, address fields, nibbles, checksums, etc..
pub fn create_track(track: u8,sides: u8,buf_len: usize,adr_fmt: SectorAddressFormat, dat_fmt: SectorDataFormat) -> Box<dyn super::TrackBits> {
    // assume interleave is the same in every zone
    let interleave = skew::get_phys_interleave(&skew::D35_PHYSICAL[0]) as u8;
    let (cyl,mut side,format) = match sides {
        1 => (track, 0, 0x00 + interleave),
        2 => (track/2, 0x20*(track%2) ,0x20 + interleave),
        _ => panic!("unexpected number of sides")
    };
    side += match cyl>=64 { true => 1, false => 0 };
    let zone = cyl as usize / 16;
    trace!("create cyl {}, side {:06b}, zone {}",cyl,side,zone);
    let sectors = ZONED_SECS_PER_TRACK[zone];
    let bit_count = TRACK_BITS[zone];
    let buf: Vec<u8> = vec![0;buf_len];
    let mut ans = TrackBits::create(buf,bit_count,sides as usize);
    ans.set_format_protocol(adr_fmt, dat_fmt);
    ans.write_sync_gap(SYNC_TRACK_HEADER,10);
    for sector_pos in 0..sectors {
        let sector = skew::D35_PHYSICAL[zone][sector_pos];
        // address field
        let chk = cyl ^ sector as u8 ^ side ^ format;
        ans.write(&adr_fmt.prolog,24);
        ans.write(&[encode_62(cyl%64)],8);
        ans.write(&[encode_62(sector as u8)],8);
        ans.write(&[encode_62(side)],8);
        ans.write(&[encode_62(format)],8);
        ans.write(&[encode_62(chk)],8);
        ans.write(&adr_fmt.epilog,16);
        // data segment
        ans.encode_sector(sector as u8,&[0;SECTOR_SIZE].to_vec());
        //sync gap
        ans.write_sync_gap(SYNC_CLOSE,10);
    }
    let mut obj: Box<dyn super::TrackBits> = Box::new(ans);
    obj.reset();
    return obj;
}

/// Convenient form of `create_track` for compatibility with ProDOS
pub fn create_std_track(track: u8,sides: u8,buf_len: usize) -> Box< dyn super::TrackBits> {
    return create_track(track,sides,buf_len,SectorAddressFormat::create_std(),SectorDataFormat::create_std());
}
