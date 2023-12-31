// test of FAT file system
use std::path::Path;
use std::fmt::Write;
use a2kit::fs::{fat,DiskFS,Block};
use std::collections::HashMap;

fn get_builder(filename: &str,disk: &fat::Disk) -> Vec<u8> {
    let s = std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join(filename)).expect("failed to read source code");
    disk.encode_text(&s).expect("could not encode")
}

fn build_ren_del(disk: &mut fat::Disk) -> HashMap<Block,Vec<usize>> {
    // make the same text that the BASIC program makes
    let mut txt_string = String::new();
    for i in 1..1025 {
        writeln!(txt_string," {} ",i).expect("unreachable");
    }
    let txt = disk.encode_text(&txt_string).expect("could not encode");
    
    let batch = get_builder("msdos_builder.bat",&disk);
    let basic = get_builder("msdos_builder.bas",&disk);
    disk.write_text("DSKBLD.BAT",&batch).expect("dimg error");
    disk.write_text("DSKBLD.BAS",&basic).expect("dimg error");

    disk.create(&String::from("DIR1")).expect("dimg error");
    disk.write_text("DIR1/DSKBLD.BAS",&basic).expect("dimg error");
    disk.write_text("DIR1/ASCEND.TXT",&txt).expect("dimg error");

    disk.create(&String::from("DIR1/SUBDIR1")).expect("dimg error");
    disk.write_text("DIR1/SUBDIR1/DSKBLD.BAS",&basic).expect("dimg error");
    disk.write_text("DIR1/SUBDIR1/ASCEND.TXT",&txt).expect("dimg error");
    disk.rename("DIR1/SUBDIR1/ASCEND.TXT","UP.TXT").expect("dimg error");

    disk.create(&String::from("DIR2")).expect("dimg error");
    disk.create(&String::from("DIR3")).expect("dimg error");
    disk.write_text("DIR2/ASCEND.TXT",&txt).expect("dimg error");
    disk.write_text("DIR3/ASCEND.TXT",&txt).expect("dimg error");
    
    let ignore = disk.standardize(0);

    disk.delete("DIR2/ASCEND.TXT").expect("dimg error");
    disk.delete("DIR2").expect("dimg error");

    ignore
}

#[test]
fn rename_delete_img() {
    // Reference disk was created using 86Box.
    // test delete and rename of text files and directories inside a large subdirectory
    let kind = a2kit::img::DiskKind::D525(a2kit::img::names::IBM_SSDD_8);
    let boot_sector = a2kit::bios::bpb::BootSector::create(&kind).expect("could not create boot sector");
    let img = a2kit::img::dsk_img::Img::create(kind);
    let mut disk = fat::Disk::from_img(Box::new(img),Some(boot_sector));
    disk.format(&String::from("NEW DISK 1"),None).expect("failed to format");

    let ignore = build_ren_del(&mut disk);

    disk.compare(&Path::new("tests").join("msdos-ren-del.img"),&ignore);
}

#[test]
fn rename_delete_imd() {
    // Reference disk was created using 86Box, converted to imd using a2kit sector copy.
    // test delete and rename of text files and directories inside a large subdirectory
    let kind = a2kit::img::DiskKind::D525(a2kit::img::names::IBM_DSDD_9);
    let boot_sector = a2kit::bios::bpb::BootSector::create(&kind).expect("could not create boot sector");
    let img = a2kit::img::imd::Imd::create(kind);
    let mut disk = fat::Disk::from_img(Box::new(img),Some(boot_sector));
    disk.format(&String::from("NEW DISK 1"),None).expect("failed to format");

    let ignore = build_ren_del(&mut disk);

    disk.compare(&Path::new("tests").join("msdos-ren-del.imd"),&ignore);
}
