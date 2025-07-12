//! ## module for GCR tracks
//! 
//! This handles bit-level processing of a GCR encoded disk track.
//! Image types handled include WOZ, G64, and NIB.
//! Special handling can be triggered by elements of the `ZoneFormat` struct.
//! 
//! For the most part any notion of timing has to be imposed by the caller.
//! One exception is `next_mc3470_pulse`, which emulates latency by serving
//! real bits from a position that lags the fake bit position.

use crate::img::{NibbleError,FieldCode};
use super::{SectorKey,ZoneFormat};
use crate::bios::skew;
use crate::{STDRESULT,DYNERR};

mod woz_nibbles;
mod g64_nibbles;

const FAKE_BITS: [u8;32] = [1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 1, 1];



/// This is the main interface for interacting with any GCR track bits.
/// This should be kept as a lightweight object that is recreated whenever
/// the R/W head moves to a new track.  The format of the track can be
/// somewhat heavyweight, and will be passed in by reference as an argument.
pub struct TrackEngine {
    bit_count: usize,
    bit_ptr: usize,
    nib_filter: bool,
    head_window: u8,
    fake_bit_ptr: usize
}
impl TrackEngine {
    pub fn create(bit_count: usize,bit_ptr: usize,nib_filter: bool) -> Self {
        Self {
            bit_count,
            bit_ptr,
            nib_filter,
            head_window: 255,
            fake_bit_ptr: (chrono::Local::now().timestamp() % 32) as usize
        }
    }
    /// Rotate the disk ahead by `bit_shift` bits
    fn shift_fwd(&mut self,bit_shift: usize) {
        let mut ptr = self.bit_ptr;
        ptr += bit_shift;
        while ptr >= self.bit_count {
            ptr -= self.bit_count;
        }
        self.bit_ptr = ptr;
    }
    /// Rotate the disk back by `bit_shift` bits
    fn shift_rev(&mut self,bit_shift: usize) {
        let mut ptr = self.bit_ptr as i64;
        ptr -= bit_shift as i64;
        while ptr < 0 {
            ptr += self.bit_count as i64;
        }
        self.bit_ptr = ptr as usize;
    }
    /// Read bytes (8-bit nibble codes) through a soft latch. This is a simple model of the way
    /// nibbles are generated in Apple firmware (logic state sequencer) and software.
    /// The number of track bits that passed by is returned (not necessarily 8*bytes).
    /// Specific to Apple disks.
    fn read_latch(&mut self,bits: &[u8],data: &mut [u8],num_bytes: usize) -> usize {
        let mut bit_count: usize = 0;
        for byte in 0..num_bytes {
            // Nibble alignment falls out of this loop.  Just as in the real system, it
            // depends on streams of sync-bytes preceding address or data segments.
            for _try in 0..self.bit_count {
                bit_count += 1;
                if self.next_mc3470_pulse(bits)==1 {
                    break;
                }
            }
            let mut val: u8 = 1;
            for _bit in 0..7 {
                val = val*2 + self.next_mc3470_pulse(bits);
            }
            data[byte] = val;
            bit_count += 7;
        }
        return bit_count;
    }
    /// directly get the current bit and advance
    fn next(&mut self,bits: &[u8]) -> u8 {
        let i = self.bit_ptr/8;
        let b = 7 - (self.bit_ptr%8) as u8;
        self.shift_fwd(1);
        return (bits[i] >> b) & 1;
    }
    /// Get a bit and advance, with filtering that approximates the disk ][ analog board.
    /// This will emit the bit that precedes the current pointer to account for latency, unless the
    /// fake bit condition is triggered, in which case a fake bit is emitted.
    /// If a NIB track is detected the filter is bypassed.
    /// Specific to Apple disks.
    fn next_mc3470_pulse(&mut self,bits: &[u8]) -> u8 {
        // if NIB we cannot disturb alignment, so there musn't be any latency
        if self.nib_filter {
            return self.next(bits);
        }
        // if head window is untouched bring in the first bit
        if self.head_window > 0b00001111 {
            self.head_window = self.next(bits);
        }
        self.head_window = 0b00001111 & (self.head_window << 1) | self.next(bits);
        if self.head_window != 0 {
            return (self.head_window & 2) >> 1;
        } else {
            self.fake_bit_ptr = (self.fake_bit_ptr + 1) % 32;
            return FAKE_BITS[self.fake_bit_ptr];
        }
    }
    /// Directly load `num_bits` bits into a slice of packed bytes,
    /// remainder bits are left untouched.  Bit order is MSB to LSB.
    /// This should only be used for non-Apple tracks, or for copying tracks.
    fn read(&mut self,bits: &[u8],data: &mut [u8],num_bits: usize) {
        for i in 0..num_bits {
            let src_idx = self.bit_ptr/8;
            let src_rel_bit = 7 - (self.bit_ptr%8) as u8;
            let dst_idx = i/8;
            let dst_rel_bit = 7 - (i%8) as u8;
            let term = ((bits[src_idx] >> src_rel_bit) & 1) << dst_rel_bit;
            data[dst_idx] &= (1 << dst_rel_bit) ^ u8::MAX;
            data[dst_idx] |= term;
            self.shift_fwd(1);
        }
    }
    /// Bits are packed into a slice of bytes, only `num_bits` of them are unpacked and written,
    /// the rest are padding that is ignored.  Bit order is MSB to LSB.
    fn write(&mut self,bits: &mut [u8],data: &[u8],num_bits: usize) {
        for i in 0..num_bits {
            let dst_idx = self.bit_ptr/8;
            let dst_rel_bit = 7 - (self.bit_ptr%8) as u8;
            let src_idx = i/8;
            let src_rel_bit = 7 - (i%8) as u8;
            let term = ((data[src_idx] >> src_rel_bit) & 1) << dst_rel_bit;
            bits[dst_idx] &= (1 << dst_rel_bit) ^ u8::MAX;
            bits[dst_idx] |= term;
            self.shift_fwd(1);
        }
    }
    /// Skip over count of nibbles using method appropriate for the given nibble code
    fn skip_nibbles(&mut self,bits: &[u8],count: usize,nib_code: &FieldCode) {
        let mut data = vec![0;count];
        match *nib_code {
            FieldCode::WOZ(_) => {
                self.read_latch(bits,&mut data,count);
            },
            FieldCode::G64(_) => {
                self.read(bits,&mut data,10*count);
            }
            _ => (),
        }
    }
    /// Encode and write the data header (often empty)
    fn write_data_header(&mut self,bits: &mut [u8],header: &[u8],nib_code: &FieldCode) -> STDRESULT {
        for b in header {
            match nib_code {
                FieldCode::WOZ((4,4)) => {
                    let nibs = woz_nibbles::encode_44(*b);
                    self.write(bits,&nibs,16);
                },
                FieldCode::WOZ((5,3)) => {
                    let nibs = woz_nibbles::encode_53(*b);
                    self.write(bits,&[nibs],8);
                },
                FieldCode::WOZ((6,2)) => {
                    let nibs = woz_nibbles::encode_62(*b);
                    self.write(bits,&[nibs],8);
                },
                FieldCode::G64((5,4)) => {
                    let nibs = g64_nibbles::encode_g64(*b);
                    self.write(bits,&nibs,10);
                },
                _ => return Err(Box::new(NibbleError::NibbleType))
            }
        }
        Ok(())
    }
    /// Assuming bit pointer is at an address, return vector of decoded address bytes.
    fn decode_addr(&mut self,bits: &[u8],fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let addr_bytes = fmt.addr_seek_expr.len();
        let mut buf: [u8;2] = [0;2];
        match fmt.addr_nibs() {
            FieldCode::WOZ((4,4)) => {
                for _ in 0..addr_bytes {
                    self.read_latch(bits,&mut buf,2);
                    ans.push(woz_nibbles::decode_44(buf)?);
                }
                Ok(ans)
            },
            FieldCode::WOZ((5,3)) => {
                // probably academic
                for _ in 0..addr_bytes {
                    self.read_latch(bits,&mut buf,1);
                    ans.push(woz_nibbles::decode_53(buf[0])?);
                }
                Ok(ans)
            },
            FieldCode::WOZ((6,2)) => {
                for _ in 0..addr_bytes {
                    self.read_latch(bits,&mut buf,1);
                    ans.push(woz_nibbles::decode_62(buf[0])?);
                }
                Ok(ans)
            },
            FieldCode::G64((5,4)) => {
                for _ in 0..addr_bytes {
                    self.read(bits,&mut buf[0..],5);
                    self.read(bits,&mut buf[1..],5);
                    let nib1 = g64_nibbles::decode_g64(buf[0])?;
                    let nib2 = g64_nibbles::decode_g64(buf[1])?;
                    ans.push(((nib1 & 0x0f) << 4) | (nib2 & 0x0f) );
                }
                Ok(ans)
            }
            _ => Err(Box::new(NibbleError::NibbleType)),
        }
    }
    fn find_apple_byte_pattern(&mut self,bits: &[u8],patt: &[u8],mask: &[u8],cap: Option<usize>) -> Option<usize> {
        if patt.len()==0 {
            return Some(0);
        }
        let mut bit_count: usize = 0;
        let mut matches = 0;
        let mut test_byte: [u8;1] = [0;1];
        for tries in 0..bits.len() {
            if let Some(max) = cap {
                if tries>=max {
                    return None;
                }
            }
            bit_count += self.read_latch(bits,&mut test_byte,1);
            // important this code can start and stop matching on the same byte
            let new_start = test_byte[0] & mask[0] == patt[0] & mask[0];
            let continuing = test_byte[0] & mask[matches] == patt[matches] & mask[matches];
            if continuing {
                matches += 1;
            } else if new_start {
                matches = 1;
            } else {
                matches = 0;
            }
            if matches==patt.len() {
                return Some(bit_count);
            }
        }
        return None;
    }
    /// this only accepts the pattern if it imediately follows a sync marker
    fn find_g64_byte_pattern(&mut self,bits: &[u8],patt: &[u8],mask: &[u8],cap: Option<usize>) -> Option<usize> {
        if patt.len()==0 {
            return Some(0);
        }
        let mut synced = false;
        let mut bit_count: usize = 0;
        let mut high_count: usize = 0;
        let mut buf: [u8;2] = [0;2];
        'trying: for tries in 0..bits.len()*8 {
            if let Some(max) = cap {
                if tries >= max*8 {
                    return None;
                }
            }
            if !synced {
                let now = self.next(bits);
                bit_count += 1;
                match (high_count,now) {
                    (x,0) if x > 4 => synced = true,
                    (_,0) => high_count = 0,
                    _ => high_count += 1
                };
                if synced {
                    self.shift_rev(1);
                    bit_count -= 1;
                }
            } else {
                for i in 0..patt.len() {
                    self.read(bits,&mut buf[0..],5);
                    bit_count += 5;
                    let mut test = match g64_nibbles::decode_g64(buf[0]) {
                        Ok(val) => val*16,
                        Err(_) => {
                            synced = false;
                            high_count = 0;
                            continue 'trying;
                        }
                    };
                    self.read(bits,&mut buf[1..],5);
                    bit_count += 5;
                    match g64_nibbles::decode_g64(buf[1]) {
                        Ok(val) => test += val,
                        Err(_) => {
                            synced = false;
                            high_count = 0;
                            continue 'trying;
                        }
                    }
                    if test & mask[i] != patt[i] & mask[i] {
                        synced = false;
                        high_count = 0;
                        continue 'trying;
                    }
                }
                return Some(bit_count);
            }
        }
        return None;
    }
    /// Find the pattern using a sync strategy appropriate for `nib_code`.
    /// Give up after `cap` bytes have been collected, or after whole track is searched if `cap` is `None`.
    /// Low bits in `mask` will cause corresponding bits in `patt` to automatically match. `mask` must be as long as `patt`.
    /// If pattern is found return the number of bits by which pointer advanced, otherwise return None.
    fn find_byte_pattern(&mut self,bits: &[u8],patt: &[u8],mask: &[u8],cap: Option<usize>,nib_code: &FieldCode) -> Option<usize> {
        match nib_code {
            FieldCode::WOZ(_) => self.find_apple_byte_pattern(bits, patt, mask, cap),
            FieldCode::G64(_) => self.find_g64_byte_pattern(bits, patt, mask, cap),
            _ => None,
        }
    }
    /// Find the sector as identified by the address field for this `fmt`.
    /// Advance the bit pointer to the end of the address epilog, and return the decoded address, or an error.
    /// We do not go looking for the data prolog at this stage, because it may not exist.
    /// E.g., DOS 3.2 `INIT` will not write any data fields outside of the boot tracks.
    fn find_sector(&mut self,bits: &[u8],skey: &SectorKey,sec: u8,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        log::trace!("seeking sector {}",sec);
        // Copy search patterns
        let (adr_pro,adr_pro_mask) = fmt.get_marker(0);
        let (adr_epi,adr_epi_mask) = fmt.get_marker(1);
        // Loop over attempts to read a sector
        for _try in 0..32 {
            if let Some(_shift) = self.find_byte_pattern(bits,adr_pro,adr_pro_mask,None,&fmt.addr_nibs()) {
                let actual = self.decode_addr(bits,fmt)?;
                let diff = fmt.diff_addr(skey, sec, &actual)?;
                match diff.iter().max() {
                    Some(max) => if *max > 0 {
                        let expected = fmt.get_addr_for_seeking(skey,sec,&actual)?;
                        log::trace!("skip sector {} (expect {})",hex::encode(actual),hex::encode(expected));
                        continue;
                    },
                    None => {
                        log::error!("problem during address diff");
                        return Err(Box::new(NibbleError::SectorNotFound));
                    }
                };
                if self.find_byte_pattern(bits,adr_epi,adr_epi_mask,Some(10),&fmt.addr_nibs())==None {
                    log::warn!("missed address epilog");
                    continue;
                }
                log::trace!("found sector with {:?}",actual);
                return Ok(actual);
            } else {
                log::debug!("no address prolog found on track");
                return Err(Box::new(NibbleError::BadTrack));
            }
        }
        // We tried as many times as there could be sectors, sector is missing
        log::debug!("the sector address was never matched");
        return Err(Box::new(NibbleError::SectorNotFound));
    }
    /// Assuming the bit pointer is at sector data, write a 4-4 encoded sector.
    fn encode_sector_44(&mut self,bits: &mut [u8],dat: &[u8]) {
        for i in 0..dat.len() {
            self.write(bits,&woz_nibbles::encode_44(dat[i]),16);
        }
    }
    fn encode_sector_g64(&mut self,bits: &mut [u8],dat: &[u8]) {
        for i in 0..dat.len() {
            self.write(bits,&g64_nibbles::encode_g64(dat[i]),10);
        }
    }
    /// Assuming the bit pointer is at sector data, write a 5-3 encoded sector
    /// Should be called only by encode_sector.
    fn encode_sector_53(&mut self,bits: &mut [u8],dat: &[u8],chk_seed: u8,xfrm: &[[u8;2]]) -> STDRESULT {
        let bak_buf = woz_nibbles::encode_sector_53(dat, chk_seed,xfrm)?;
        Ok(self.write(bits,&bak_buf,bak_buf.len()*8))
    }
    /// Assuming the bit pointer is at sector data, write a 6-2 encoded sector.
    /// Should be called only by encode_sector.
    fn encode_sector_62(&mut self,bits: &mut [u8],dat: &[u8],chk_seed: [u8;3],xfrm: &[[u8;2]]) -> STDRESULT {
        let bak_buf = woz_nibbles::encode_sector_62(dat, chk_seed, xfrm)?;
        Ok(self.write(bits,&bak_buf,bak_buf.len()*8))
    }
    /// This writes sync bytes, prolog, data, and epilog for any GCR sector we handle.
    /// Assumes bit pointer is at the end of the address epilog.
    fn encode_sector(&mut self,bits: &mut [u8],header: &[u8],dat: &[u8],fmt: &ZoneFormat) -> STDRESULT {
        log::trace!("encoding sector");
        let (prolog,_) = fmt.get_marker(2);
        let (epilog,_) = fmt.get_marker(3);
        match fmt.data_nibs() {
            FieldCode::WOZ((4,4)) => {
                self.write_sync_gap(bits, 1, fmt);
                self.write(bits,prolog,8*prolog.len());
                self.write_data_header(bits,header,&fmt.data_nibs())?;
                self.encode_sector_44(bits,dat);
                self.write(bits,epilog,8*epilog.len());
                Ok(())
            },
            FieldCode::WOZ((5,3)) => {
                self.write_sync_gap(bits, 1, fmt);
                self.write(bits,prolog,8*prolog.len());
                self.write_data_header(bits,header,&fmt.data_nibs())?;
                self.encode_sector_53(bits,dat,0,&fmt.swap_nibs)?;
                self.write(bits,epilog,8*epilog.len());
                Ok(())
            },
            FieldCode::WOZ((6,2)) => {
                self.write_sync_gap(bits, 1, fmt);
                self.write(bits,prolog,8*prolog.len());
                self.write_data_header(bits,header,&fmt.data_nibs())?;
                self.encode_sector_62(bits,dat,[0;3],&fmt.swap_nibs)?;
                self.write(bits,epilog,8*epilog.len());
                Ok(())
            }, 
            FieldCode::G64((5,4)) => {
                self.write_sync_gap(bits, 1, fmt);
                for i in 0..prolog.len() {
                    self.write(bits,&g64_nibbles::encode_g64(prolog[i]),10);
                }
                self.write_data_header(bits,header,&fmt.data_nibs())?;
                self.encode_sector_g64(bits,dat);
                for i in 0..epilog.len() {
                    self.write(bits,&g64_nibbles::encode_g64(epilog[i]),10);
                }
                Ok(())
            },
            _ => Err(Box::new(NibbleError::NibbleType))
        }
    }
    /// Assuming the bit pointer is at sector data, decode from 4-4 and return the sector.
    fn decode_sector_44(&mut self,bits: &[u8],capacity: usize) -> Result<Vec<u8>,DYNERR> {
        let mut nibble: [u8;2] = [0;2];
        let mut ans = Vec::new();
        for _i in 0..capacity {
            self.read_latch(bits,&mut nibble,2);
            ans.push(woz_nibbles::decode_44(nibble)?);
        }
        Ok(ans)
    }
    /// Assuming the bit pointer is at sector data, decode from g64 and return the sector.
    fn decode_sector_g64(&mut self,bits: &[u8],capacity: usize) -> Result<Vec<u8>,DYNERR> {
        let mut nibble: [u8;2] = [0;2];
        let mut ans = Vec::new();
        for _i in 0..capacity {
            self.read(bits,&mut nibble[0..],5);
            self.read(bits,&mut nibble[1..],5);
            let nib1 = g64_nibbles::decode_g64(nibble[0])?;
            let nib2 = g64_nibbles::decode_g64(nibble[1])?;
            ans.push(nib1*16 + nib2);
        }
        Ok(ans)
    }
    /// Assuming the bit pointer is at sector data, decode from 5-3 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_53(&mut self,bits: &[u8],chk_seed: u8,verify_chk: bool,capacity: usize,xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
        let nib_count = match capacity {
            256 => 411,
            _  => return Err(Box::new(crate::img::Error::SectorAccess))
        };
        let mut nibs = vec![0;nib_count];
        self.read_latch(bits,&mut nibs,nib_count);
        woz_nibbles::decode_sector_53(&nibs, chk_seed, verify_chk, xfrm)
    }
    /// Assuming the bit pointer is at sector data, decode from 6-2 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_62(&mut self,bits: &[u8],chk_seed: [u8;3],verify_chk: bool,capacity: usize,xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
        let nib_count = match capacity {
            256 => 343,
            524 => 703,
            _ => return Err(Box::new(crate::img::Error::SectorAccess))
        };
        let mut nibs = vec![0;nib_count];
        self.read_latch(bits,&mut nibs,nib_count);
        woz_nibbles::decode_sector_62(&nibs, chk_seed, verify_chk,xfrm)
    }
    /// Decode the sector using the scheme for this track.
    /// Assumes bit pointer is at the end of the address epilog.
    fn decode_sector(&mut self,bits: &[u8],skey: &SectorKey,sec: u8,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        log::trace!("decoding sector");
        // Find data prolog without looking ahead too far, for if it does not exist, we
        // are to interpret the sector as empty.
        let (prolog,pmask) = fmt.get_marker(2);
        let (epilog,emask) = fmt.get_marker(3);
        let maybe_shift = self.find_byte_pattern(bits, prolog, pmask, Some(40), &fmt.data_nibs);
        let header = fmt.get_data_header(skey, sec)?;
        self.skip_nibbles(bits,header.len(),&fmt.data_nibs());
        let capacity = fmt.capacity(sec as usize);
        let dat = match (maybe_shift,fmt.data_nibs()) {
            (Some(_),FieldCode::WOZ((4,4))) => self.decode_sector_44(bits,capacity)?,
            (Some(_),FieldCode::WOZ((5,3))) => self.decode_sector_53(bits,0,true,capacity,&fmt.swap_nibs)?,
            (Some(_),FieldCode::WOZ((6,2))) => self.decode_sector_62(bits,[0;3],true,capacity,&fmt.swap_nibs)?,
            (Some(_),FieldCode::G64((5,4))) => self.decode_sector_g64(bits,capacity)?,
            (Some(_),_) => return Err(Box::new(NibbleError::NibbleType)),
            (None,_) => vec![0;capacity]
        };
        if self.find_byte_pattern(bits, epilog, emask, Some(10), &fmt.data_nibs).is_none() {
            // emit a warning, but still accept the data
            log::warn!("data epilog not found");
        }
        return Ok(dat);
    }
    /// Process data field to determine its size, may be a little faster than fully decoding.
    /// Assumes bit pointer is at the end of the address field.
    /// For the more elaborate nibbles (5&3, 6&2) the result will be a standard size or an error,
    /// but will also allow for a few unexpected header nibbles that are not counted in the result.
    fn get_sector_capacity(&mut self,bits: &[u8],fmt: &ZoneFormat) -> Result<usize,DYNERR> {
        // skip a few nibbles to make sure we get into a sync gap, specifics not important
        self.skip_nibbles(bits,3,&fmt.data_nibs());
        // Find data prolog without looking ahead too far.  No data field is an error, *except* for 5&3 data.
        // For DOS 3.2, no data field is treated as a sector of zeroes, so in this case return Ok(256).
        // N.b. protected disks can have things (e.g. 6&2 boot sector) embedded in an "empty" 5&3 data field.
        let (prolog,pmask) = fmt.get_marker(2);
        let (epilog,emask) = fmt.get_marker(3);
        if self.find_byte_pattern(bits, prolog, pmask, Some(40), &fmt.data_nibs).is_none() {
            return match fmt.data_nibs() {
                FieldCode::WOZ((5,3)) => {
                    log::trace!("pristine 5&3 sector (no data field)");
                    Ok(256)
                },
                _ => Err(Box::new(NibbleError::BitPatternNotFound))
            };
        }
        // scanning 2048 nibbles allows for 1024 byte 4&4 sectors, somewhat more for others
        for nib_count in 0..2048 {
            self.skip_nibbles(bits,1,&fmt.data_nibs());
            let save_pos = self.bit_ptr;
            if let Some(_) = self.find_byte_pattern(bits, epilog, emask, Some(epilog.len()), &fmt.data_nibs) {
                log::trace!("found data epilog at nibble {}",nib_count+1);
                return match (fmt.data_nibs(),nib_count+1) {
                    (FieldCode::WOZ((4,4)),x) => Ok(x/2),
                    (FieldCode::WOZ((5,3)),x) if x >= 411 && x < 415 => Ok(256),
                    (FieldCode::WOZ((6,2)),x) if x >= 343 && x < 347 => Ok(256),
                    (FieldCode::WOZ((6,2)),x) if x >= 703 && x < 707 => Ok(524),
                    (FieldCode::G64((5,4)),x) => Ok(x),
                    _ => Err(Box::new(NibbleError::NibbleType))
                }
            }
            self.bit_ptr = save_pos;
        }
        Err(Box::new(NibbleError::BitPatternNotFound))
    }
    pub fn bit_count(&self) -> usize {
        self.bit_count
    }
    fn reset(&mut self) {
        self.bit_ptr = 0;
    }
    pub fn get_bit_ptr(&self) -> usize {
        self.bit_ptr
    }
    /// Write `which` sync gap (0,1,2) given the `fmt`.
    fn write_sync_gap(&mut self,bits: &mut [u8],which: usize,fmt: &ZoneFormat) {
        let gap_bits = fmt.get_gap_bits(which).to_bytes();
        self.write(bits,&gap_bits,fmt.get_gap_bits(which).len());
    }
    pub fn read_sector(&mut self,bits: &[u8],skey: &SectorKey,sec: u8,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        self.find_sector(bits,skey,sec,fmt)?;
        self.decode_sector(bits,skey,sec,fmt)
    }
    /// This currently unwinds the WOZ read latency after finding the sector.  The result is prettier sync gaps.
    pub fn write_sector(&mut self,bits: &mut [u8],dat: &[u8],skey: &SectorKey,sec: u8,fmt: &ZoneFormat) -> Result<(),DYNERR> {
        let header = fmt.get_data_header(skey, sec)?;
        let quantum = fmt.capacity(sec as usize);
        self.find_sector(bits,skey,sec,fmt)?;
        match (fmt.data_nibs(),self.nib_filter) {
            (FieldCode::WOZ(_),false) => self.shift_rev(1),
            _ => {}
        }
        self.encode_sector(bits,&header,&crate::img::quantize_block(dat,quantum),fmt)
    }
    pub fn to_nibbles(&mut self,bits: &[u8],fmt: &ZoneFormat) -> Vec<u8> {
        // dump exactly one revolution starting on an address prolog
        let mut ans: Vec<u8> = Vec::new();
        let mut byte: [u8;1] = [0;1];
        let (patt,mask) = fmt.get_marker(0);
        if self.find_byte_pattern(bits, patt, mask, None, &fmt.addr_nibs()).is_none() {
            self.reset();
        } else {
            self.shift_rev(patt.len()*8);
        }
        let mut bit_count = 0;
        for _try in 0..bits.len()*2 {
            bit_count += self.read_latch(bits,&mut byte,1);
            ans.push(byte[0]);
            if bit_count >= self.bit_count {
                break;
            }
        }
        return ans;
    }
    pub fn chss_map(&mut self,bits: &[u8],fmt: &ZoneFormat) -> Result<Vec<[usize;4]>,DYNERR> {
        let mut bit_ptr_list: Vec<usize> = Vec::new();
        self.reset();
        let mut ans: Vec<[usize;4]> = Vec::new();
        let (patt,mask) = fmt.get_marker(0);
        for _try in 0..32 {
            if self.find_byte_pattern(bits,patt,mask,None,&fmt.addr_nibs()).is_some() {
                let addr = self.decode_addr(bits,fmt)?;
                if bit_ptr_list.contains(&self.bit_ptr) {
                    // if we have seen this one before we are done
                    return Ok(ans)
                }
                bit_ptr_list.push(self.bit_ptr);
                let chs = fmt.get_chs(&addr)?;
                log::trace!("scan sector {}",chs[2]);
                let capacity = self.get_sector_capacity(bits, fmt)?;
                ans.push([chs[0] as usize,chs[1] as usize,chs[2] as usize,capacity]);
            } else {
                return Err(Box::new(NibbleError::BitPatternNotFound));
            }
        }
        return Ok(ans);
    }
}

fn get_and_check_bit_count(buf_len: usize, fmt: &ZoneFormat) -> Result<usize,DYNERR> {
    let addr_nibs = match fmt.addr_nibs() {
        FieldCode::WOZ((4,4)) => 2*fmt.addr_fmt_expr.len(),
        _ => fmt.addr_fmt_expr.len()
    };
    let data_nibs = match (fmt.capacity(0),fmt.data_nibs()) {
        (256,FieldCode::WOZ((4,4))) => 512,
        (256,FieldCode::WOZ((5,3))) => 411,
        (256,FieldCode::WOZ((6,2))) => 343,
        (524,FieldCode::WOZ((6,2))) => 703,
        _ => return Err(Box::new(NibbleError::NibbleType))
    };
    let mut marker_nibs = 0;
    for i in 0..4 {
        marker_nibs += fmt.markers[i].key.len();
    }
    let sectors = fmt.sector_count();
    let gap_bits0 = fmt.get_gap_bits(0).len();
    let gap_bits1 = fmt.get_gap_bits(1).len();
    let gap_bits2 = fmt.get_gap_bits(2).len();
    let bit_count = gap_bits0 + sectors*(marker_nibs*8 + addr_nibs*8 + gap_bits1 + data_nibs*8 + gap_bits2);
    if bit_count > buf_len*8 {
        log::error!("track buffer could not accommodate the track");
        return Err(Box::new(NibbleError::BadTrack));
    }
    Ok(bit_count)
}

/// Create a GCR track based on the given ZoneFormat.
/// * skey - standard address components, in general will be transformed by fmt
/// * buf_len - length of the buffer in which track bits will be loaded (usually padded)
/// * fmt - defines the track format to be used
/// * nib_filter - set true only if this is a NIB image
/// * returns - (track buffer, TrackBits object).
/// There is some special handling to emulate the way different versions of Apple DOS would format the track.
pub fn format_track(skey: SectorKey, buf_len: usize, fmt: &ZoneFormat, nib_filter: bool) -> Result<(Vec<u8>,TrackEngine),DYNERR> {
    log::trace!("formatting track at {},{}",skey.cyl,skey.head);
    for i in 0..3 {
        log::trace!("sync gap {} {}",i,hex::encode(fmt.gaps[i].to_bytes()));
    }
    for i in 0..4 {
        log::trace!("marker {} {}",i,hex::encode(&fmt.markers[i].key));
    }
    let bit_count = get_and_check_bit_count(buf_len, fmt)?;
    let sectors = fmt.sector_count();
    let mut bits: Vec<u8> = match nib_filter {
        false => vec![0;buf_len], // WOZ
        true => vec![0xff;buf_len] // NIB
    };
    let mut ans = TrackEngine::create(bit_count,0,nib_filter);
    ans.write_sync_gap(&mut bits,0,fmt);
    for theta in 0..sectors {
        // address field
        let sec = match sectors {
            // DOS 3.2 skews the sectors directly on the disk track
            13 => skew::DOS32_PHYSICAL[theta] as u8,
            // DOS 3.3 writes addresses in physical order, skew is in software
            _ => u8::try_from(theta)?
        };
        log::trace!("formatting angle {} id {}",theta,sec);
        let addr = fmt.get_addr_for_formatting(&skey,sec)?;
        log::trace!("address {}",hex::encode(&addr));
        let prolog = fmt.get_marker(0).0;
        let epilog = fmt.get_marker(1).0;
        match fmt.addr_nibs() {
            FieldCode::WOZ((4,4)) => {
                ans.write(&mut bits,prolog,prolog.len()*8);
                for i in 0..addr.len() {
                    ans.write(&mut bits,&woz_nibbles::encode_44(addr[i]),16);
                }
                ans.write(&mut bits,epilog,epilog.len()*8);
            },
            FieldCode::WOZ((5,3)) => {
                ans.write(&mut bits,prolog,prolog.len()*8);
                for i in 0..addr.len() {
                    ans.write(&mut bits,&[woz_nibbles::encode_53(addr[i])],8);
                }
                ans.write(&mut bits,epilog,epilog.len()*8);
            },
            FieldCode::WOZ((6,2)) => {
                ans.write(&mut bits,prolog,prolog.len()*8);
                for i in 0..addr.len() {
                    ans.write(&mut bits,&[woz_nibbles::encode_62(addr[i])],8);
                }
                ans.write(&mut bits,epilog,epilog.len()*8);
            },
            FieldCode::G64((5,4)) => {
                // For G64 we also encode the markers
                for i in 0..prolog.len() {
                    ans.write(&mut bits,&g64_nibbles::encode_g64(prolog[i]),10);
                }
                for i in 0..addr.len() {
                    ans.write(&mut bits,&g64_nibbles::encode_g64(addr[i]),10);
                }
                for i in 0..epilog.len() {
                    ans.write(&mut bits,&g64_nibbles::encode_g64(epilog[i]),10);
                }
            },
            _ => {
                return Err(Box::new(NibbleError::NibbleType));
            },
        }
        // data segment
        match (fmt.data_nibs(),fmt.capacity(sec as usize)) {

            (FieldCode::WOZ((5,3)),256) => {
                // special handling for DOS 3.2, the data segment is *not* created, but instead
                // the required space is filled with 0xff
                ans.write_sync_gap(&mut bits,1,fmt);
                ans.write(&mut bits,&[0xff;417],417*8);
            },
            (_,capacity) => {
                let header = fmt.get_data_header(&skey, sec)?;
                let dat = vec![0;capacity];
                ans.encode_sector(&mut bits,&header,&dat,fmt)?;
            }
        }
        //sync gap
        ans.write_sync_gap(&mut bits,2,fmt);
    }
    ans.reset();
    Ok((bits,ans))
}

/// Decode the given value using the given nibble code, if not valid return error.
/// Panics if nibble code is not handled.
pub fn decode(val: usize,nib_code: &FieldCode) -> Result<u8,DYNERR> {
    let b = val.to_le_bytes();
    match nib_code {
        FieldCode::WOZ((4,4)) => woz_nibbles::decode_44([b[1],b[0]]),
        FieldCode::WOZ((5,3)) => woz_nibbles::decode_53(b[0]),
        FieldCode::WOZ((6,2)) => woz_nibbles::decode_62(b[0]),
        _ => panic!("nibble code not handled")
    }
}