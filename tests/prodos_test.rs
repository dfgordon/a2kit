// test of prodos disk image module
use std::path::Path;
use std::fmt::Write;
use a2kit::prodos;
use a2kit::applesoft;
use a2kit::disk_base::{ItemType,A2Disk};
use chrono;


fn get_emulator_bytes(path: &std::path::Path) -> Vec<u8> {
    let img = std::fs::read(path).expect("failed to read test image file");
    let mut emulator_disk = a2kit::create_disk_from_bytestream(&img);
    emulator_disk.standardize(2);
    return emulator_disk.to_img();
}

fn compare_blocks(actual: &Vec<u8>,expected: &Vec<u8>,num: u16) {
    for block in 0..num as usize {
        let mut fmt_actual = String::new();
        let mut fmt_expected = String::new();
        write!(&mut fmt_actual,"{:02X?}",&actual[block*512..(block+1)*512].to_vec()).expect("format error");
        write!(&mut fmt_expected,"{:02X?}",&expected[block*512..(block+1)*512].to_vec()).expect("format error");
        assert_eq!(fmt_actual,fmt_expected," at block {}",block)
    }
}

#[test]
fn test_format() {
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,
        Some(chrono::NaiveDate::from_ymd(2022,08,31).and_hms(3, 48, 0)));
    disk.standardize(2);

    let actual = disk.to_img();
    let expected = get_emulator_bytes(&Path::new("tests").join("prodos-blank.po"));
    compare_blocks(&actual,&expected,280);
}

#[test]
fn test_create_dirs() {
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("DIRTEST"),true,None);
    disk.create(&String::from("TEST"),None).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"TEST/T{}",i).expect("unreachable");
        disk.create(&path,None).expect("unreachable");
    }
    disk.standardize(2);

    let actual = disk.to_img();
    let expected = get_emulator_bytes(&Path::new("tests").join("mkdir_test.dsk"));
    compare_blocks(&actual,&expected,280);
}

#[test]
fn test_write() {
    // test a disk we formatted ourselves, but saved some files in VII:
    // 1. BSAVE THECHIP,A$300,L$4    ($300: 6 5 0 2)
    // 2. SAVE HELLO    (10 PRINT "HELLO")
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);
    disk.bsave(&"thechip".to_string(),&[6,5,0,2].to_vec(),768).expect("error");
    let basic_program = "10 print \"HELLO\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let tokens = tokenizer.tokenize(basic_program.to_string(), 2049);
    disk.save(&"hello".to_string(),&tokens,ItemType::ApplesoftTokens).expect("error");
    disk.standardize(2);

    let actual = disk.to_img();
    let expected = get_emulator_bytes(&Path::new("tests").join("prodos-smallfiles.po"));
    compare_blocks(&actual,&expected,280);
}

#[test]
fn test_out_of_space() {
    let mut disk = prodos::Disk::new(280);
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("NEW.DISK"),true,None);
    disk.bsave(&"f1".to_string(),&big,0x800).expect("error");
    disk.bsave(&"f2".to_string(),&big,0x800).expect("error");
    disk.bsave(&"f3".to_string(),&big,0x800).expect("error");
    disk.bsave(&"f4".to_string(),&big,0x800).expect("error");
    match disk.bsave(&"f5".to_string(),&big,0x800) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "DISK FULL" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}

#[test]
fn test_big_files() {
    // Test against a disk created in Virtual II using Copy2Plus and the below BASIC code.
    // This tests a seedling, a sapling, and two trees (both sparse)
    let mut buf: Vec<u8>;
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("BIG"),true,None);

    // create and save the BASIC program, this is a seedling file
    let basic_program = "
    10 d$ = chr$(4)
    20 print d$;\"open tree1,l128\"
    30 print d$;\"write tree1,r2000\"
    40 print \"HELLO FROM TREE 1\"
    50 print d$;\"close tree1\"
    60 print d$;\"open tree2,l127\"
    70 print d$;\"write tree2,r2000\"
    80 print \"HELLO FROM TREE 2\"
    90 print d$;\"close tree2\"
    100 for i = 16384 to 32767: poke i,256*((i-16384)/256 - int((i-16384)/256)): next
    110 print d$;\"bsave sapling,a16384,l16384\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program.to_string(),2049);
    tokens.push(0xc4); // Virtual II added an extra byte, why?
    disk.save(&"makebig".to_string(),&tokens,ItemType::ApplesoftTokens).expect("dimg error");
    let mut sparse: prodos::types::SparseFileData;

    // make sparse text file tree1 manually
    sparse = prodos::types::SparseFileData::new();
    buf = Vec::new();
    sparse.index.push(1);
    sparse.map.insert(1,vec![0;512]); // first block always written
    for _i in 1..500 {
        sparse.index.push(0); // 499 empty blocks
    }
    for c in "HELLO FROM TREE 1".as_bytes() {
        buf.push(*c);
    }
    buf.push(0x0d);
    sparse.index.push(2);
    sparse.map.insert(2,buf.clone());
    disk.write_sparse(&"tree1".to_string(), &sparse, a2kit::prodos::types::FileType::Text, 128).expect("dimg error");

    // make sparse text file tree2 manually
    sparse = prodos::types::SparseFileData::new();
    buf = Vec::new();
    sparse.index.push(1);
    sparse.map.insert(1,vec![0;512]); // first block always written
    for _i in 1..496 {
        sparse.index.push(0); // 495 empty blocks
    }
    for _i in 0..48 {
        buf.push(0); // record not aligned, need to pad block
    }
    for c in "HELLO FROM TREE 2".as_bytes() {
        buf.push(*c);
    }
    buf.push(0x0d);
    sparse.index.push(2);
    sparse.map.insert(2,buf.clone());
    disk.write_sparse(&"tree2".to_string(),&sparse,a2kit::prodos::types::FileType::Text,127).expect("dimg error");

    // write a large binary, this is a non-sparse sapling
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave(&"sapling".to_string(),&buf,16384).expect("dimg error");

    disk.standardize(2);

    let actual = disk.to_img();
    let expected = get_emulator_bytes(&Path::new("tests").join("prodos-bigfiles.dsk"));
    compare_blocks(&actual,&expected,280);
}