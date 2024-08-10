// test of pascal disk image module
use std::path::Path;
use std::collections::HashMap;
use a2kit::fs::pascal::types::BLOCK_SIZE;
use a2kit::fs::{Block,pascal,DiskFS};

// Some sample programs to test
// Indentation is important

const PROG1: &str =
"PROGRAM TEST;
BEGIN
  WRITE('HELLO FROM PASCAL')
END.
";

const PROG2: &str =
"
PROGRAM TEST2

BEGIN
        WRITE('ANOTHER SOURCE FILE')
END.
";

const PROG3: &str =
"   (* FIRST LINE INDENT **)
   
 PROGRAM TEST3;
 
 (* IS THIS SYNTAX OK? *)
    BEGIN
       WRITE('HELLO FROM TEST3')
    END.
";

fn ignore_boot_blocks(ignore: &mut HashMap<Block,Vec<usize>>) {
    for block in 0..2 {
        let mut all: Vec<usize> = Vec::new();
        for i in 0..BLOCK_SIZE {
            all.push(i);
        }
        ignore.insert(Block::PO(block),all);
    }
}

#[test]
fn format() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = pascal::Disk::from_img(Box::new(img)).expect("bad setup");
    disk.format(&String::from("BLANK"),0,None).expect("could not format");
    let mut ignore = disk.standardize(0);
    ignore_boot_blocks(&mut ignore);
    disk.compare(&Path::new("tests").join("pascal-blank.do"),&ignore);
}

#[test]
fn read_small() {
    // Formatting: CiderPress, writing: MicroM8:
    // This tests small Pascal source files
    let img = std::fs::read(&Path::new("tests").join("pascal-smallfiles.do")).expect("failed to read test image file");
    let mut emulator_disk = a2kit::create_fs_from_bytestream(&img,None).expect("file not found");

    // check source 1
    let txt = emulator_disk.read_text("hello.text").expect("error");
    assert_eq!(txt,PROG1);

    // check source 2
    let txt = emulator_disk.read_text("test2.text").expect("error");
    assert_eq!(txt,PROG2);

    // check source 3
    let txt = emulator_disk.read_text("test3.text").expect("error");
    assert_eq!(txt,PROG3);
}

#[test]
fn write_small() {
    // Formatting: CiderPress, writing: MicroM8:
    // This tests small Pascal source files
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = pascal::Disk::from_img(Box::new(img)).expect("bad setup");
    disk.format(&String::from("BLANK"),0,None).expect("failed to format");

    // save the text
    disk.write_text("hello.text",PROG1).expect("error");
    disk.write_text("test2.text",PROG2).expect("error");
    disk.write_text("test3.text",PROG3).expect("error");

    let mut ignore = disk.standardize(0);
    ignore_boot_blocks(&mut ignore);
    disk.compare(&Path::new("tests").join("pascal-smallfiles.do"),&ignore);
}

#[test]
fn out_of_space() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = pascal::Disk::from_img(Box::new(img)).expect("bad setup");
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("TEST"),0,None).expect("could not format");
    disk.bsave("f1",&big,None,None).expect("error");
    disk.bsave("f2",&big,None,None).expect("error");
    disk.bsave("f3",&big,None,None).expect("error");
    disk.bsave("f4",&big,None,None).expect("error");
    match disk.bsave("f5",&big,None,None) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "insufficient space" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}
