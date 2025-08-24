//! Module for handling Commodore or Victor nibbles
//! 
//! address field contains ["marker","chksum","sector","track","format1","format2","marker","marker"] all encoded
//! standard decoded address is [0x08,chk,sec,trk,0x2e,0x20,0x0f,0x0f]

use crate::img::Error;
use crate::DYNERR;

const INVALID_NIB_BYTE: u8 = 0xff;

const FWD_G64: [u8;16] = [
    0b01010, 0b01011, 0b10010, 0b10011,
    0b01110, 0b01111, 0b10110, 0b10111,
    0b01001, 0b11001, 0b11010, 0b11011,
    0b01101, 0b11101, 0b11110, 0b10101
];

const REV_G64: [u8;32] = [
    0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xFF,0x08,0x00,0x01,0xFF,0x0C,0x04,0x05,
    0xFF,0xFF,0x02,0x03,0xFF,0x0F,0x06,0x07,
    0xFF,0x09,0x0A,0x0B,0xFF,0x0D,0x0E,0xFF
];

/// encode a normal byte as a 10-bit nibble packed in MSB order
pub fn encode_g64(val: u8) -> [u8;2] {
    let nib1 = FWD_G64[(val >> 4) as usize];
    let nib2 = FWD_G64[(val & 0x0f) as usize];
    [
        (nib1 << 3) | (nib2 >> 2),
        nib2 << 6
    ]
}

/// decode a 5-bit nibble aligned to MSB as a 4-bit value, invalid nibble will yield error
pub fn decode_g64(nib: u8) -> Result<u8,DYNERR> {
    let ans = REV_G64[(nib >> 3) as usize];
    if ans == INVALID_NIB_BYTE {
        Err(Box::new(Error::InvalidByte))
    } else {
        Ok(ans)
    }
}
