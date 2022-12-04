// test of pascal disk image module
use std::path::Path;
use std::collections::HashMap;
use a2kit::fs::pascal::types::BLOCK_SIZE;
use a2kit::fs::{ChunkSpec,pascal};
use a2kit::disk_base::TextEncoder;
use a2kit::disk_base::{DiskFS,DiskKind};
use a2kit_macro::DiskStruct;

// Some sample programs to test
// Indentation is important

const PROG1: &str =
"PROGRAM TEST;
BEGIN
  WRITE('HELLO FROM PASCAL')
END.";

const PROG2: &str =
"
PROGRAM TEST2

BEGIN
        WRITE('ANOTHER SOURCE FILE')
END.";

const PROG3: &str =
"   (* FIRST LINE INDENT **)
   
 PROGRAM TEST3;
 
 (* IS THIS SYNTAX OK? *)
    BEGIN
       WRITE('HELLO FROM TEST3')
    END.";

fn ignore_boot_blocks(ignore: &mut HashMap<ChunkSpec,Vec<usize>>) {
    for block in 0..2 {
        let mut all: Vec<usize> = Vec::new();
        for i in 0..BLOCK_SIZE {
            all.push(i);
        }
        ignore.insert(ChunkSpec::PO(block),all);
    }
}

#[test]
fn format() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = pascal::Disk::from_img(Box::new(img));
    disk.format(&String::from("BLANK"),0,&DiskKind::A2_525_16,None).expect("could not format");
    let mut ignore = disk.standardize(0);
    ignore_boot_blocks(&mut ignore);
    disk.compare(&Path::new("tests").join("pascal-blank.do"),&ignore);
}

#[test]
fn read_small() {
    // Formatting: CiderPress, writing: MicroM8:
    // This tests small Pascal source files
    let img = std::fs::read(&Path::new("tests").join("pascal-smallfiles.do")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

    // check source 1
    let (_z,raw) = emulator_disk.read_text("hello.text").expect("error");
    let txt = pascal::types::SequentialText::from_bytes(&raw);
    let encoder = pascal::types::Encoder::new(Some(0x0d));
    assert_eq!(txt.text,encoder.encode(PROG1).unwrap());
    assert_eq!(encoder.decode(&txt.text).unwrap(),String::from(PROG1)+"\n");

    // check source 2
    let (_z,raw) = emulator_disk.read_text("test2.text").expect("error");
    let txt = pascal::types::SequentialText::from_bytes(&raw);
    let encoder = pascal::types::Encoder::new(Some(0x0d));
    assert_eq!(txt.text,encoder.encode(PROG2).unwrap());
    assert_eq!(encoder.decode(&txt.text).unwrap(),String::from(PROG2)+"\n");

    // check source 3
    let (_z,raw) = emulator_disk.read_text("test3.text").expect("error");
    let txt = pascal::types::SequentialText::from_bytes(&raw);
    let encoder = pascal::types::Encoder::new(Some(0x0d));
    assert_eq!(txt.text,encoder.encode(PROG3).unwrap());
    assert_eq!(encoder.decode(&txt.text).unwrap(),String::from(PROG3)+"\n");
}

#[test]
fn out_of_space() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = pascal::Disk::from_img(Box::new(img));
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("TEST"),0,&DiskKind::A2_525_16,None).expect("could not format");
    disk.bsave("f1",&big,0x800,None).expect("error");
    disk.bsave("f2",&big,0x800,None).expect("error");
    disk.bsave("f3",&big,0x800,None).expect("error");
    disk.bsave("f4",&big,0x800,None).expect("error");
    match disk.bsave("f5",&big,0x800,None) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "insufficient space" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}
