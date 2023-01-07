//! ## Low level treatment of 5.25 inch floppy disks
//! 
//! This handles the detailed track layout of a real floppy disk.
//! 
//! //! It should be noted the logic state sequencer (LSS) that is used in a real Apple computer
//! is approximated by a "soft latch" which collects bytes one bit at a time, obeying the rule
//! that leading low-bits must be dropped.
//! 
//! Acknowledgment: some of this module is adapted from CiderPress.

use super::NibbleError;
use log::{debug,trace,warn};
use crate::bios::skew;

const INVALID_NIB_BYTE: u8 = 0xff;
const CHUNK53: usize = 0x33;
const CHUNK62: usize = 0x56;

const DISK_BYTES_53: [u8;32] = [
    0xab, 0xad, 0xae, 0xaf, 0xb5, 0xb6, 0xb7, 0xba,
    0xbb, 0xbd, 0xbe, 0xbf, 0xd6, 0xd7, 0xda, 0xdb,
    0xdd, 0xde, 0xdf, 0xea, 0xeb, 0xed, 0xee, 0xef,
    0xf5, 0xf6, 0xf7, 0xfa, 0xfb, 0xfd, 0xfe, 0xff
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

#[derive(PartialEq,Clone,Copy)]
enum NibbleType {
    Enc44,
    Enc53,
    Enc62
}

/// How to find and read the sector address fields
#[derive(Clone,Copy)]
pub struct SectorAddressFormat {
    prolog: [u8;3],
    epilog: [u8;3],
    chk_seed: u8,
    verify_chk: bool,
    verify_track: bool,
    prolog_mask: [u8;3],
    epilog_mask: [u8;3]
}

impl SectorAddressFormat {
    pub fn create_std16() -> Self {
        Self {
            prolog: [0xd5,0xaa,0x96],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            verify_track: true,
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff,0x00]
        }
    }
    pub fn create_std13() -> Self {
        Self {
            prolog: [0xd5,0xaa,0xb5],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            verify_track: true,
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff,0x00]
        }
    }
}

/// How to find and read the sector data
#[derive(Clone,Copy)]
pub struct SectorDataFormat {
    prolog: [u8;3],
    epilog: [u8;3],
    chk_seed: u8,
    verify_chk: bool,
    nib: NibbleType,
    prolog_mask: [u8;3],
    epilog_mask: [u8;3]
}

impl SectorDataFormat {
    pub fn create_std16() -> Self {
        Self {
            prolog: [0xd5,0xaa,0xad],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            nib: NibbleType::Enc62,
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff,0x00]
        }
    }
    pub fn create_std13() -> Self {
        Self {
            prolog: [0xd5,0xaa,0xad],
            epilog: [0xde,0xaa,0xeb],
            chk_seed: 0x00,
            verify_chk: true,
            nib: NibbleType::Enc53,
            prolog_mask: [0xff,0xff,0xff],
            epilog_mask: [0xff,0xff,0x00]
        }
    }
}

/// This is the main interface for interacting with a realistic 5.25 inch disk.
/// This represents a track at the level of bits.
/// Writing to the track is at the bit stream level, any bit pattern will be accepted.
/// Reading can be done by direct bit stream consumption, or through a soft latch.
/// The underlying `Vec<u8>` is exposed only upon construction, any padding is determined at this stage.
/// This will also behave as a cyclic buffer to reflect a circular track.
pub struct TrackBits {
    adr_fmt: SectorAddressFormat,
    dat_fmt: SectorDataFormat,
    bit_count: usize,
    bit_ptr: usize,
    buf: Vec<u8>
}
impl TrackBits {
    /// Create an empty track with formatting protocol (but no actual format).
    /// Use `disk525::create_track`, or variants, to actually format the track.
    pub fn create(buf: Vec<u8>,bit_count: usize,adr_fmt: SectorAddressFormat,dat_fmt: SectorDataFormat) -> Self {
        if bit_count > buf.len()*8 {
            panic!("buffer cannot hold requested bits");
        }
        Self {
            adr_fmt,
            dat_fmt,
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
    /// Read bytes through a soft latch, this mocks up the way the hardware reads bytes.
    /// The number of track bits that passed by is returned (not necessarily 8*bytes)
    pub fn read_latch(&mut self,data: &mut [u8],num_bytes: usize) -> usize {
        let mut bit_count: usize = 0;
        for byte in 0..num_bytes {
            loop {
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
    /// Assuming bit pointer is at an address, return tuple with (vol,track,sector,chksum)
    fn decode_addr(&mut self) -> (u8,u8,u8,u8) {
        let mut buf: [u8;8] = [0;8];
        self.read_latch(&mut buf,8);
        return (
            decode_44([buf[0],buf[1]]),
            decode_44([buf[2],buf[3]]),
            decode_44([buf[4],buf[5]]),
            decode_44([buf[6],buf[7]])
        );
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
    /// Advance the bit pointer to the end of the address epilog, and return the volume number, or an error.
    /// We do not go looking for the data prolog at this stage, because it may not exist.
    /// E.g., DOS 3.2 `INIT` will not write any data fields outside of the boot tracks.
    fn find_sector(&mut self,ts: [u8;2]) -> Result<u8,NibbleError> {
        // Copy search patterns
        let adr_pro = self.adr_fmt.prolog.clone();
        let adr_pro_mask = self.adr_fmt.prolog_mask.clone();
        let adr_epi = self.adr_fmt.epilog.clone();
        let adr_epi_mask = self.adr_fmt.epilog_mask.clone();
        // Loop over attempts to read a sector
        for _try in 0..32 {
            if let Some(_shift) = self.find_byte_pattern(&adr_pro,&adr_pro_mask,None) {
                let (vol,track,sector,chksum) = self.decode_addr();
                let chk = self.adr_fmt.chk_seed ^ vol ^ track ^ sector ^ chksum;
                if self.adr_fmt.verify_track && track!=ts[0] {
                    warn!("track mismatch (want {}, got {})",ts[0],track);
                    continue;
                }
                if self.adr_fmt.verify_chk && chk != 0 {
                    warn!("checksum nonzero ({})",chk);
                    continue;
                }
                if self.find_byte_pattern(&adr_epi,&adr_epi_mask,Some(10))==None {
                    warn!("missed address epilog");
                    continue;
                }
                // we have a good header
                if ts[1] != sector {
                    trace!("skip sector {}, wait for {},{}",sector,ts[0],ts[1]);
                    continue;
                }
                return Ok(vol);
            } else {
                debug!("no address prolog found on track");
                return Err(NibbleError::BadTrack);
            }
        }
        // We tried as many times as there could be sectors, sector is missing
        debug!("the sector address was never matched");
        return Err(NibbleError::SectorNotFound);
    }
    /// Assuming the bit pointer is at sector data, write a 5-3 encoded sector
    /// Should be called only by encode_sector.
    fn encode_sector_53(&mut self,dat: &Vec<u8>) {
        // first work with bytes; adapted from CiderPress `EncodeNibble53`
        let mut bak_buf: [u8;411] = [0;411];
        let mut top: [u8;256] = [0;256];
        let mut threes: [u8;154] = [0;154];
        for i in 0..CHUNK53 {
            let offset = CHUNK53-1-i;
            top[offset+CHUNK53*0] = dat[i*5+0] >> 3;
            top[offset+CHUNK53*1] = dat[i*5+1] >> 3;
            top[offset+CHUNK53*2] = dat[i*5+2] >> 3;
            top[offset+CHUNK53*3] = dat[i*5+3] >> 3;
            top[offset+CHUNK53*4] = dat[i*5+4] >> 3;
            threes[offset+CHUNK53*0] =
                (dat[i*5+0] & 0x07) << 2 | (dat[i*5+3] & 0x04) >> 1 | (dat[i*5+4] & 0x04) >> 2;
            threes[offset+CHUNK53*1] =
                (dat[i*5+1] & 0x07) << 2 | (dat[i*5+3] & 0x02) >> 0 | (dat[i*5+4] & 0x02) >> 1;
            threes[offset+CHUNK53*2] =
                (dat[i*5+2] & 0x07) << 2 | (dat[i*5+3] & 0x01) << 1 | (dat[i*5+4] & 0x01) >> 0;
        }
        // last byte is different
        top[255] = dat[255] >> 3;
        threes[153] = dat[255] & 0x07;
        // fill backing buffer while computing checksum
        let mut chksum = self.dat_fmt.chk_seed;
        let mut idx = 0;
        for i in (0..threes.len()).rev() {
            bak_buf[idx] = encode_53(threes[i] ^ chksum);
            chksum = threes[i];
            idx += 1;
        }
        for i in 0..top.len() {
            bak_buf[idx] = encode_53(top[i] ^ chksum);
            chksum = top[i];
            idx += 1;
        }
        bak_buf[idx] = encode_53(chksum);
        // now copy the bits into the track from the backing buffer
        self.write(&bak_buf,411*8);
    }
    /// Assuming the bit pointer is at sector data, write a 6-2 encoded sector.
    /// Should be called only by encode_sector.
    fn encode_sector_62(&mut self,dat: &Vec<u8>) {
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
        let mut chksum = self.dat_fmt.chk_seed;
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
        self.write(&bak_buf,343*8);
    }
    /// This writes sync bytes, prolog, data, and epilog.
    /// Assumes bit pointer is at the end of the address epilog.
    /// This function is allowed to panic.
    fn encode_sector(&mut self,dat: &Vec<u8>) {
        let dat_fmt = self.dat_fmt;
        match dat_fmt.nib {
            NibbleType::Enc44 => panic!("only 5-3 or 6-2 nibbles allowed in data"),
            NibbleType::Enc53 => {
                self.write_sync_gap(10,9);
                self.write(&dat_fmt.prolog,24);
                self.encode_sector_53(dat);
                self.write(&dat_fmt.epilog,24);
            },
            NibbleType::Enc62 => {
                self.write_sync_gap(10,10);
                self.write(&dat_fmt.prolog,24);
                self.encode_sector_62(dat);
                self.write(&dat_fmt.epilog,24);
            }
        }
    }
    /// Assuming the bit pointer is at sector data, decode from 5-3 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_53(&mut self) -> Result<Vec<u8>,NibbleError> {
        let mut ans: Vec<u8> = Vec::new();
        // First get the bits into an ordinary byte-aligned buffer
        let mut bak_buf: [u8;411] = [0;411];
        self.read_latch(&mut bak_buf,411);
        // Now decode; adaptation from CiderPress `DecodeNibble53`
        let mut base: [u8;256] = [0;256];
        let mut threes: [u8;154] = [0;154];
        let mut chksum = self.dat_fmt.chk_seed;
        let inv = invert_53();
        let mut idx = 0;
        for i in (0..threes.len()).rev() {
            let val = decode_53(bak_buf[idx], inv);
            if val==INVALID_NIB_BYTE {
                return Err(NibbleError::InvalidByte);
            }
            chksum ^= val;
            threes[i] = chksum;
            idx += 1;
        }
        for i in 0..base.len() {
            let val = decode_53(bak_buf[idx],inv);
            if val==INVALID_NIB_BYTE {
                return Err(NibbleError::InvalidByte);
            }
            chksum ^= val;
            base[i] = chksum << 3;
            idx += 1;
        }
        // get chksum byte (index 411) and verify
        let val = decode_53(bak_buf[idx],inv);
        if val==INVALID_NIB_BYTE {
            return Err(NibbleError::InvalidByte);
        }
        chksum ^= val;
        if self.dat_fmt.verify_chk && chksum!=0 {
            return Err(NibbleError::BadChecksum);
        }
        // assemble the decoded data
        for i in (0..CHUNK53).rev() {
            let three1 = threes[CHUNK53*0+i];
            let three2 = threes[CHUNK53*1+i];
            let three3 = threes[CHUNK53*2+i];
            let three4 = (three1 & 0x02) << 1 | (three2 & 0x02) | (three3 & 0x02) >> 1;
            let three5 = (three1 & 0x01) << 2 | (three2 & 0x01) << 1 | (three3 & 0x01);

            ans.push(base[CHUNK53*0+i] | ((three1 >> 2) & 0x07));
            ans.push(base[CHUNK53*1+i] | ((three2 >> 2) & 0x07));
            ans.push(base[CHUNK53*2+i] | ((three3 >> 2) & 0x07));
            ans.push(base[CHUNK53*3+i] | ((three4 >> 0) & 0x07));
            ans.push(base[CHUNK53*4+i] | ((three5 >> 0) & 0x07));
        }
        ans.push(base[255] | (threes[threes.len()-1] & 0x07));
        return Ok(ans);
    }
    /// Assuming the bit pointer is at sector data, decode from 6-2 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_62(&mut self) -> Result<Vec<u8>,NibbleError> {
        let mut ans: Vec<u8> = Vec::new();
        // First get the bits into an ordinary byte-aligned buffer
        let mut bak_buf: [u8;343] = [0;343];
        self.read_latch(&mut bak_buf,343);
        // Now decode; direct adaptation from CiderPress `DecodeNibble62`
        let mut twos: [u8;CHUNK62 as usize*3] = [0;CHUNK62 as usize*3];
        let mut chksum = self.dat_fmt.chk_seed;
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
        if self.dat_fmt.verify_chk && chksum!=0 {
            return Err(NibbleError::BadChecksum)
        }
        return Ok(ans);
    }
    /// Decode the sector using the scheme for this track.
    /// Assumes bit pointer is at the end of the address epilog.
    fn decode_sector(&mut self) -> Result<Vec<u8>,NibbleError> {
        // Find data prolog without looking ahead too far, for if it does not exist, we
        // are to interpret the sector as empty.
        let prolog = self.dat_fmt.prolog.clone();
        let mask = self.dat_fmt.prolog_mask.clone();
        if let Some(_shift) = self.find_byte_pattern(&prolog,&mask,Some(40)) {
            trace!("data field found");
            return match self.dat_fmt.nib {
                NibbleType::Enc44 => Err(NibbleError::NibbleType),
                NibbleType::Enc53 => self.decode_sector_53(),
                NibbleType::Enc62 => self.decode_sector_62()
            };
        } else {
            return Ok([0;256].to_vec());
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
            Ok(_vol) => Ok(self.encode_sector(dat)),
            Err(e) => Err(e)
        }
    }
    fn to_buf(&self) -> Vec<u8> {
        self.buf.clone()
    }
    fn to_bytes(&mut self) -> Vec<u8> {
        // TODO: dump exactly one revolution starting on an address prolog
        let mut ans: Vec<u8> = Vec::new();
        let mut byte: [u8;1] = [0;1];
        self.reset();
        for _i in 0..self.len() {
            self.read_latch(&mut byte,1);
            ans.push(byte[0]);
            if self.bit_ptr+8 > self.bit_count {
                break;
            }
        }
        return ans;
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
pub fn decode_44(nibs: [u8;2]) -> u8 {
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

/// This creates a track including sync bytes, address fields, nibbles, checksums, etc..
/// For 13 sector disks, data segments are filled with high bits.
/// For 16 sector disks, the data segment is created, and the data itself is zeroed.
pub fn create_track(vol: u8,track: u8,buf_len: usize,adr_fmt: SectorAddressFormat, dat_fmt: SectorDataFormat) -> Box<dyn super::TrackBits> {
    let bit_count_13 = 40*9 + 13*((3+8+3)*8 + 10*9 + (3+411+3)*8 + 20*9);
    let bit_count_16 = 40*10 + 16*((3+8+3)*8 + 10*10 + (3+343+3)*8 + 20*10);
    let (sectors,bit_count,sync_bits) = match dat_fmt.nib {
        NibbleType::Enc53 => (13,bit_count_13,9),
        NibbleType::Enc62 => (16,bit_count_16,10),
        _ => panic!("only 5-3 or 6-2 nibbles allowed")
    };
    let buf: Vec<u8> = vec![0;buf_len];
    let mut ans = TrackBits::create(buf,bit_count,adr_fmt,dat_fmt);
    ans.write_sync_gap(40,sync_bits);
    for sector in 0..sectors {
        // address field
        ans.write(&adr_fmt.prolog,24);
        ans.write(&encode_44(vol),16);
        ans.write(&encode_44(track),16);
        let sec_addr = match sectors {
            // DOS 3.2 skews the sectors directly on the disk track
            13 => skew::DOS32_PHYSICAL[sector] as u8,
            // DOS 3.3 writes addresses in physical order, skew is in software
            _ => sector as u8
        };
        ans.write(&encode_44(sec_addr),16);
        let chksum = adr_fmt.chk_seed ^ vol ^ track ^ sec_addr;
        ans.write(&encode_44(chksum),16);
        ans.write(&adr_fmt.epilog,24);
        // data segment
        match sectors {
            13 => {
                ans.write_sync_gap(10,sync_bits);
                ans.write(&[0xff;417],417*8);
            },
            _ => {
                ans.encode_sector(&[0;256].to_vec());
            }
        }
        //sync gap
        ans.write_sync_gap(20,sync_bits);
    }
    let mut obj: Box<dyn super::TrackBits> = Box::new(ans);
    obj.reset();
    return obj;
}

/// Convenient form of `create_track` for compatibility with DOS 3.3 and ProDOS
pub fn create_std16_track(vol: u8,track: u8,buf_len: usize) -> Box<dyn super::TrackBits> {
    debug!("create 16 sectors on track {}",track);
    return create_track(vol,track,buf_len,SectorAddressFormat::create_std16(),SectorDataFormat::create_std16());
}

/// Convenient form of `create_track` for compatibility with DOS 3.0, 3.1, and 3.2
pub fn create_std13_track(vol: u8,track: u8,buf_len: usize) -> Box<dyn super::TrackBits> {
    debug!("create 13 sectors on track {}",track);
    return create_track(vol,track,buf_len,SectorAddressFormat::create_std13(),SectorDataFormat::create_std13());
}

