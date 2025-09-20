
//! Functions to create standard formats
//! 
//! These functions appear as implementations of `DiskFormat` and `ZoneFormat`,
//! where the former tends to call into the latter.  A given function can often create
//! variations on a format depending on its arguments.

use super::*;

fn expr_vec(lst: &[&str]) -> Vec<String> {
    let mut ans = Vec::new();
    for x in lst {
        ans.push(x.to_string())
    }
    ans
}

impl ZoneFormat {
    /// argument is bits in a sync-byte, normally 9 for 13-sector disks, but if you are
    /// creating this for something that loses leading zeroes (like NIB) set it to 8.
    pub fn apple_525_13(sync_bits: usize) -> Self {
        assert!(sync_bits==8 || sync_bits==9);
        Self {
            flux_code: img::FluxCode::GCR,
            addr_nibs: img::FieldCode::WOZ((4,4)),
            data_nibs: img::FieldCode::WOZ((5,3)),
            speed_kbps: 250,
            motor_start: 0,
            motor_end: 140,
            motor_step: 4,
            heads: vec![0],
            addr_fmt_expr: expr_vec(&["vol","cyl","sec","vol^cyl^sec"]),
            addr_seek_expr: expr_vec(&["a0","cyl","sec","a0^a1^a2"]),
            data_expr: expr_vec(&["dat"]),
            markers: [
                SectorMarker {key: vec![0xd5,0xaa,0xb5], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa,0xeb], mask: vec![0xff,0xff,0x00]},
                SectorMarker {key: vec![0xd5,0xaa,0xad], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa,0xeb], mask: vec![0xff,0xff,0x00]},
            ],
            gaps: [
                BitVec::from_fn(40*sync_bits, |i| { i%sync_bits < 8 }),
                BitVec::from_fn(10*sync_bits, |i| { i%sync_bits < 8 }),
                BitVec::from_fn(20*sync_bits, |i| { i%sync_bits < 8 }),
            ],
            capacity: vec![256;13],
            swap_nibs: vec![]
        }
    }
    /// argument is bits in a sync-byte, normally 10 for 16-sector disks, but if you are
    /// creating this for something that loses leading zeroes (like NIB) set it to 8.
    pub fn apple_525_16(sync_bits: usize) -> Self {
        assert!(sync_bits==8 || sync_bits==10);
        Self {
            flux_code: img::FluxCode::GCR,
            addr_nibs: img::FieldCode::WOZ((4,4)),
            data_nibs: img::FieldCode::WOZ((6,2)),
            speed_kbps: 250,
            motor_start: 0,
            motor_end: 140,
            motor_step: 4,
            heads: vec![0],
            addr_fmt_expr: expr_vec(&["vol","cyl","sec","vol^cyl^sec"]),
            addr_seek_expr: expr_vec(&["a0","cyl","sec","a0^a1^a2"]),
            data_expr: expr_vec(&["dat"]),
            markers: [
                SectorMarker {key: vec![0xd5,0xaa,0x96], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa,0xeb], mask: vec![0xff,0xff,0x00]},
                SectorMarker {key: vec![0xd5,0xaa,0xad], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa,0xeb], mask: vec![0xff,0xff,0x00]},
            ],
            gaps: [
                BitVec::from_fn(40*sync_bits, |i| { i%sync_bits < 8 }),
                BitVec::from_fn(10*sync_bits, |i| { i%sync_bits < 8 }),
                BitVec::from_fn(20*sync_bits, |i| { i%sync_bits < 8 }),
            ],
            capacity: vec![256;16],
            swap_nibs: vec![]
        }
    }
    pub fn apple_35(zone: usize,sides: usize,interleave: usize) -> Self {
        assert!(sides>0 && sides<3 && zone<5 && interleave<32);
        let format = (interleave + (sides-1)*32).to_string();
        let chk = ["(cyl^sec^(head*32+cyl//64)^",&format,")&63"].concat();
        Self {
            flux_code: img::FluxCode::GCR,
            addr_nibs: img::FieldCode::WOZ((6,2)),
            data_nibs: img::FieldCode::WOZ((6,2)),
            speed_kbps: 500,
            motor_start: zone*16,
            motor_end: zone*16 + 16,
            motor_step: 1,
            heads: match sides {
                1 => vec![0],
                2 => vec![0,1],
                _ => panic!("sides should be 1 or 2")
            },
            addr_fmt_expr: expr_vec(&["cyl%64","sec","head*32+cyl//64",&format,&chk]),
            addr_seek_expr: expr_vec(&["cyl%64","sec","head*32+cyl//64",&format,"(a0^a1^a2^a3)&63"]),
            data_expr: expr_vec(&["sec","dat"]),
            markers: [
                SectorMarker {key: vec![0xd5,0xaa,0x96], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa], mask: vec![0xff,0xfe]}, // allow for error in the last bit
                SectorMarker {key: vec![0xd5,0xaa,0xad], mask: vec![0xff,0xff,0xff]},
                SectorMarker {key: vec![0xde,0xaa], mask: vec![0xff,0xff]},
            ],
            gaps: [
                BitVec::from_fn(36*10, |i| { i%10 < 8 }),
                BitVec::from_fn(6*10, |i| { i%10 < 8 }),
                BitVec::from_fn(36*10, |i| { i%10 < 8 }),
            ],
            capacity: vec![524;12-zone],
            swap_nibs: vec![]
        }
    }
}

impl DiskFormat {
    pub fn apple_525_13(sync_bits: usize) -> Self {
        Self {
            zones: vec![ZoneFormat::apple_525_13(sync_bits)]
        }
    }
    pub fn apple_525_16(sync_bits: usize) -> Self {
        Self {
            zones: vec![ZoneFormat::apple_525_16(sync_bits)]
        }
    }
    pub fn apple_35(sides: usize,interleave: usize) -> Self {
        Self {
            zones: vec![
                ZoneFormat::apple_35(0, sides, interleave),
                ZoneFormat::apple_35(1, sides, interleave),
                ZoneFormat::apple_35(2, sides, interleave),
                ZoneFormat::apple_35(3, sides, interleave),
                ZoneFormat::apple_35(4, sides, interleave),
            ]
        }
    }
}