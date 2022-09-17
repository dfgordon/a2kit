pub mod dos33;
pub mod prodos;
pub mod applesoft;
pub mod integer;
mod walker;
pub mod disk_base;

use crate::disk_base::A2Disk;
use std::io::Read;

/// Given a bytestream try to identify the type of disk image and create a disk object
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

pub fn create_disk_from_stdin() -> Box<dyn A2Disk> {
    let mut disk_img_data = Vec::new();
    std::io::stdin().read_to_end(&mut disk_img_data).expect("failed to read input stream");
    return create_disk_from_bytestream(&disk_img_data);
}

pub fn create_disk_from_file(img_path: &str) -> Box<dyn A2Disk> {
    let disk_img_data = std::fs::read(img_path).expect("failed to read file");
    return create_disk_from_bytestream(&disk_img_data);
}
