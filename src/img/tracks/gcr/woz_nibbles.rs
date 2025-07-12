//! Module for handling Steve Wozniak's nibbles
//! 
//! The nibbles handled by this module are a form of group code recording (GCR),
//! although Apple did not use that terminology for a long time.
//! It is more complicated than other GCR schemes: each 8-bit nibble is derived from
//! multiple bytes of data in a way that admits no simple expression; in particular,
//! each byte of data is scrambled across multiple non-contiguous nibbles.
//! It was done this way to make the disk II fast while also allowing for better
//! storage density than FM schemes.
//! 
//! Much of the code herein is a rust port of C++ code that appears in CiderPress 1.

use crate::DYNERR;
use crate::img::NibbleError;

const INVALID_NIB_BYTE: u8 = 0xff;
const CHUNK53: usize = 0x33;
const CHUNK62: usize = 0x56;

const FWD_53: [u8;32] = [
    0xab, 0xad, 0xae, 0xaf, 0xb5, 0xb6, 0xb7, 0xba,
    0xbb, 0xbd, 0xbe, 0xbf, 0xd6, 0xd7, 0xda, 0xdb,
    0xdd, 0xde, 0xdf, 0xea, 0xeb, 0xed, 0xee, 0xef,
    0xf5, 0xf6, 0xf7, 0xfa, 0xfb, 0xfd, 0xfe, 0xff
];

const FWD_62: [u8;64] = [
    0x96, 0x97, 0x9a, 0x9b, 0x9d, 0x9e, 0x9f, 0xa6,
    0xa7, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb2, 0xb3,
    0xb4, 0xb5, 0xb6, 0xb7, 0xb9, 0xba, 0xbb, 0xbc,
    0xbd, 0xbe, 0xbf, 0xcb, 0xcd, 0xce, 0xcf, 0xd3,
    0xd6, 0xd7, 0xd9, 0xda, 0xdb, 0xdc, 0xdd, 0xde,
    0xdf, 0xe5, 0xe6, 0xe7, 0xe9, 0xea, 0xeb, 0xec,
    0xed, 0xee, 0xef, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6,
    0xf7, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe, 0xff
];

const REV_53: [u8;256] = [
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0x01,0x02,0x03,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x04,0x05,0x06,0xFF,0xFF,0x07,0x08,0xFF,0x09,0x0A,0x0B,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x0C,0x0D,0xFF,0xFF,0x0E,0x0F,0xFF,0x10,0x11,0x12,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x13,0x14,0xFF,0x15,0x16,0x17,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x18,0x19,0x1A,0xFF,0xFF,0x1B,0x1C,0xFF,0x1D,0x1E,0x1F
];

const REV_62: [u8;256] = [
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0x01,0xFF,0xFF,0x02,0x03,0xFF,0x04,0x05,0x06,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x07,0x08,0xFF,0xFF,0xFF,0x09,0x0A,0x0B,0x0C,0x0D,
    0xFF,0xFF,0x0E,0x0F,0x10,0x11,0x12,0x13,0xFF,0x14,0x15,0x16,0x17,0x18,0x19,0x1A,
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x1B,0xFF,0x1C,0x1D,0x1E,
    0xFF,0xFF,0xFF,0x1F,0xFF,0xFF,0x20,0x21,0xFF,0x22,0x23,0x24,0x25,0x26,0x27,0x28,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x29,0x2A,0x2B,0xFF,0x2C,0x2D,0x2E,0x2F,0x30,0x31,0x32,
    0xFF,0xFF,0x33,0x34,0x35,0x36,0x37,0x38,0xFF,0x39,0x3A,0x3B,0x3C,0x3D,0x3E,0x3F
];

fn xfrm_read(val: u8, xfrm: &[[u8;2]]) -> u8 {
    for [i,j] in xfrm {
        if val == *i {
            return *j;
        }
    }
    val
}

fn xfrm_write(val: u8, xfrm: &[[u8;2]]) -> u8 {
    for [i,j] in xfrm {
        if val == *j {
            return *i;
        }
    }
    val
}

/// encode a normal byte as two 4&4 nibbles
pub fn encode_44(val: u8) -> [u8;2] {
    return [(val >> 1) | 0xaa, val | 0xaa];
}

/// decode two 4&4 nibbles as a normal byte, invalid nibble will yield error
pub fn decode_44(nibs: [u8;2]) -> Result<u8,DYNERR> {
    if nibs[0] & 0xaa != 0xaa || nibs[1] & 0xaa != 0xaa {
        Err(Box::new(NibbleError::InvalidByte))
    } else {
        Ok(((nibs[0] << 1) | 0x01) & nibs[1])
    }
}

/// encode a 5-bit value as a 5&3 nibble
pub fn encode_53(val: u8) -> u8 {
    return FWD_53[(val & 0x1f) as usize];
}

/// decode a 5&3 nibble as a 5-bit value, invalid nibble will yield error
pub fn decode_53(nib: u8) -> Result<u8,DYNERR> {
    let ans = REV_53[nib as usize];
    if ans == INVALID_NIB_BYTE {
        Err(Box::new(NibbleError::InvalidByte))
    } else {
        Ok(ans)
    }
}

/// encode a 6-bit value as a 6&2 nibble
pub fn encode_62(val: u8) -> u8 {
    return FWD_62[(val & 0x3f) as usize];
}

/// decode a 6&2 nibble as a 6-bit value, invalid nibble will yield error
pub fn decode_62(nib: u8) -> Result<u8,DYNERR> {
    let ans = REV_62[nib as usize];
    if ans == INVALID_NIB_BYTE {
        Err(Box::new(NibbleError::InvalidByte))
    } else {
        Ok(ans)
    }
}

pub fn encode_sector_53(dat: &[u8], chk_seed: u8, xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    if dat.len() == 256 {
        Ok(encode_sector_53_256(dat, chk_seed, xfrm))
    } else {
        Err(Box::new(NibbleError::NibbleType))
    }
}

pub fn decode_sector_53(nibs: &[u8], chk_seed: u8, verify_chk: bool, xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    if nibs.len() == 411 {
        decode_sector_53_256(nibs, chk_seed, verify_chk, xfrm)
    } else {
        Err(Box::new(NibbleError::NibbleType))
    }
}

pub fn encode_sector_62(dat: &[u8], chk_seed: [u8;3], xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    match dat.len() {
        256 => Ok(encode_sector_62_256(dat, chk_seed[0], xfrm)),
        524 => Ok(encode_sector_62_524(dat, chk_seed, xfrm)),
        _ => Err(Box::new(NibbleError::NibbleType))
    }
}

pub fn decode_sector_62(nibs: &[u8],chk_seed: [u8;3],verify_chk: bool,xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    match nibs.len() {
        343 => decode_sector_62_256(nibs, chk_seed[0], verify_chk, xfrm),
        703 => decode_sector_62_524(nibs, chk_seed, verify_chk, xfrm),
        _ => Err(Box::new(NibbleError::NibbleType))
    }
}

/// encode 256 bytes as 411 nibbles
fn encode_sector_53_256(dat: &[u8], chk_seed: u8, xfrm: &[[u8;2]]) -> Vec<u8> {
    // port of CiderPress `EncodeNibble53`
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
    let mut chksum = chk_seed;
    let mut idx = 0;
    for i in (0..threes.len()).rev() {
        bak_buf[idx] = encode_53(xfrm_write(threes[i] ^ chksum,xfrm));
        chksum = threes[i];
        idx += 1;
    }
    for i in 0..top.len() {
        bak_buf[idx] = encode_53(xfrm_write(top[i] ^ chksum,xfrm));
        chksum = top[i];
        idx += 1;
    }
    bak_buf[idx] = encode_53(xfrm_write(chksum,xfrm));
    bak_buf.to_vec()
}

/// encode 256 bytes as 343 nibbles
fn encode_sector_62_256(dat: &[u8], chk_seed: u8, xfrm: &[[u8;2]]) -> Vec<u8> {
    // port of CiderPress `EncodeNibble62`
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
    let mut chksum = chk_seed;
    let mut idx = 0;
    for i in (0..CHUNK62).rev() {
        bak_buf[idx] = encode_62(xfrm_write(twos[i] ^ chksum,xfrm));
        chksum = twos[i];
        idx += 1;
    }
    for i in 0..256 {
        bak_buf[idx] = encode_62(xfrm_write(top[i] ^ chksum,xfrm));
        chksum = top[i];
        idx += 1;
    }
    bak_buf[idx] = encode_62(xfrm_write(chksum,xfrm));
    bak_buf.to_vec()
}

/// decode 411 nibbles as 256 bytes
fn decode_sector_53_256(bak_buf: &[u8], chk_seed: u8, verify_chk: bool, xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    // port of CiderPress `DecodeNibble53`
    let mut ans: Vec<u8> = Vec::new();
    let mut base: [u8;256] = [0;256];
    let mut threes: [u8;154] = [0;154];
    let mut chksum = chk_seed;
    let mut idx = 0;
    for i in (0..threes.len()).rev() {
        let val = decode_53(xfrm_read(bak_buf[idx],xfrm))?;
        chksum ^= val;
        threes[i] = chksum;
        idx += 1;
    }
    for i in 0..base.len() {
        let val = decode_53(xfrm_read(bak_buf[idx],xfrm))?;
        chksum ^= val;
        base[i] = chksum << 3;
        idx += 1;
    }
    // get chksum byte (index 411) and verify
    let val = decode_53(xfrm_read(bak_buf[idx],xfrm))?;
    chksum ^= val;
    if verify_chk && chksum!=0 {
        return Err(Box::new(NibbleError::BadChecksum));
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

/// decode 343 nibbles as 256 bytes
fn decode_sector_62_256(bak_buf: &[u8], chk_seed: u8, verify_chk: bool, xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    // port of CiderPress `DecodeNibble62`
    let mut ans: Vec<u8> = Vec::new();
    let mut twos: [u8;CHUNK62 as usize*3] = [0;CHUNK62 as usize*3];
    let mut chksum = chk_seed;
    let mut idx = 0;
    for i in 0..CHUNK62 {
        let val = decode_62(xfrm_read(bak_buf[idx],xfrm))?;
        chksum ^= val;
        twos[i] = ((chksum & 0x01) << 1) | ((chksum & 0x02) >> 1);
        twos[i + CHUNK62] = ((chksum & 0x04) >> 1) | ((chksum & 0x08) >> 3);
        twos[i + CHUNK62*2] = ((chksum & 0x10) >> 3) | ((chksum & 0x20) >> 5);
        idx += 1;
    }
    for i in 0..256 {
        let val = decode_62(xfrm_read(bak_buf[idx],xfrm))?;
        chksum ^= val;
        ans.push((chksum << 2) | twos[i]);
        idx += 1;
    }
    // we have the sector, now verify checksum
    let val = decode_62(xfrm_read(bak_buf[idx],xfrm))?;
    chksum ^= val;
    if verify_chk && chksum!=0 {
        return Err(Box::new(NibbleError::BadChecksum));
    }
    return Ok(ans);
}

/// Tag bytes are included.
fn encode_sector_62_524(dat: &[u8], chk_seed: [u8;3], xfrm: &[[u8;2]]) -> Vec<u8> {
    // port of CiderPress `EncodeNibbleSector35`
    const SECTOR_SIZE: usize = 524; // 12 tag byte header + 512 data bytes
    const CHUNK62: usize = 175;
    const DATA_NIBS: usize = 699; // nibbles of data, checksum follows 
    const CHK_NIBS: usize = 4; // how many checksum nibbles after data
    assert!(dat.len()>=SECTOR_SIZE);
    let mut bak_buf: [u8;DATA_NIBS+CHK_NIBS] = [0;DATA_NIBS+CHK_NIBS];
    let mut part0: [u8;CHUNK62] = [0;CHUNK62];
    let mut part1: [u8;CHUNK62] = [0;CHUNK62];
    let mut part2: [u8;CHUNK62] = [0;CHUNK62];
    let [mut chk0,mut chk1,mut chk2]: [usize;3] = [
        chk_seed[0] as usize,
        chk_seed[1] as usize,
        chk_seed[2] as usize
    ];
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
            chk0 &= 0xff;
            chk1 &= 0xff;
            chk2 &= 0xff;
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
        bak_buf[i*4+0] = encode_62(xfrm_write(twos,xfrm));
        bak_buf[i*4+1] = encode_62(xfrm_write(part0[i] & 0x3f,xfrm));
        bak_buf[i*4+2] = encode_62(xfrm_write(part1[i] & 0x3f,xfrm));
        if i*4 + 3 < DATA_NIBS+CHK_NIBS {
            bak_buf[i*4+3] = encode_62(xfrm_write(part2[i] & 0x3f,xfrm));
        }
    }

    // checksum
    twos = (((chk0 & 0xc0) >> 6) | ((chk1 & 0xc0) >> 4) | ((chk2 & 0xc0) >> 2)) as u8;
    bak_buf[DATA_NIBS+0] = encode_62(xfrm_write(twos,xfrm));
    bak_buf[DATA_NIBS+1] = encode_62(xfrm_write(chk2 as u8 & 0x3f,xfrm));
    bak_buf[DATA_NIBS+2] = encode_62(xfrm_write(chk1 as u8 & 0x3f,xfrm));
    bak_buf[DATA_NIBS+3] = encode_62(xfrm_write(chk0 as u8 & 0x3f,xfrm));

    bak_buf.to_vec()
}

/// Tag bytes are included.
fn decode_sector_62_524(bak_buf: &[u8], chk_seed: [u8;3], verify_chk: bool, xfrm: &[[u8;2]]) -> Result<Vec<u8>,DYNERR> {
    // port of CiderPress `DecodeNibbleSector35`
    // sector size is 524: 12 tag byte header + 512 data bytes
    const CHUNK62: usize = 175;
    const DATA_NIBS: usize = 699; // nibbles of data, does not include 4 checksum bytes to follow 
    let mut ans: Vec<u8> = Vec::new();
    let [mut val,mut nib0,mut nib1,mut nib2,mut twos]: [u8;5];
    let mut part0: [u8;CHUNK62] = [0;CHUNK62];
    let mut part1: [u8;CHUNK62] = [0;CHUNK62];
    let mut part2: [u8;CHUNK62] = [0;CHUNK62];
    let mut idx = 0;
    for i in 0..CHUNK62 {
        twos = decode_62(xfrm_read(bak_buf[idx+0],xfrm))?;
        nib0 = decode_62(xfrm_read(bak_buf[idx+1],xfrm))?;
        nib1 = decode_62(xfrm_read(bak_buf[idx+2],xfrm))?;
        idx += 3;
        if i != CHUNK62-1 {
            nib2 = decode_62(xfrm_read(bak_buf[idx],xfrm))?;
            idx += 1;
        } else {
            nib2 = 0;
        }
        part0[i] = nib0 | ((twos << 2) & 0xc0);
        part1[i] = nib1 | ((twos << 4) & 0xc0);
        part2[i] = nib2 | ((twos << 6) & 0xc0);
    }

    let [mut chk0,mut chk1,mut chk2]: [usize;3] = [
        chk_seed[0] as usize,
        chk_seed[1] as usize,
        chk_seed[2] as usize
    ];
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
            chk0 &= 0xff;
            chk1 &= 0xff;
            chk2 &= 0xff;
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
    twos = decode_62(xfrm_read(bak_buf[idx+0],xfrm))?;
    nib2 = decode_62(xfrm_read(bak_buf[idx+1],xfrm))?;
    nib1 = decode_62(xfrm_read(bak_buf[idx+2],xfrm))?;
    nib0 = decode_62(xfrm_read(bak_buf[idx+3],xfrm))?;
    let rdchk0 = (nib0 | ((twos << 6) & 0xc0)) as usize;
    let rdchk1 = (nib1 | ((twos << 4) & 0xc0)) as usize;
    let rdchk2 = (nib2 | ((twos << 2) & 0xc0)) as usize;
    if chk0 != rdchk0 || chk1 != rdchk1 || chk2 != rdchk2 {
        log::debug!("expect checksum {},{},{} got {},{},{}",chk0,chk1,chk2,rdchk0,rdchk1,rdchk2);
        if verify_chk {
            return Err(Box::new(NibbleError::BadChecksum));
        }
    }
    return Ok(ans);
}
