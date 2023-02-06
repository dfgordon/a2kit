// test of DOS 3.2 support, which resides in the dos3x disk image module
use std::path::Path;
use std::collections::HashMap;
use a2kit::fs::{Block,dos3x,TextEncoder,DiskFS};
use a2kit::img::{dsk_d13,woz1,names};
use a2kit::commands::ItemType;
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

fn ignore_boot_tracks(ignore: &mut HashMap<Block,Vec<usize>>) {
    for t in 0..3 {
        for s in 0..13 {
            let mut all = vec![0;256];
            for i in 0..256 {
                all[i] = i;
            }
            ignore.insert(Block::D13([t,s]),all);
        }
    }
}

fn get_tokens(filename: &str) -> Vec<u8> {
    let basic_program = std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join(filename)).expect("failed to read source code");
    let mut tokenizer = integer::tokenizer::Tokenizer::new();
    tokenizer.tokenize(basic_program).expect("tokenizer failed")
}

#[test]
fn out_of_space() {
    let img = dsk_d13::D13::create(35);
    let mut disk = dos3x::Disk::from_img(Box::new(img));
    let big: Vec<u8> = vec![0;0x7f00];
    disk.init32(254,true).expect("failed to INIT");
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
    let mut emulator_disk = a2kit::create_fs_from_bytestream(&img,None).expect("file not found");

    // check the BASIC program
    let lib_tokens = get_tokens("disk_builder.ibas");
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the binary
    let binary_data = emulator_disk.bload("thechip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));

    // check the sequential text file
    let (_z,raw) = emulator_disk.read_text("thetext").expect("error");
    let txt = dos3x::types::SequentialText::from_bytes(&raw);
    let encoder = dos3x::types::Encoder::new(vec![0x8d]);
    assert_eq!(txt.text,encoder.encode("HELLO FROM DOS 3.2").unwrap());
}

#[test]
fn write_small() {
    // Formatting: DOS, Writing: Virtual II
    // This tests a small BASIC program, binary, and text file
    let img = woz1::Woz1::create(35, names::A2_DOS32_KIND);
    let mut disk = dos3x::Disk::from_img(Box::new(img));
    disk.init32(254,true).expect("failed to INIT");

    // save the BASIC program
    let lib_tokens = get_tokens("disk_builder.ibas");
    disk.save("hello",&lib_tokens,ItemType::IntegerTokens,Some(&vec![0x08])).expect("error");

    // save the binary
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,Some(&vec![0x0a])).expect("error");

    // save the text
    let txt = disk.encode_text("HELLO FROM DOS 3.2").expect("could not encode text");
    disk.write_text("thetext",&txt).expect("error");

    let mut ignore = disk.standardize(0);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos32-smallfiles.woz"),&ignore);
}


#[test]
fn read_big() {
    // Formatting: DOS, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let img = std::fs::read(&Path::new("tests").join("dos32-bigfiles.woz")).expect("failed to read test image file");
    let mut emulator_disk = a2kit::create_fs_from_bytestream(&img,None).expect("could not interpret image");
    let mut buf: Vec<u8>;

    // check the BASIC program
    let lib_tokens = get_tokens("disk_builder.ibas");
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the text records
    let recs = emulator_disk.read_records("tree1", 128).expect("failed to read tree1");
    assert_eq!(recs.map.get(&2000).unwrap(),"HELLO FROM TREE 1\n");
    let recs = emulator_disk.read_records("tree2", 127).expect("failed to read tree2");
    assert_eq!(recs.map.get(&2000).unwrap(),"HELLO FROM TREE 2\n");
    assert_eq!(recs.map.get(&4000).unwrap(),"HELLO FROM TREE 2\n");

    // check a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    let binary_data = emulator_disk.bload("sapling").expect("dimg error");
    assert_eq!(binary_data,(16384,buf));

}

#[test]
fn write_big() {
    // Formatting: DOS, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let mut buf: Vec<u8>;
    let img = dsk_d13::D13::create(35);
    let mut disk = dos3x::Disk::from_img(Box::new(img));
    disk.init32(254,true).expect("failed to INIT");

    // create and save the BASIC program
    let tokens = get_tokens("disk_builder.ibas");
    disk.save("hello",&tokens,ItemType::IntegerTokens,Some(&vec![0x08])).expect("dimg error");

    // make tree files directly and from JSON
    let mut records = a2kit::fs::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    let records = a2kit::fs::Records::from_json(JSON_REC).expect("could not parse JSON");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,Some(&vec![0x5e])).expect("dimg error");

    let mut ignore = disk.standardize(0);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos32-bigfiles.woz"),&ignore);
}

#[test]
fn rename_delete() {
    // Formatting: DOS, Writing: Virtual II
    // Adds deletion and renaming to scenario in `write_big`.
    let mut buf: Vec<u8>;
    let img = dsk_d13::D13::create(35);
    let mut disk = dos3x::Disk::from_img(Box::new(img));
    disk.init32(254,true).expect("failed to INIT");

    // create and save the BASIC program
    let tokens = get_tokens("disk_builder.ibas");
    disk.save("hello",&tokens,ItemType::IntegerTokens,Some(&vec![0x08])).expect("dimg error");

    // make tree files using random access text module
    let mut records = a2kit::fs::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    records = a2kit::fs::Records::new(127);
    records.add_record(2000, "HELLO FROM TREE 2");
    records.add_record(4000, "HELLO FROM TREE 2");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,Some(&vec![0x5e])).expect("dimg error");

    // delete and rename
    disk.delete("tree2").expect("dimg error");
    disk.rename("sapling","sap").expect("dimg error");
    disk.rename("tree1","mytree1").expect("dimg error");

    let mut ignore = disk.standardize(0);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos32-ren-del.woz"),&ignore);
}
