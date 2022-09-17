//! # Sector mappings for 5.25 inch floppy disks

use std::collections::HashMap;

/// Get block number and byte offset into block corresponding to
/// a given track and sector.  Returned in tuple (block,offset)
pub fn block_from_ts(track: u8,sector: u8) -> (u8,usize) {
    let block_offset: HashMap<u8,u8> = HashMap::from([
        (0,0),
        (1,7),
        (2,6),
        (3,6),
        (4,5),
        (5,5),
        (6,4),
        (7,4),
        (8,3),
        (9,3),
        (10,2),
        (11,2),
        (12,1),
        (13,1),
        (14,0),
        (15,7)
    ]);
    let byte_offset: HashMap<u8,usize> = HashMap::from([
        (0,0),
        (1,0),
        (2,256),
        (3,0),
        (4,256),
        (5,0),
        (6,256),
        (7,0),
        (8,256),
        (9,0),
        (10,256),
        (11,0),
        (12,256),
        (13,0),
        (14,256),
        (15,256)
    ]);
    return (8*track + block_offset.get(&sector).unwrap(), *byte_offset.get(&sector).unwrap());
}

/// Get the two track and sector pairs corresponding to a block.
/// The returned tuple is arranged in order.
pub fn ts_from_block(block: u16) -> ([u8;2],[u8;2]) {
    let sector1: HashMap<u16,u8> = HashMap::from([
        (0,0),
        (1,13),
        (2,11),
        (3,9),
        (4,7),
        (5,5),
        (6,3),
        (7,1),
    ]);
    let sector2: HashMap<u16,u8> = HashMap::from([
        (0,14),
        (1,12),
        (2,10),
        (3,8),
        (4,6),
        (5,4),
        (6,2),
        (7,15),
    ]);
    return (
        [(block/8) as u8, *sector1.get(&(block%8)).unwrap()],
        [(block/8) as u8, *sector2.get(&(block%8)).unwrap()]
    );
}