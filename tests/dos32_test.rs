// test of DOS 3.2 support, which resides in the dos3x disk image module
use std::path::Path;
use std::collections::HashMap;
use a2kit::fs::{ChunkSpec,dos33};
use a2kit::img::{dsk_d13,woz1};
use a2kit::disk_base::TextEncoder;
use a2kit::disk_base::{ItemType,DiskFS,DiskKind};
use a2kit::lang::integer;
use a2kit_macro::DiskStruct;

pub const JSON_REC: &str = "
{
    \"fimg_type\": \"rec\",
    \"record_length\": 127,
    \"records\": {
        \"2000\": [\"HELLO FROM TREE 2\"],
        \"4000\": [\"HELLO FROM TREE 2\"]
    }
}";

fn ignore_boot_tracks(ignore: &mut HashMap<ChunkSpec,Vec<usize>>) {
    for t in 0..3 {
        for s in 0..13 {
            let mut all = vec![0;256];
            for i in 0..256 {
                all[i] = i;
            }
            ignore.insert(ChunkSpec::D13([t,s]),all);
        }
    }
}

#[test]
fn out_of_space() {
    let img = dsk_d13::D13::create(35);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    let big: Vec<u8> = vec![0;0x7f00];
    disk.init32(254,true);
    disk.bsave("f1",&big,0x800,None).expect("error");
    disk.bsave("f2",&big,0x800,None).expect("error");
    disk.bsave("f3",&big,0x800,None).expect("error");
    match disk.bsave("f4",&big,0x800,None) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "DISK FULL" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}

#[test]
fn read_small() {
    // Formatting: DOS, Writing: Virtual II
    // This tests a small BASIC program, binary, and text files
    let img = std::fs::read(&Path::new("tests").join("dos32-smallfiles.woz")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

    // check the BASIC program
    let basic_program = std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join("disk_builder.ibas")).expect("failed to read source code");
    let mut tokenizer = integer::tokenizer::Tokenizer::new();
    let lib_tokens = tokenizer.tokenize(basic_program);
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the binary
    let binary_data = emulator_disk.bload("thechip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));

    // check the sequential text file
    let (_z,raw) = emulator_disk.read_text("thetext").expect("error");
    let txt = dos33::types::SequentialText::from_bytes(&raw);
    let encoder = dos33::types::Encoder::new(Some(0x8d));
    assert_eq!(txt.text,encoder.encode("HELLO FROM DOS 3.2").unwrap());
}

#[test]
fn write_small() {
    // Formatting: DOS, Writing: Virtual II
    // This tests a small BASIC program, binary, and text file
    let img = woz1::Woz1::create(35, DiskKind::A2_525_13);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    disk.init32(254,true);

    // save the BASIC program
    let basic_program = std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join("disk_builder.ibas")).expect("failed to read source code");
    let mut tokenizer = integer::tokenizer::Tokenizer::new();
    let lib_tokens = tokenizer.tokenize(basic_program);
    disk.save("hello",&lib_tokens,ItemType::IntegerTokens,Some(&vec![0x08])).expect("error");

    // save the binary
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,Some(&vec![0x0a])).expect("error");

    // save the text
    let encoder = dos33::types::Encoder::new(Some(0x8d));
    disk.write_text("thetext",&encoder.encode("HELLO FROM DOS 3.2").unwrap()).expect("error");

    let mut ignore = disk.standardize(0);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos32-smallfiles.woz"),&ignore);
}
