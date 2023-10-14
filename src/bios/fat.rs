//! ### File allocation table (FAT)
//! 
//! Module for manipulating the FAT on FAT volumes.  This module assumes the
//! entire FAT is buffered (as usual we suppose small retro volumes).
//! 
//! The FAT can be thought of as a cluster pool with forward links.
//! A cluster is an allocation unit composed of a fixed number of logical sectors.
//! The links in the FAT form chains of clusters, each chain points to a file's data.
//! A cluster value tells us:
//! * state of cluster, can be damaged, free, or allocated
//! * if allocated, is this the last cluster
//! * if allocated and not the last cluster, where is the next cluster
//! 
//! The first two clusters are reserved, so that the first data cluster is cluster 2.
//! Cluster 0 contains the same value as the BPB's media field in the low 8 bits, higher bits are 1.
//! Cluster 1 contains end of cluster chain (EOC) upon formatting.
//! FAT16 and FAT32 use the high 2 bits of cluster 1 for flags, remembering
//! that with FAT32 the high 2 bits are really 26 and 27.

// end of cluster chain (EOC), if FAT entry is >= the value it is EOC.
const EOC12_MIN: u32 = 0xff8;
const EOC16_MIN: u32 = 0xfff8;
const EOC32_MIN: u32 = 0xffffff8; // remember FAT32 is really 28 bits
const EOC12_SET: u32 = 0xfff;
const EOC16_SET: u32 = 0xffff;
const EOC32_SET: u32 = 0xfffffff; // remember FAT32 is really 28 bits

const BAD_CLUSTER12: u32 = 0xff7;
const BAD_CLUSTER16: u32 = 0xfff7;
const BAD_CLUSTER32: u32 = 0xffffff7;

const FREE_CLUSTER: u32 = 0;
pub const FIRST_DATA_CLUSTER: u32 = 2;

/// get the value of cluster `n`.
/// `typ` = bits per FAT entry (12,16,32)
/// `buf` = buffer containing the entire FAT
pub fn get_cluster(n: usize,typ: usize,buf: &Vec<u8>) -> u32 {
    match typ {
        12 => {
            let offset = n + (n/2);
            let val16 = u16::from_le_bytes([buf[offset],buf[offset+1]]);
            if n & 1 == 1 {
                (val16 >> 4) as u32
            } else {
                (val16 & 0x0fff) as u32
            }
        },
        16 => {
            let offset = n*2;
            u16::from_le_bytes([buf[offset],buf[offset+1]]) as u32
        },
        32 => {
            let offset = n*4;
            u32::from_le_bytes(buf[offset..offset+4].try_into().expect("bounds")) & 0x0fffffff
        }
        _ => panic!("unexpected FAT type {}",typ)
    }
}

/// set the value of cluster `n`.
/// `typ` = bits per FAT entry (12,16,32)
/// `buf` = buffer containing the entire FAT
pub fn set_cluster(n: usize,val: u32,typ: usize,buf: &mut Vec<u8>) {
    match typ {
        12 => {
            let offset = n + (n/2);
            if n & 1 == 1 {
                let val12 = (val as u16) << 4;
                let low4 = 0x000f & u16::from_le_bytes(buf[offset..offset+2].try_into().expect("bounds"));
                let val16 = u16::to_le_bytes(val12 | low4);
                buf[offset] = val16[0];
                buf[offset+1] = val16[1];
            } else {
                let val12 = (val as u16) & 0x0fff;
                let high4 = 0xf000 & u16::from_le_bytes(buf[offset..offset+2].try_into().expect("bounds"));
                let val16 = u16::to_le_bytes(val12 | high4);
                buf[offset] = val16[0];
                buf[offset+1] = val16[1];
            }
        },
        16 => {
            let offset = n*2;
            let val16 = u16::to_le_bytes(val as u16);
            buf[offset] = val16[0];
            buf[offset+1] = val16[1];
        },
        32 => {
            let offset = n*4;
            let high4 = 0xf0000000 & u32::from_le_bytes(buf[offset..offset+4].try_into().expect("bounds"));
            let val32 = u32::to_le_bytes(high4 | (val & 0x0fffffff));
            for i in 0..4 {
                buf[offset+i] = val32[i];
            }
        }
        _ => panic!("unexpected FAT type {}",typ)
    }
}

pub fn is_damaged(n: usize,typ: usize,buf: &Vec<u8>) -> bool {
    match typ {
        12 => BAD_CLUSTER12==get_cluster(n,typ,buf),
        16 => BAD_CLUSTER16==get_cluster(n,typ,buf),
        32 => BAD_CLUSTER32==get_cluster(n,typ,buf),
        _ => panic!("unexpected FAT type {}",typ)
    }
}

pub fn is_free(n: usize,typ: usize,buf: &Vec<u8>) -> bool {
    get_cluster(n,typ,buf)==FREE_CLUSTER
}

pub fn is_last(n: usize,typ: usize,buf: &Vec<u8>) -> bool {
    match typ {
        12 => EOC12_MIN<=get_cluster(n,typ,buf),
        16 => EOC16_MIN<=get_cluster(n,typ,buf),
        32 => EOC32_MIN<=get_cluster(n,typ,buf),
        _ => panic!("unexpected FAT type {}",typ)
    }
}

pub fn deallocate(n: usize,typ: usize,buf: &mut Vec<u8>) {
    set_cluster(n,FREE_CLUSTER,typ,buf);
}

pub fn mark_damaged(n: usize,typ: usize,buf: &mut Vec<u8>) {
    match typ {
        12 => set_cluster(n,BAD_CLUSTER12,typ,buf),
        16 => set_cluster(n,BAD_CLUSTER16,typ,buf),
        32 => set_cluster(n,BAD_CLUSTER32,typ,buf),
        _ => panic!("unexpected FAT type {}",typ)
    }
}

pub fn mark_last(n: usize,typ: usize,buf: &mut Vec<u8>) {
    match typ {
        12 => set_cluster(n,EOC12_SET,typ,buf),
        16 => set_cluster(n,EOC16_SET,typ,buf),
        32 => set_cluster(n,EOC32_SET,typ,buf),
        _ => panic!("unexpected FAT type {}",typ)
    }
}
