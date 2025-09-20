//! ## module for GCR tracks
//! 
//! This handles bit-level processing of a GCR encoded disk track.
//! Image types handled include WOZ, G64, and NIB.
//! Special handling can be triggered by elements of the `ZoneFormat` struct.
//! 
//! There is an assumed tick normalization for each kind of disk:
//! * Apple 5.25 inch - 125000 ps
//! * Apple 3.5 inch - 62500 ps

use crate::img::{Error,FieldCode,SectorHood,Sector};
use super::{ZoneFormat,Method,FluxCells};
use crate::bios::skew;
use crate::{STDRESULT,DYNERR};

mod woz_nibbles;
mod g64_nibbles;
mod woz_state_machine;

#[derive(Clone)]
struct SaveState {
    ptr: usize,
    time: u64,
    lss: (usize,u64,u8)
}

/// This is the main interface for interacting with any GCR track data.
/// It maintains the state of the controller, but does not own either
/// the track data or the format information.
pub struct TrackEngine {
    nib_filter: bool,
    method: Method,
    woz_state: woz_state_machine::State,
}
impl TrackEngine {
    /// create a new track engine, any machine state is lost
    pub fn create(method: Method,nib_filter: bool) -> Self {
        let mut woz_state = woz_state_machine::State::new();
        match method {
            Method::Emulate => woz_state.enable_fake_bits(),
            _ => woz_state.disable_fake_bits()
        }
        Self {
            nib_filter,
            method,
            woz_state
        }
    }
    /// change the method used process the data stream
    pub fn change_method(&mut self,method: Method) {
        match method {
            Method::Emulate => self.woz_state.enable_fake_bits(),
            _ => self.woz_state.disable_fake_bits()
        }
        self.method = method;
    }
    fn save_state(&self, cells: &FluxCells) -> SaveState {
        SaveState { ptr: cells.ptr, time: cells.time, lss: self.woz_state.get_critical_state() }
    }
    fn restore_state(&mut self, cells: &mut FluxCells, state: &SaveState) {
        cells.ptr = state.ptr;
        cells.time = state.time;
        self.woz_state.restore_critical_state(state.lss);
    }
    /// Read specified number of 8-bit codes as they are loaded into the data latch.
    /// The number of ticks that passed by is returned.  There are varying levels of fidelity
    /// (and expense) that the engine will use depending on its settings.
    /// Specific to Apple disks.
    fn read_woz(&mut self,cells: &mut FluxCells,codes: &mut [u8],num_codes: usize) -> usize {
        // We need to have in mind some typical code that would read the disk, in order to run
        // the LSS with workable timings.  Here is how DOS 3.2 reads the secondary data buffer.
        // The primary buffer routine is similar (slightly longer buffering time).
        // beg      STY   $3C        ; 3
        // wait     LDY   $C08C,X    ; 4
        //          BPL   wait       ; 2
        //          EOR   $800,Y     ; 4
        //          LDY   $3C        ; 3
        //          DEY              ; 2
        //          STA   $800,Y     ; 5
        //          BNE   beg        ; 2
        // Writing has to be done every 32 microseconds, here is an example:
        //          LDA   #val       ; 2
        //          JSR   write      ; 6
        // write    CLC              ; 2
        //          PHA              ; 3
        //          PLA              ; 4
        //          STA   $C08D,X    ; 5
        //          ORA   $C08C,X    ; 4
        //          RTS              ; 6
        if self.nib_filter {
            self.read(cells,codes,num_codes*8);
            return num_codes << 3 << cells.bshift;
        }
        let latch_reps = 1000;
        let loop_time = 6*8; // time spent in `wait` loop
        let buf_time = 19*8; // time spent buffering and re-entering `wait` loop
        let flux = cells.bshift > cells.fshift;
        match (flux,&self.method) {
            (_,Method::Fast) | (false,Method::Auto) => {
                let mut code_count = 0;
                let mut tick_count = 0;
                let mut reps = 0;
                while code_count < num_codes && reps < latch_reps*num_codes {
                    if cells.read_bit() {
                        self.read(cells,&mut codes[code_count..],7);
                        codes[code_count] = 0x80 + (codes[code_count] >> 1);
                        code_count += 1;
                        tick_count += 7 << cells.bshift;
                    }
                    tick_count += 1 << cells.bshift;
                    reps += 1;
                }
                tick_count
            },
            (_,Method::Analyze) => {
                let mut tick_count: usize = 0;
                self.woz_state.start_read();
                for byte in 0..num_codes {
                    for _try in 0..latch_reps {
                        let touched = self.woz_state.advance(loop_time, cells);
                        tick_count += loop_time;
                        codes[byte] = self.woz_state.get_latch();
                        if codes[byte] & 0x80 > 0 && touched {
                            break;
                        }
                    }
                    self.woz_state.advance(buf_time,cells);
                    tick_count += buf_time;
                }
                tick_count
            },
            (_,Method::Emulate) | (true,Method::Auto) => {
                let mut tick_count: usize = 0;
                self.woz_state.start_read();
                for byte in 0..num_codes {
                    for _try in 0..latch_reps {
                        self.woz_state.advance(loop_time, cells);
                        tick_count += loop_time;
                        codes[byte] = self.woz_state.get_latch();
                        if codes[byte] & 0x80 > 0 {
                            break;
                        }
                    }
                    self.woz_state.advance(buf_time,cells);
                    tick_count += buf_time;
                }
                tick_count
            }
        }
    }
    /// Direct loading from flux-cells into bytes, multiple transitions in a bit-cell resolve as high.
    fn read(&mut self,cells: &mut FluxCells,data: &mut [u8],num_bits: usize) {
        for i in 0..num_bits {
            let pulse = cells.read_bit();
            let dst_idx = i >> 3;
            let dst_rel_bit = 7 - (i & 7) as u8;
            data[dst_idx] &= (1 << dst_rel_bit) ^ u8::MAX;
            data[dst_idx] |= (pulse as u8) << dst_rel_bit;
        }
    }
    /// Direct transfer of bits to the flux-cells.
    fn write(&mut self,cells: &mut FluxCells,data: &[u8],num_bits: usize) {
        for i in 0..num_bits {
            let src_idx = i >> 3;
            let src_rel_bit = 7 - (i & 7) as u8;
            let pulse = (data[src_idx] >> src_rel_bit) & 1;
            cells.write_bit(pulse>0);
        }
    }
    /// Skip over count of nibbles using method appropriate for the given nibble code
    fn skip_nibbles(&mut self,cells: &mut FluxCells,count: usize,nib_code: &FieldCode) {
        let mut data = vec![0;count];
        match *nib_code {
            FieldCode::WOZ(_) => {
                self.read_woz(cells,&mut data,count);
            },
            FieldCode::G64(_) => {
                self.read(cells,&mut data,10*count);
            }
            _ => (),
        }
    }
    /// Encode and write the data header (often empty)
    fn write_data_header(&mut self,cells: &mut FluxCells,header: &[u8],nib_code: &FieldCode) -> STDRESULT {
        for b in header {
            match nib_code {
                FieldCode::WOZ((4,4)) => {
                    let nibs = woz_nibbles::encode_44(*b);
                    self.write(cells,&nibs,16);
                },
                FieldCode::WOZ((5,3)) => {
                    let nibs = woz_nibbles::encode_53(*b);
                    self.write(cells,&[nibs],8);
                },
                FieldCode::WOZ((6,2)) => {
                    let nibs = woz_nibbles::encode_62(*b);
                    self.write(cells,&[nibs],8);
                },
                FieldCode::G64((5,4)) => {
                    let nibs = g64_nibbles::encode_g64(*b);
                    self.write(cells,&nibs,10);
                },
                _ => return Err(Box::new(Error::NibbleType))
            }
        }
        Ok(())
    }
    /// Assuming bit pointer is at an address, return vector of decoded address bytes.
    fn decode_addr(&mut self,cells: &mut FluxCells,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        let mut ans = Vec::new();
        let addr_bytes = fmt.addr_seek_expr.len();
        let mut buf: [u8;2] = [0;2];
        match fmt.addr_nibs() {
            FieldCode::WOZ((4,4)) => {
                for _ in 0..addr_bytes {
                    self.read_woz(cells,&mut buf,2);
                    ans.push(woz_nibbles::decode_44(buf)?);
                }
                Ok(ans)
            },
            FieldCode::WOZ((5,3)) => {
                // probably academic
                for _ in 0..addr_bytes {
                    self.read_woz(cells,&mut buf,1);
                    ans.push(woz_nibbles::decode_53(buf[0])?);
                }
                Ok(ans)
            },
            FieldCode::WOZ((6,2)) => {
                for _ in 0..addr_bytes {
                    self.read_woz(cells,&mut buf,1);
                    ans.push(woz_nibbles::decode_62(buf[0])?);
                }
                Ok(ans)
            },
            FieldCode::G64((5,4)) => {
                for _ in 0..addr_bytes {
                    self.read(cells,&mut buf[0..],5);
                    self.read(cells,&mut buf[1..],5);
                    let nib1 = g64_nibbles::decode_g64(buf[0])?;
                    let nib2 = g64_nibbles::decode_g64(buf[1])?;
                    ans.push(((nib1 & 0x0f) << 4) | (nib2 & 0x0f) );
                }
                Ok(ans)
            }
            _ => Err(Box::new(Error::NibbleType)),
        }
    }
    fn find_apple_byte_pattern(&mut self,cells: &mut FluxCells,patt: &[u8],mask: &[u8],cap: Option<usize>) -> Option<SaveState> {
        let mut beg_state = self.save_state(cells);
        if patt.len()==0 {
            return Some(beg_state);
        }
        let mut matches = 0;
        let mut test_byte: [u8;1] = [0;1];
        for tries in 0..cells.revolution >> cells.bshift >> 2 {
            if let Some(max) = cap {
                if tries>=max {
                    return None;
                }
            }
            let new_state = self.save_state(cells);
            self.read_woz(cells,&mut test_byte,1);
            // important this code can start and stop matching on the same byte
            let new_start = test_byte[0] & mask[0] == patt[0] & mask[0];
            let continuing = matches > 0 && (test_byte[0] & mask[matches] == patt[matches] & mask[matches]);
            if continuing {
                matches += 1;
            } else if new_start {
                matches = 1;
                beg_state = new_state;
            } else {
                matches = 0;
            }
            if matches==patt.len() {
                return Some(beg_state);
            }
        }
        return None;
    }
    /// this only accepts the pattern if it imediately follows a sync marker
    fn find_g64_byte_pattern(&mut self,cells: &mut FluxCells,patt: &[u8],mask: &[u8],cap: Option<usize>) -> Option<SaveState> {
        let mut beg_state = self.save_state(cells);
        if patt.len()==0 {
            return Some(beg_state);
        }
        let mut synced = false;
        let mut high_count: usize = 0;
        let mut buf: [u8;2] = [0;2];
        'trying: for tries in 0..cells.stream.len() {
            if let Some(max) = cap {
                if tries >= max*8 {
                    return None;
                }
            }
            if !synced {
                let now = cells.read_bit() as u8;
                match (high_count,now) {
                    (x,0) if x > 4 => synced = true,
                    (_,0) => high_count = 0,
                    _ => high_count += 1
                };
                if synced {
                    cells.rev(1 << cells.bshift);
                }
            } else {
                beg_state = self.save_state(cells);
                for i in 0..patt.len() {
                    self.read(cells,&mut buf[0..],5);
                    let mut test = match g64_nibbles::decode_g64(buf[0]) {
                        Ok(val) => val*16,
                        Err(_) => {
                            synced = false;
                            high_count = 0;
                            continue 'trying;
                        }
                    };
                    self.read(cells,&mut buf[1..],5);
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
                return Some(beg_state);
            }
        }
        return None;
    }
    /// Find the pattern using a sync strategy appropriate for `nib_code`.
    /// Give up after `cap` bytes have been collected, or after whole track is searched if `cap` is `None`.
    /// Low bits in `mask` will cause corresponding bits in `patt` to automatically match. `mask` must be as long as `patt`.
    /// If pattern is found, return the state just prior to finding it, otherwise return None.
    fn find_byte_pattern(&mut self,cells: &mut FluxCells,patt: &[u8],mask: &[u8],cap: Option<usize>,nib_code: &FieldCode) -> Option<SaveState> {
        match nib_code {
            FieldCode::WOZ(_) => self.find_apple_byte_pattern(cells, patt, mask, cap),
            FieldCode::G64(_) => self.find_g64_byte_pattern(cells, patt, mask, cap),
            _ => None,
        }
    }
    /// Find the sector as identified by the address field for this `fmt`.
    /// Advance the bit pointer to the end of the address epilog, and return the decoded address, or an error.
    /// We do not go looking for the data prolog at this stage, because it may not exist.
    /// E.g., DOS 3.2 `INIT` will not write any data fields outside of the boot tracks.
    fn find_sector(&mut self,cells: &mut FluxCells,hood: &SectorHood,sec: &Sector,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        log::trace!("seeking sector {}, method {}",sec,&self.method);
        // Copy search patterns
        let (adr_pro,adr_pro_mask) = fmt.get_marker(0);
        let (adr_epi,adr_epi_mask) = fmt.get_marker(1);
        // Loop over attempts to read a sector
        for _try in 0..32 {
            if let Some(_shift) = self.find_byte_pattern(cells,adr_pro,adr_pro_mask,None,&fmt.addr_nibs()) {
                let actual = self.decode_addr(cells,fmt)?;
                let diff = fmt.diff_addr(hood, sec, &actual)?;
                match diff.iter().max() {
                    Some(max) => if *max > 0 {
                        let expected = fmt.get_addr_for_seeking(hood, sec, &actual)?;
                        log::trace!("skip sector {} (expect {})",hex::encode(actual),hex::encode(expected));
                        continue;
                    },
                    None => {
                        log::error!("problem during address diff");
                        return Err(Box::new(Error::SectorNotFound));
                    }
                };
                if self.find_byte_pattern(cells,adr_epi,adr_epi_mask,Some(10),&fmt.addr_nibs()).is_none() {
                    log::warn!("missed address epilog");
                    continue;
                }
                log::trace!("found sector with {:?}",actual);
                return Ok(actual);
            } else {
                log::debug!("no address prolog found on track");
                return Err(Box::new(Error::SectorNotFound));
            }
        }
        // We tried as many times as there could be sectors, sector is missing
        log::debug!("the sector address was never matched");
        return Err(Box::new(Error::SectorNotFound));
    }
    /// Assuming the bit pointer is at sector data, write a 4-4 encoded sector.
    fn encode_sector_44(&mut self,cells: &mut FluxCells,dat: &[u8]) {
        for i in 0..dat.len() {
            self.write(cells,&woz_nibbles::encode_44(dat[i]),16);
        }
    }
    fn encode_sector_g64(&mut self,cells: &mut FluxCells,dat: &[u8]) {
        for i in 0..dat.len() {
            self.write(cells,&g64_nibbles::encode_g64(dat[i]),10);
        }
    }
    /// Assuming the bit pointer is at sector data, write a 5-3 encoded sector
    /// Should be called only by encode_sector.
    fn encode_sector_53(&mut self,cells: &mut FluxCells,dat: &[u8],chk_seed: u8,xfrm: &[[u8;2]]) -> STDRESULT {
        let bak_buf = woz_nibbles::encode_sector_53(dat, chk_seed,xfrm)?;
        Ok(self.write(cells,&bak_buf,bak_buf.len()*8))
    }
    /// Assuming the bit pointer is at sector data, write a 6-2 encoded sector.
    /// Should be called only by encode_sector.
    fn encode_sector_62(&mut self,cells: &mut FluxCells,dat: &[u8],chk_seed: [u8;3],xfrm: &[[u8;2]]) -> STDRESULT {
        let bak_buf = woz_nibbles::encode_sector_62(dat, chk_seed, xfrm)?;
        Ok(self.write(cells,&bak_buf,bak_buf.len()*8))
    }
    /// This writes sync bytes, prolog, data, and epilog for any GCR sector we handle.
    /// Assumes bit pointer is at the end of the address epilog.
    fn encode_sector(&mut self,cells: &mut FluxCells,header: &[u8],dat: &[u8],fmt: &ZoneFormat) -> STDRESULT {
        log::trace!("encoding sector");
        let (prolog,_) = fmt.get_marker(2);
        let (epilog,_) = fmt.get_marker(3);
        match fmt.data_nibs() {
            FieldCode::WOZ((4,4)) => {
                self.write_sync_gap(cells, 1, fmt);
                self.write(cells,prolog,8*prolog.len());
                self.write_data_header(cells,header,&fmt.data_nibs())?;
                self.encode_sector_44(cells,dat);
                self.write(cells,epilog,8*epilog.len());
                Ok(())
            },
            FieldCode::WOZ((5,3)) => {
                self.write_sync_gap(cells, 1, fmt);
                self.write(cells,prolog,8*prolog.len());
                self.write_data_header(cells,header,&fmt.data_nibs())?;
                self.encode_sector_53(cells,dat,0,&fmt.swap_nibs)?;
                self.write(cells,epilog,8*epilog.len());
                Ok(())
            },
            FieldCode::WOZ((6,2)) => {
                self.write_sync_gap(cells, 1, fmt);
                self.write(cells,prolog,8*prolog.len());
                self.write_data_header(cells,header,&fmt.data_nibs())?;
                self.encode_sector_62(cells,dat,[0;3],&fmt.swap_nibs)?;
                self.write(cells,epilog,8*epilog.len());
                Ok(())
            }, 
            FieldCode::G64((5,4)) => {
                self.write_sync_gap(cells, 1, fmt);
                for i in 0..prolog.len() {
                    self.write(cells,&g64_nibbles::encode_g64(prolog[i]),10);
                }
                self.write_data_header(cells,header,&fmt.data_nibs())?;
                self.encode_sector_g64(cells,dat);
                for i in 0..epilog.len() {
                    self.write(cells,&g64_nibbles::encode_g64(epilog[i]),10);
                }
                Ok(())
            },
            _ => Err(Box::new(Error::NibbleType))
        }
    }
    /// Assuming the bit pointer is at sector data, decode from 4-4 and return the sector.
    fn decode_sector_44(&mut self,cells: &mut FluxCells,capacity: usize) -> Result<Vec<u8>,DYNERR> {
        let mut nibble: [u8;2] = [0;2];
        let mut ans = Vec::new();
        for _i in 0..capacity {
            self.read_woz(cells,&mut nibble,2);
            ans.push(woz_nibbles::decode_44(nibble)?);
        }
        Ok(ans)
    }
    /// Assuming the bit pointer is at sector data, decode from g64 and return the sector.
    fn decode_sector_g64(&mut self,cells: &mut FluxCells,capacity: usize) -> Result<Vec<u8>,DYNERR> {
        let mut nibble: [u8;2] = [0;2];
        let mut ans = Vec::new();
        for _i in 0..capacity {
            self.read(cells,&mut nibble[0..],5);
            self.read(cells,&mut nibble[1..],5);
            let nib1 = g64_nibbles::decode_g64(nibble[0])?;
            let nib2 = g64_nibbles::decode_g64(nibble[1])?;
            ans.push(nib1*16 + nib2);
        }
        Ok(ans)
    }
    /// Assuming the bit pointer is at sector data, decode from 5-3 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_53(&mut self,cells: &mut FluxCells,chk_seed: u8,verify_chk: bool,capacity: usize,xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
        let nib_count = match capacity {
            256 => 411,
            _  => return Err(Box::new(crate::img::Error::GeometryMismatch))
        };
        let mut nibs = vec![0;nib_count];
        self.read_woz(cells,&mut nibs,nib_count);
        woz_nibbles::decode_sector_53(&nibs, chk_seed, verify_chk, xfrm)
    }
    /// Assuming the bit pointer is at sector data, decode from 6-2 and return the sector.
    /// Should only be called by decode_sector.
    fn decode_sector_62(&mut self,cells: &mut FluxCells,chk_seed: [u8;3],verify_chk: bool,capacity: usize,xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
        let nib_count = match capacity {
            256 => 343,
            524 => 703,
            _ => return Err(Box::new(crate::img::Error::GeometryMismatch))
        };
        let mut nibs = vec![0;nib_count];
        self.read_woz(cells,&mut nibs,nib_count);
        woz_nibbles::decode_sector_62(&nibs, chk_seed, verify_chk,xfrm)
    }
    /// Decode the sector using the scheme for this track.
    /// Assumes bit pointer is at the end of the address epilog.
    fn decode_sector(&mut self,cells: &mut FluxCells,hood: &SectorHood,sec: &Sector,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        log::trace!("decoding sector");
        // Find data prolog without looking ahead too far, for if it does not exist, we
        // are to interpret the sector as empty.
        let (prolog,pmask) = fmt.get_marker(2);
        let (epilog,emask) = fmt.get_marker(3);
        let maybe_shift = self.find_byte_pattern(cells, prolog, pmask, Some(40), &fmt.data_nibs);
        let header = fmt.get_data_header(hood, sec)?;
        self.skip_nibbles(cells,header.len(),&fmt.data_nibs());
        let capacity = fmt.capacity(&sec);
        let dat = match (maybe_shift,fmt.data_nibs()) {
            (Some(_),FieldCode::WOZ((4,4))) => self.decode_sector_44(cells,capacity)?,
            (Some(_),FieldCode::WOZ((5,3))) => self.decode_sector_53(cells,0,true,capacity,&fmt.swap_nibs)?,
            (Some(_),FieldCode::WOZ((6,2))) => self.decode_sector_62(cells,[0;3],true,capacity,&fmt.swap_nibs)?,
            (Some(_),FieldCode::G64((5,4))) => self.decode_sector_g64(cells,capacity)?,
            (Some(_),_) => return Err(Box::new(Error::NibbleType)),
            (None,_) => vec![0;capacity]
        };
        if self.find_byte_pattern(cells, epilog, emask, Some(10), &fmt.data_nibs).is_none() {
            // emit a warning, but still accept the data
            log::warn!("data epilog not found");
        }
        return Ok(dat);
    }
    /// Process data field to determine its size, may be a little faster than fully decoding.
    /// Assumes bit pointer is at the end of the address field.
    /// For the more elaborate nibbles (5&3, 6&2) the result will be a standard size or an error,
    /// but will also allow for a few unexpected header nibbles that are not counted in the result.
    fn get_sector_capacity(&mut self,cells: &mut FluxCells,fmt: &ZoneFormat) -> Result<usize,DYNERR> {
        // skip a few nibbles to make sure we get into a sync gap, specifics not important
        //self.skip_nibbles(cells,3,&fmt.data_nibs());
        // Find data prolog without looking ahead too far.  No data field is an error, *except* for 5&3 data.
        // For DOS 3.2, no data field is treated as a sector of zeroes, so in this case return Ok(256).
        // N.b. protected disks can have things (e.g. 6&2 boot sector) embedded in an "empty" 5&3 data field.
        let (prolog,pmask) = fmt.get_marker(2);
        let (epilog,emask) = fmt.get_marker(3);
        if self.find_byte_pattern(cells, prolog, pmask, Some(40), &fmt.data_nibs).is_none() {
            return match fmt.data_nibs() {
                FieldCode::WOZ((5,3)) => {
                    log::trace!("pristine 5&3 sector (no data field)");
                    Ok(256)
                },
                _ => {
                    log::trace!("data prolog {} was not found",hex::encode(prolog));
                    Err(Box::new(Error::BitPatternNotFound))
                }
            };
        }
        // scanning 2048 nibbles allows for 1024 byte 4&4 sectors, somewhat more for others
        for nib_count in 0..2048 {
            self.skip_nibbles(cells,1,&fmt.data_nibs());
            let state = self.save_state(cells);
            if let Some(_) = self.find_byte_pattern(cells, epilog, emask, Some(epilog.len()), &fmt.data_nibs) {
                log::trace!("found data epilog at nibble {}",nib_count+1);
                return match (fmt.data_nibs(),nib_count+1) {
                    (FieldCode::WOZ((4,4)),x) => Ok(x/2),
                    (FieldCode::WOZ((5,3)),x) if x >= 411 && x < 415 => Ok(256),
                    (FieldCode::WOZ((6,2)),x) if x >= 343 && x < 347 => Ok(256),
                    (FieldCode::WOZ((6,2)),x) if x >= 703 && x < 707 => Ok(524),
                    (FieldCode::G64((5,4)),x) => Ok(x),
                    _ => Err(Box::new(Error::NibbleType))
                }
            }
            self.restore_state(cells,&state);
        }
        Err(Box::new(Error::BitPatternNotFound))
    }
    /// Write `which` sync gap (0,1,2) given the `fmt`.
    fn write_sync_gap(&mut self,cells: &mut FluxCells,which: usize,fmt: &ZoneFormat) {
        let gap_bits = fmt.get_gap_bits(which).to_bytes();
        self.write(cells,&gap_bits,fmt.get_gap_bits(which).len());
    }
    pub fn read_sector(&mut self,cells: &mut FluxCells,hood: &SectorHood,sec: &Sector,fmt: &ZoneFormat) -> Result<Vec<u8>,DYNERR> {
        self.find_sector(cells,hood,sec,fmt)?;
        self.decode_sector(cells,hood,sec,fmt)
    }
    pub fn write_sector(&mut self,cells: &mut FluxCells,dat: &[u8],hood: &SectorHood,sec: &Sector,fmt: &ZoneFormat) -> Result<(),DYNERR> {
        let header = fmt.get_data_header(hood, sec)?;
        let quantum = fmt.capacity(sec);
        self.find_sector(cells,hood,sec,fmt)?;
        // in some cases writing a zero before proceeding is needed to prevent bad splices
        match (fmt.data_nibs(),self.nib_filter) {
            (FieldCode::WOZ(_),false) => self.write(cells,&vec![0],1),
            _ => {}
        }
        self.encode_sector(cells,&header,&crate::img::quantize_block(dat,quantum),fmt)
    }
    /// dump nibble stream starting on an address prolog continuing through one revolution
    pub fn to_nibbles(&mut self,cells: &mut FluxCells,fmt: &ZoneFormat) -> Vec<u8> {
        let mut ans: Vec<u8> = Vec::new();
        let mut byte: [u8;1] = [0;1];
        let (patt,mask) = fmt.get_marker(0);
        cells.ptr = 0;
        match self.find_byte_pattern(cells, patt, mask, None, &fmt.addr_nibs()) {
            None => cells.ptr = 0,
            Some(state) => self.restore_state(cells,&state)
        };
        let mut tick_count = 0;
        for _try in 0..cells.revolution >> cells.bshift >> 2 {
            tick_count += self.read_woz(cells,&mut byte,1);
            ans.push(byte[0]);
            if tick_count >= cells.revolution {
                break;
            }
        }
        return ans;
    }
    /// return vectors of sector addresses and sizes in geometric order for user consumption
    pub fn get_sector_map(&mut self,cells: &mut FluxCells,fmt: &ZoneFormat) -> Result<(Vec<[u8;6]>,Vec<usize>),DYNERR> {
        let mut first_found: Option<usize> = None;
        let tolerance = 2 << cells.bshift;
        cells.ptr = 0;
        let mut addr_map: Vec<[u8;6]> = Vec::new();
        let mut size_map: Vec<usize> = Vec::new();
        let (patt,mask) = fmt.get_marker(0);
        log::trace!("seek {}, mask {}, method {}",hex::encode(patt),hex::encode(mask),&self.method);
        for i in 0..32 {
            log::trace!("seeking angle {}",i);
            if self.find_byte_pattern(cells,patt,mask,None,&fmt.addr_nibs()).is_some() {
                let mut addr = self.decode_addr(cells,fmt)?;
                log::trace!("found {}",hex::encode(&addr));
                match first_found {
                    None => first_found = Some(cells.ptr),
                    Some(ptr) => {
                        if cells.ptr + tolerance > ptr && cells.ptr < ptr + tolerance {
                            // we have seen this before, so we are done
                            return Ok((addr_map,size_map))
                        }
                    }
                }
                while addr.len() < 6 {
                    addr.push(0);
                }
                let capacity = self.get_sector_capacity(cells, fmt)?;
                addr_map.push([addr[0],addr[1],addr[2],addr[3],addr[4],addr[5]]);
                size_map.push(capacity);
            } else {
                return Err(Box::new(Error::BitPatternNotFound));
            }
        }
        return Ok((addr_map,size_map));
    }
    /// Create a GCR track based on the given ZoneFormat (currently Apple only).
    /// * hood - standard address components, in general will be transformed by fmt
    /// * buf_len - length of the buffer in which track bits will be loaded (usually padded, only used as a check)
    /// * fmt - defines the track format to be used
    /// * returns - FluxCells object.
    /// There is some special handling to emulate the way different versions of Apple DOS would format the track.
    pub fn format_track(&mut self, hood: SectorHood, buf_len: usize, fmt: &ZoneFormat) -> Result<FluxCells,DYNERR> {
        log::trace!("formatting track at {},{}",hood.cyl,hood.head);
        for i in 0..3 {
            log::trace!("sync gap {} {}",i,hex::encode(fmt.gaps[i].to_bytes()));
        }
        for i in 0..4 {
            log::trace!("marker {} {}",i,hex::encode(&fmt.markers[i].key));
        }
        let double_speed = match fmt.speed_kbps {
            500 => true,
            _ => false
        };
        let bit_count = get_and_check_bit_count(buf_len, fmt)?;
        let sectors = fmt.sector_count();
        let mut cells = FluxCells::new_woz_bits(0, bit_vec::BitVec::from_elem(bit_count,self.nib_filter),0,double_speed);
        self.write_sync_gap(&mut cells,0,fmt);
        for theta in 0..sectors {
            // address field
            let sec = match sectors {
                // DOS 3.2 skews the sectors directly on the disk track
                13 => skew::DOS32_PHYSICAL[theta] as u8,
                // DOS 3.3 writes addresses in physical order, skew is in software
                _ => u8::try_from(theta)?
            };
            log::trace!("formatting angle {} id {}",theta,sec);
            let addr = fmt.get_addr_for_formatting(&hood,&Sector::Num(sec as usize))?;
            log::trace!("address {}",hex::encode(&addr));
            let prolog = fmt.get_marker(0).0;
            let epilog = fmt.get_marker(1).0;
            match fmt.addr_nibs() {
                FieldCode::WOZ((4,4)) => {
                    self.write(&mut cells,prolog,prolog.len()*8);
                    for i in 0..addr.len() {
                        self.write(&mut cells,&woz_nibbles::encode_44(addr[i]),16);
                    }
                    self.write(&mut cells,epilog,epilog.len()*8);
                },
                FieldCode::WOZ((5,3)) => {
                    self.write(&mut cells,prolog,prolog.len()*8);
                    for i in 0..addr.len() {
                        self.write(&mut cells,&[woz_nibbles::encode_53(addr[i])],8);
                    }
                    self.write(&mut cells,epilog,epilog.len()*8);
                },
                FieldCode::WOZ((6,2)) => {
                    self.write(&mut cells,prolog,prolog.len()*8);
                    for i in 0..addr.len() {
                        self.write(&mut cells,&[woz_nibbles::encode_62(addr[i])],8);
                    }
                    self.write(&mut cells,epilog,epilog.len()*8);
                },
                FieldCode::G64((5,4)) => {
                    // For G64 we also encode the markers
                    for i in 0..prolog.len() {
                        self.write(&mut cells,&g64_nibbles::encode_g64(prolog[i]),10);
                    }
                    for i in 0..addr.len() {
                        self.write(&mut cells,&g64_nibbles::encode_g64(addr[i]),10);
                    }
                    for i in 0..epilog.len() {
                        self.write(&mut cells,&g64_nibbles::encode_g64(epilog[i]),10);
                    }
                },
                _ => {
                    return Err(Box::new(Error::NibbleType));
                },
            }
            // data segment
            match (fmt.data_nibs(),fmt.capacity(&Sector::Num(sec as usize))) {

                (FieldCode::WOZ((5,3)),256) => {
                    // special handling for DOS 3.2, the data segment is *not* created, but instead
                    // the required space is filled with 0xff
                    self.write_sync_gap(&mut cells,1,fmt);
                    self.write(&mut cells,&[0xff;417],417*8);
                },
                (_,capacity) => {
                    let header = fmt.get_data_header(&hood, &Sector::Num(sec as usize))?;
                    let dat = vec![0;capacity];
                    self.encode_sector(&mut cells,&header,&dat,fmt)?;
                }
            }
            //sync gap
            self.write_sync_gap(&mut cells,2,fmt);
        }
        cells.ptr = 0;
        Ok(cells)
    }
}

fn get_and_check_bit_count(buf_len: usize, fmt: &ZoneFormat) -> Result<usize,DYNERR> {
    let addr_nibs = match fmt.addr_nibs() {
        FieldCode::WOZ((4,4)) => 2*fmt.addr_fmt_expr.len(),
        _ => fmt.addr_fmt_expr.len()
    };
    let data_nibs = match (fmt.capacity(&Sector::Num(0)),fmt.data_nibs()) {
        (256,FieldCode::WOZ((4,4))) => 512,
        (256,FieldCode::WOZ((5,3))) => 411,
        (256,FieldCode::WOZ((6,2))) => 343,
        (524,FieldCode::WOZ((6,2))) => 703,
        _ => return Err(Box::new(Error::NibbleType))
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
        return Err(Box::new(Error::InternalStructureAccess));
    }
    Ok(bit_count)
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