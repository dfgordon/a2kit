pub mod dos33;
pub mod prodos;
pub mod applesoft;
pub mod integer;
pub mod walker;
pub mod disk_base;
pub mod woz;
pub mod disk525;

use crate::disk_base::A2Disk;
use std::io::Read;

/// Given a bytestream try to identify the type of disk image and create a disk object.
/// N.b. this discards metadata and track layout details, only the high level data remains.
pub fn create_disk_from_bytestream(disk_img_data: &Vec<u8>) -> Box<dyn A2Disk> {
    if let Some(disk) = dos33::Disk::from_img(&disk_img_data) {
        return Box::new(disk);
    } else if let Some(disk) = prodos::Disk::from_do_img(&disk_img_data) {
        return Box::new(disk);
    } else if let Some(disk) = prodos::Disk::from_po_img(&disk_img_data) {
        return Box::new(disk);
    } else {
        panic!("could not interpret disk image data");
    }
}

/// Calls `create_disk_from_bytestream` getting the bytes from stdin
pub fn create_disk_from_stdin() -> Box<dyn A2Disk> {
    let mut disk_img_data = Vec::new();
    std::io::stdin().read_to_end(&mut disk_img_data).expect("failed to read input stream");
    return create_disk_from_bytestream(&disk_img_data);
}

/// Calls `create_disk_from_bytestream` getting the bytes from a file.
/// The pathname must already be in the right format for the file system.
pub fn create_disk_from_file(img_path: &str) -> Box<dyn A2Disk> {
    let disk_img_data = std::fs::read(img_path).expect("failed to read file");
    return create_disk_from_bytestream(&disk_img_data);
}

/// Display binary to stdout in columns of hex, +ascii, and -ascii
pub fn display_chunk(start_addr: u16,chunk: &Vec<u8>) {
    let mut slice_start = 0;
    loop {
        let row_label = start_addr as usize + slice_start;
        let mut slice_end = slice_start + 16;
        if slice_end > chunk.len() {
            slice_end = chunk.len();
        }
        let slice = chunk[slice_start..slice_end].to_vec();
        let txt: Vec<u8> = slice.iter().map(|c| match *c {
            x if x<32 => 46,
            x if x<128 => x,
            _ => 46
        }).collect();
        let neg_txt: Vec<u8> = slice.iter().map(|c| match *c {
            x if x>=160 && x<255 => x - 128,
            _ => 46
        }).collect();
        print!("{:04X} : ",row_label);
        for byte in slice {
            print!("{:02X} ",byte);
        }
        for _blank in slice_end..slice_start+16 {
            print!("   ");
        }
        print!("+ {} ",String::from_utf8_lossy(&txt));
        for _blank in slice_end..slice_start+16 {
            print!(" ");
        }
        println!("- {}",String::from_utf8_lossy(&neg_txt));
        slice_start += 16;
        if slice_end==chunk.len() {
            break;
        }
    }
}