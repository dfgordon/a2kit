// test of prodos disk image module
use std::path::Path;
use std::fmt::Write;
use a2kit::prodos;
use a2kit::applesoft;
use a2kit::disk_base::{ItemType,A2Disk};

#[test]
fn format() {
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);
    disk.compare(&Path::new("tests").join("prodos-blank.po"),&disk.standardize(2));
}

#[test]
fn create_dirs() {
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("DIRTEST"),true,None);
    disk.create(&String::from("TEST")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"TEST/T{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }
    let ignore = disk.standardize(2);
    disk.compare(&Path::new("tests").join("prodos-mkdir.dsk"),&ignore);
}

#[test]
fn read_small() {
    // test a disk we formatted ourselves, but saved some files in VII:
    // 1. BSAVE THECHIP,A$300,L$4    ($300: 6 5 0 2)
    // 2. SAVE HELLO    (10 PRINT "HELLO")
    // TODO: add a small sequential text file
    let img = std::fs::read(&Path::new("tests").join("prodos-smallfiles.po")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_disk_from_bytestream(&img);
    let binary_data = emulator_disk.bload("thechip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));
    let disk_tokens = emulator_disk.load("hello").expect("error");
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let lib_tokens = tokenizer.tokenize("10 print \"HELLO\"",2049);
    assert_eq!(disk_tokens,(0,lib_tokens));
}

#[test]
fn write_small() {
    // test a disk we formatted ourselves, but saved some files in VII:
    // 1. BSAVE THECHIP,A$300,L$4    ($300: 6 5 0 2)
    // 2. SAVE HELLO    (10 PRINT "HELLO")
    // TODO: add a small sequential text file
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,None).expect("error");
    let basic_program = "10 print \"HELLO\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let tokens = tokenizer.tokenize(basic_program, 2049);
    disk.save("hello",&tokens,ItemType::ApplesoftTokens,None).expect("error");
    disk.compare(&Path::new("tests").join("prodos-smallfiles.po"),&disk.standardize(2));
}

#[test]
fn out_of_space() {
    let mut disk = prodos::Disk::new(280);
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
    let emulator_disk = a2kit::create_disk_from_bytestream(&img);
    let mut buf: Vec<u8>;

    // check the BASIC program, this is a seedling file
    let basic_program = "
    10 d$ = chr$(4)
    20 print d$;\"open tree1,l128\"
    30 print d$;\"write tree1,r2000\"
    40 print \"HELLO FROM TREE 1\"
    50 print d$;\"close tree1\"
    60 print d$;\"open tree2,l127\"
    70 print d$;\"write tree2,r2000\"
    80 print \"HELLO FROM TREE 2\"
    90 print d$;\"write tree2,r4000\"
    100 print \"HELLO FROM TREE 2\"
    110 print d$;\"close tree2\"
    120 for i = 16384 to 32767: poke i,256*((i-16384)/256 - int((i-16384)/256)): next
    130 print d$;\"bsave sapling,a16384,l16384\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut lib_tokens = tokenizer.tokenize(basic_program,2049);
    let disk_tokens = emulator_disk.load("make.big").expect("error");
    lib_tokens.push(0xc4); // Virtual II added an extra byte, why?
    assert_eq!(disk_tokens,(0,lib_tokens));

    // TODO: read text records

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
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);

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
    90 print d$;\"write tree2,r4000\"
    100 print \"HELLO FROM TREE 2\"
    110 print d$;\"close tree2\"
    120 for i = 16384 to 32767: poke i,256*((i-16384)/256 - int((i-16384)/256)): next
    130 print d$;\"bsave sapling,a16384,l16384\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program,2049);
    tokens.push(0xc4); // Virtual II added an extra byte, why?
    disk.save("make.big",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    // make tree files using random access text module
    let mut records = a2kit::disk_base::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    records = a2kit::disk_base::Records::new(127);
    records.add_record(2000, "HELLO FROM TREE 2");
    records.add_record(4000, "HELLO FROM TREE 2");
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
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);

    let basic_program = "
    10 D$ =  CHR$ (4)
    20  PRINT D$;\"create inner.dirs\": PRINT D$;\"prefix /new.disk/inner.dirs\"
    30  FOR I = 1 TO 54: PRINT D$;\"create dir\";I: NEXT 
    40  FOR I = 1 TO 4: READ N
    50  PRINT D$;\"prefix /new.disk/inner.dirs/dir\";N
    60  PRINT D$;\"open tree,l127\"
    70  PRINT D$;\"write tree,r4000\"
    80  PRINT \"hello from tree\"
    90  PRINT D$;\"close tree\"
    100  NEXT 
    110  PRINT \"DELETE AND RENAME? \";: GET A$: IF A$ <  > \"Y\" THEN  END 
    120  PRINT D$;\"prefix /new.disk/inner.dirs\"
    130  PRINT D$;\"delete dir1\"
    140  PRINT D$;\"delete dir32/tree\"
    150  PRINT D$;\"delete dir32\"
    160  PRINT D$;\"rename dir53/tree,dir53/tree53\"
    170  DATA 5,19,32,53";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program,2049);
    tokens.push(0xc4); // extra and it was counted
    disk.save("setup",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    disk.create(&String::from("inner.dirs")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }

    // make tree files using random access text module
    let mut records = a2kit::disk_base::Records::new(127);
    records.add_record(4000, "hello from tree");

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
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);

    let basic_program = "
    10 D$ =  CHR$ (4)
    20  PRINT D$;\"create inner.dirs\": PRINT D$;\"prefix /new.disk/inner.dirs\"
    30  FOR I = 1 TO 54: PRINT D$;\"create dir\";I: NEXT 
    40  FOR I = 1 TO 4: READ N
    50  PRINT D$;\"prefix /new.disk/inner.dirs/dir\";N
    60  PRINT D$;\"open tree,l127\"
    70  PRINT D$;\"write tree,r4000\"
    80  PRINT \"hello from tree\"
    90  PRINT D$;\"close tree\"
    100  NEXT 
    110  PRINT \"DELETE AND RENAME? \";: GET A$: IF A$ <  > \"Y\" THEN  END 
    120  PRINT D$;\"prefix /new.disk/inner.dirs\"
    130  PRINT D$;\"delete dir1\"
    140  PRINT D$;\"delete dir32/tree\"
    150  PRINT D$;\"delete dir32\"
    160  PRINT D$;\"rename dir53/tree,dir53/tree53\"
    170  DATA 5,19,32,53";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program,2049);
    tokens.push(0xc4); // extra and it was counted
    disk.save("setup",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    disk.create(&String::from("inner.dirs")).expect("unreachable");
    for i in 1..55 {
        let mut path = "".to_string();
        write!(path,"inner.dirs/dir{}",i).expect("unreachable");
        disk.create(&path).expect("unreachable");
    }

    // make tree files using random access text module
    let mut records = a2kit::disk_base::Records::new(127);
    records.add_record(4000, "hello from tree");

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