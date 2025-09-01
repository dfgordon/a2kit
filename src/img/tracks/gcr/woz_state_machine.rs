//! # Model for Disk II Controller
//! 
//! This models Apple hardware starting from the analog board in the drive up to the
//! logic state sequencer (LSS) in the controller card.  The job of this module is to
//! fill the data latch in response to arbitrary sequences of pulses from the drive head,
//! in a way that emulates as close as possible the actual hardware.
//! 
//! It is not in general necessary to model the system at this level of detail.
//! The `Method` setting can be used to prevent descending to this level.
//! 
//! ## Basic timing parameters for 5.25 inch disks
//! 
//! * Time is normalized to the resolution of a WOZ flux track, 1 tick = 125 ns.
//! * The bit-cell duration is 32 ticks
//! * The LSS cycle is 4 ticks
//! * Apple 2/2+/2e/2c processor cycle is 8 ticks
//! 
//! ## Using with 3.5 inch disks
//! 
//! The state machine runs twice as fast when used with 3.5 inch disks. Since the implementation
//! only references abstract tick counts, it can be used without change if the 3.5 inch disk
//! `FluxCells` are counting ticks every 62500 ps.
//! 
//! The LSS will use 32/4 = 8 LSS-cycles to liberate 1 bit.
//! It takes a minimum of 64 LSS-cycles to liberate 1 byte.
//! The LSS cannot resolve the fastest timescale possible in a WOZ image.

use super::super::FluxCells;

const FAKE_BITS: [u8;32] = [180, 2, 177, 40, 180, 160, 114, 96, 20, 1, 26, 45, 25, 96, 129, 70, 3, 0, 0, 77, 140, 42, 8, 137, 2, 8, 68, 4, 225, 195, 141, 0];

/// ROM is accessed as `[Q6*2 + Q7][high-bit][pulse][sequence]`
const ROM: [[[[u8;16];2];2];4] = [
    // Q6=0,Q7=0 (read)
    [
        // high bit clear
        [
            // no pulse
            [0x18,0x2d,0x38,0x48,0x58,0x68,0x78,0x88,0x98,0x29,0xbd,0x59,0xd9,0x08,0xfd,0x4d],
            // pulse
            [0x18,0x2d,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xcd,0xd9,0xd9,0xd8,0xfd,0xdd]
        ],
        // high bit set
        [
            // no pulse
            [0x18,0x38,0x28,0x48,0x58,0x68,0x78,0x88,0x98,0xa8,0xb8,0xc8,0xa0,0xe8,0xf8,0xe0],
            // pulse
            [0x18,0x38,0x08,0x48,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xd8,0xe8,0xf8,0xe0]
        ]
    ],
    // Q6=0,Q7=1 (shift for write, pulse does not affect)
    [
        // high bit clear
        [
            // no pulse
            [0x18,0x28,0x39,0x48,0x58,0x68,0x78,0x08,0x98,0xa8,0xb9,0xc8,0xd8,0xe8,0xf8,0x88],
            // pulse
            [0x18,0x28,0x39,0x48,0x58,0x68,0x78,0x08,0x98,0xa8,0xb9,0xc8,0xd8,0xe8,0xf8,0x88]
        ],
        // high bit set
        [
            // no pulse
            [0x18,0x28,0x39,0x48,0x58,0x68,0x78,0x88,0x98,0xa8,0xb9,0xc8,0xd8,0xe8,0xf8,0x08],
            // pulse
            [0x18,0x28,0x39,0x48,0x58,0x68,0x78,0x88,0x98,0xa8,0xb9,0xc8,0xd8,0xe8,0xf8,0x08]
        ]
    ],
    // Q6=1,Q7=0 (check write protect)
    [
        // high bit clear
        [
            // no pulse
            [0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a],
            // pulse
            [0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a]
        ],
        // high bit set
        [
            // no pulse
            [0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a],
            // pulse
            [0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a,0x0a]
        ]
    ],
    // Q6=1,Q7=1 (load for write, pulse does not affect)
    [
        // high bit clear
        [
            // no pulse
            [0x18,0x28,0x3b,0x48,0x58,0x68,0x78,0x08,0x98,0xa8,0xbb,0xc8,0xd8,0xe8,0xf8,0x88],
            // pulse
            [0x18,0x28,0x3b,0x48,0x58,0x68,0x78,0x08,0x98,0xa8,0xbb,0xc8,0xd8,0xe8,0xf8,0x88]
        ],
        // high bit set
        [
            // no pulse
            [0x18,0x28,0x3b,0x48,0x58,0x68,0x78,0x88,0x98,0xa8,0xbb,0xc8,0xd8,0xe8,0xf8,0x08],
            // pulse
            [0x18,0x28,0x3b,0x48,0x58,0x68,0x78,0x88,0x98,0xa8,0xbb,0xc8,0xd8,0xe8,0xf8,0x08]
        ]
    ]
];

/// State for both the analog board and the controller
#[derive(Clone)]
pub struct State {
    /// sequence number in the LSS program
    seq: usize,
    /// current value of data latch
    latch: u8,
    /// value of $C08D is what will be written out
    c08d: u8,
    /// first bit selecting arm of LSS program ($C08C -> off, $C08D -> on)
    q6: bool,
    /// second bit selecting arm of LSS program ($C08E -> off, $C08F -> on)
    q7: bool,
    /// is the disk write protected
    write_protect: bool,
    /// pointer into the circular fake bit buffer
    fake_bit_ptr: usize,
    /// absolute ticks when last pulse was emitted, used to emit fake bits
    last_pulse: u64,
    /// circular buffer of fake bits
    fake_bit_pool: bit_vec::BitVec
}

impl State {
    pub fn new() -> Self {
        Self {
            seq: 0,
            latch: 0,
            c08d: 0,
            q6: false,
            q7: false,
            write_protect: false,
            fake_bit_ptr: (chrono::Local::now().timestamp() % 256) as usize,
            last_pulse: 0,
            fake_bit_pool: bit_vec::BitVec::from_bytes(&FAKE_BITS)
        }
    }
    pub fn disable_fake_bits(&mut self) {
        self.fake_bit_pool = bit_vec::BitVec::from_bytes(&vec![0;32]);
    }
    pub fn enable_fake_bits(&mut self) {
        self.fake_bit_pool = bit_vec::BitVec::from_bytes(&FAKE_BITS);
    }
    /// Check for a pulse while advancing through one LSS cycle.
    fn mc3470_pulse(&mut self,cells: &mut FluxCells) -> u8 {
        let mut pulse = false;        
        if cells.ptr & cells.fmask == 0 {
            let flux_ptr = cells.ptr >> cells.fshift;
            let mut new_pulse = cells.stream.get(flux_ptr).unwrap();
            if new_pulse {
                self.last_pulse = cells.time;
            }
            if cells.ticks_since(self.last_pulse) > 96 {
                self.fake_bit_ptr = (self.fake_bit_ptr + 1) & 0xff;
                new_pulse = self.fake_bit_pool[self.fake_bit_ptr];
            }
            pulse |= new_pulse;                
        }
        cells.fwd(4);
        pulse as u8
    }
    pub fn get_seq(&self) -> usize {
        self.seq
    }
    pub fn get_latch(&self) -> u8 {
        self.latch
    }
    pub fn restore(&mut self,seq: usize,latch: u8) {
        self.seq = seq;
        self.latch = latch;
    }
    pub fn start_read(&mut self) {
        self.q6 = false;
        self.q7 = false;
    }
    // pub fn check_write_protect(&mut self) {
    //     self.q6 = true;
    //     self.q7 = false;
    // }
    // pub fn start_write(&mut self,set: u8) {
    //     self.q6 = true;
    //     self.q7 = true;
    //     self.c08d = set;
    // }
    // pub fn continue_write(&mut self) {
    //     self.q6 = false;
    // }
    /// Advance the state machine through `ticks` time units (125 ns).
    /// Returns whether the latch was touched or not.
    /// Assertion panic if anything is not aligned to 4-tick boundaries.
    pub fn advance(&mut self, ticks: usize, cells: &mut FluxCells) -> bool {
        assert!(ticks%4==0);
        assert!(cells.fshift > 1);
        assert!(cells.ptr & 3 == 0); 
        let mut touched = false;
        let cycles = ticks/4;
        for _ in 0..cycles {
            let pulse = self.mc3470_pulse(cells);
            let q6q7 = self.q6 as usize * 2 + self.q7 as usize;
            let neg = 0x80 & self.latch as usize;
            let next = ROM[q6q7][neg/0x80][pulse as usize][self.seq];
            let next_op = next & 0x0f;
            let next_seq = (next & 0xf0) >> 4;
            match next_op {
                0x00 => self.latch = 0,
                0x08 => {},
                0x09 => self.latch = self.latch << 1,
                0x0a => {
                    if self.write_protect {
                        self.latch = 0xff;
                    } else {
                        self.latch = self.latch >> 1;
                    }
                },
                0x0b => self.latch = self.c08d,
                0x0d => self.latch = (self.latch << 1) | 1,
                _ => panic!("illegal value in state machine ROM")
            };
            self.seq = next_seq as usize;
            touched |= next_op != 0x08;
        }
        touched
    }
}
