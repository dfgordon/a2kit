// test of prodos disk image module
use std::path::Path;
use std::fmt::Write;
use std::collections::HashMap;
use a2kit::fs::{Chunk,prodos,TextEncoder,DiskFS};
use a2kit::fs::prodos::types::BLOCK_SIZE;
use a2kit::lang::applesoft;
use a2kit::commands::ItemType;
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

fn ignore_boot_blocks(ignore: &mut HashMap<Chunk,Vec<usize>>) {
    for block in 0..2 {
        let mut all: Vec<usize> = Vec::new();
        for i in 0..BLOCK_SIZE {
            all.push(i);
        }
        ignore.insert(Chunk::PO(block),all);
    }
}

fn get_tokens(filename: &str) -> Vec<u8> {
    let basic_program = std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join(filename)).expect("failed to read source code");
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    return tokenizer.tokenize(&basic_program,2049);
}

#[test]
fn format() {
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);
    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-blank.po"),&ignore);
}

#[test]
fn create_dirs() {
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);
    let mut tokens = get_tokens("build_dirs.bas");
    tokens.push(0xc4); // Virtual II added an extra byte, why?
    disk.save("hello",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");
    disk.create(&String::from("INNER.DIRS")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"INNER.DIRS/DIR{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }
    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-mkdir.dsk"),&ignore);
}

#[test]
fn read_small() {
    // Formatting: Copy2Plus, writing: Virtual II:
    // This tests a small BASIC program, binary, and text files
    let img = std::fs::read(&Path::new("tests").join("prodos-smallfiles.do")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

    // check the BASIC program
    let mut lib_tokens = get_tokens("disk_builder.abas");
    lib_tokens.push(196);
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the binary
    let binary_data = emulator_disk.bload("thechip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));

    // check the sequential text file
    let (_z,raw) = emulator_disk.read_text("thetext").expect("error");
    let txt = prodos::types::SequentialText::from_bytes(&raw);
    let encoder = prodos::types::Encoder::new(vec![0x0d]);
    assert_eq!(txt.text,encoder.encode("HELLO FROM EMULATOR").unwrap());
}

#[test]
fn write_small() {
    // Formatting: Copy2Plus, writing: Virtual II:
    // This tests a small BASIC program, binary, and text file
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);

    // save the BASIC program
    let mut lib_tokens = get_tokens("disk_builder.abas");
    lib_tokens.push(196);
    disk.save("hello",&lib_tokens,ItemType::ApplesoftTokens,None).expect("error");

    // save the binary
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,None).expect("error");

    // save the text
    let encoder = prodos::types::Encoder::new(vec![0x0d]);
    disk.write_text("thetext",&encoder.encode("HELLO FROM EMULATOR").unwrap()).expect("error");

    let mut ignore = disk.standardize(2);
    ignore_boot_blocks(&mut ignore);
    disk.compare(&Path::new("tests").join("prodos-smallfiles.do"),&ignore);
}

#[test]
fn out_of_space() {
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("NEW.DISK"),true,None);
    disk.bsave("f1",&big,0x800,None).expect("error");
    disk.bsave("f2",&big,0x800,None).expect("error");
    disk.bsave("f3",&big,0x800,None).expect("error");
    disk.bsave("f4",&big,0x800,None).expect("error");
    match disk.bsave("f5",&big,0x800,None) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "DISK FULL" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}

#[test]
fn read_big() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a seedling, a sapling, and two trees (both sparse)
    let img = std::fs::read(&Path::new("tests").join("prodos-bigfiles.dsk")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("could not interpret image");
    let mut buf: Vec<u8>;

    // check the BASIC program, this is a seedling file
    let mut lib_tokens = get_tokens("disk_builder.abas");
    let disk_tokens = emulator_disk.load("hello").expect("error");
    lib_tokens.push(0xc4); // Virtual II added an extra byte, why?
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the text records
    let recs = emulator_disk.read_records("tree1", 0).expect("failed to read tree1");
    assert_eq!(recs.map.get(&2000).unwrap(),"HELLO FROM TREE 1\n");
    let recs = emulator_disk.read_records("tree2", 0).expect("failed to read tree2");
    assert_eq!(recs.map.get(&2000).unwrap(),"HELLO FROM TREE 2\n");
    assert_eq!(recs.map.get(&4000).unwrap(),"HELLO FROM TREE 2\n");

    // check a large binary, this is a non-sparse sapling
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    let binary_data = emulator_disk.bload("sapling").expect("dimg error");
    assert_eq!(binary_data,(16384,buf));

}

#[test]
fn write_big() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a seedling, a sapling, and two trees (both sparse)
    let mut buf: Vec<u8>;
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);

    // create and save the BASIC program, this is a seedling file
    let mut lib_tokens = get_tokens("disk_builder.abas");
    lib_tokens.push(0xc4); // Virtual II added an extra byte, why?
    disk.save("hello",&lib_tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    // make tree files using random access text module
    let mut records = a2kit::fs::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    let records = a2kit::fs::Records::from_json(JSON_REC).expect("could not parse JSON");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary, this is a non-sparse sapling
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,None).expect("dimg error");

    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-bigfiles.dsk"),&ignore);
}

#[test]
fn fill_dirs() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // Make a lot of directories and put sparse files in a few of them
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);

    let mut tokens = get_tokens("build_dirs.bas");
    tokens.push(0xc4); // extra and it was counted
    disk.save("hello",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    disk.create(&String::from("inner.dirs")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }

    // make tree files using random access text module
    let mut records = a2kit::fs::Records::new(127);
    records.add_record(4000, "HELLO FROM TREE");

    let n_set = [5,19,32,53];
    for n in n_set {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}/tree",n).expect("unreachable");
        disk.write_records(&path, &records).expect("dimg error");
    }

    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-fill-dirs.dsk"),&ignore);
}

#[test]
fn rename_delete() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // test delete and rename of sparse tree files and directories inside a large subdirectory
    let img = a2kit::img::dsk_po::PO::create(280);
    let mut disk = prodos::Disk::from_img(Box::new(img));
    disk.format(&String::from("NEW.DISK"),true,None);

    let mut tokens = get_tokens("build_dirs.bas");
    tokens.push(0xc4); // extra and it was counted
    disk.save("hello",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    disk.create(&String::from("inner.dirs")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }

    // make tree files using random access text module
    let mut records = a2kit::fs::Records::new(127);
    records.add_record(4000, "HELLO FROM TREE");

    let n_set = [5,19,32,53];
    for n in n_set {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}/tree",n).expect("unreachable");
        disk.write_records(&path, &records).expect("dimg error");
    }

    let ignore = disk.standardize(2);

    // delete and rename
    disk.delete("inner.dirs/dir1").expect("dimg error");
    disk.delete("inner.dirs/dir32/tree").expect("dimg error");
    disk.delete("inner.dirs/dir32").expect("dimg error");
    disk.rename("inner.dirs/dir53/tree","tree53").expect("dimg error");

    disk.compare(&Path::new("tests").join("prodos-ren-del.dsk"),&ignore);
}

#[test]
fn read_big_woz1() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests the same file system information used for read_big and write_big.

    let buf = Path::new("tests").join("prodos-bigfiles.woz");
    let woz1_path = buf.to_str().expect("could not get path");
    let disk = a2kit::create_fs_from_file(woz1_path).expect("could not interpret image");
    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-bigfiles.dsk"),&ignore);    
}
