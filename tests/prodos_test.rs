// test of prodos disk image module
use std::path::Path;
use std::fmt::Write;
use a2kit::fs::prodos;
use a2kit::img::{woz1,woz2};
use a2kit::disk_base::TextEncoder;
use a2kit::lang::applesoft;
use a2kit::disk_base::{ItemType,DiskFS,DiskImage,DiskKind};
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
    // Formatting: Copy2Plus, writing: MicroM8:
    // This tests a small BASIC program, binary, and text files
    let img = std::fs::read(&Path::new("tests").join("prodos-smallfiles.do")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

    // check the BASIC program
    let basic_program = "
    10 D$ =  CHR$ (4)
    20  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
    30  PRINT D$;\"BSAVE THECHIP,A768,L4\"
    40  PRINT D$;\"OPEN THETEXT\"
    50  PRINT D$;\"WRITE THETEXT\"
    60  PRINT \"HELLO FROM PRODOS\"
    70  PRINT D$;\"CLOSE THETEXT\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut lib_tokens = tokenizer.tokenize(basic_program,2049);
    lib_tokens.push(196);
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the binary
    let binary_data = emulator_disk.bload("thechip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));

    // check the sequential text file
    let (_z,raw) = emulator_disk.read_text("thetext").expect("error");
    let txt = prodos::types::SequentialText::from_bytes(&raw);
    let encoder = prodos::types::Encoder::new(Some(0x0d));
    assert_eq!(txt.text,encoder.encode("HELLO FROM PRODOS").unwrap());
}

#[test]
fn write_small() {
    // Formatting: Copy2Plus, writing: MicroM8:
    // This tests a small BASIC program, binary, and text file
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);

    // save the BASIC program
    let basic_program = "
    10 D$ =  CHR$ (4)
    20  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
    30  PRINT D$;\"BSAVE THECHIP,A768,L4\"
    40  PRINT D$;\"OPEN THETEXT\"
    50  PRINT D$;\"WRITE THETEXT\"
    60  PRINT \"HELLO FROM PRODOS\"
    70  PRINT D$;\"CLOSE THETEXT\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut lib_tokens = tokenizer.tokenize(basic_program,2049);
    lib_tokens.push(196);
    disk.save("hello",&lib_tokens,ItemType::ApplesoftTokens,None).expect("error");

    // save the binary
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,None).expect("error");

    // save the text
    let encoder = prodos::types::Encoder::new(Some(0x0d));
    disk.write_text("thetext",&encoder.encode("HELLO FROM PRODOS").unwrap()).expect("error");

    let mut ignore = disk.standardize(2);
    // loop to ignore boot blocks for this test
    for i in 0..1024 {
        ignore.push(i);
    }
    disk.compare(&Path::new("tests").join("prodos-smallfiles.do"),&ignore);
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
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("could not interpret image");
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
    let records = a2kit::disk_base::Records::from_json(JSON_REC).expect("could not parse JSON");
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

#[test]
fn read_big_woz1() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests the same file system information used for read_big and write_big.
    // We do not expect our WOZ track bits to be identical to those from an emulator.
    // So the strategy is to load the WOZ image as created by the emulator,
    // convert to DSK, and compare with a DSK that was also created via the emulator.
    // In testing the conversion we test a lot of underlying WOZ machinery.

    let buf = Path::new("tests").join("prodos-bigfiles.woz");
    let woz1_path = buf.to_str().expect("could not get path");
    let (_img,mut disk) = a2kit::create_img_and_fs_from_file(woz1_path).expect("could not interpret image");
    let ignore = disk.standardize(2);
    // As usual we have mysterious trailing byte differences which seem to be a real artifact of the emulators.
    // When VII saves the WOZ it does not have the trailing byte(s), using DSK in the exact same way does.
    if let Ok((_x,mut dir_chunk)) = disk.read_chunk("2") {
        dir_chunk[0x40] = 0x71;
        disk.write_chunk("2",&dir_chunk).expect("could not apply chunk correction");
    }
    if let Ok((_x,mut dir_chunk)) = disk.read_chunk("7") {
        dir_chunk[0x170] = 0xc4;
        disk.write_chunk("7",&dir_chunk).expect("could not apply chunk correction");
    }
    disk.compare(&Path::new("tests").join("prodos-bigfiles.dsk"),&ignore);    
}

#[test]
fn read_write_small_woz1_and_woz2() {
    // To verify we can write to WOZ, we create a DiskFS, save it to a WOZ, regenerate the DiskFS
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,None);

    // save the BASIC program
    let basic_program = "
    10 D$ =  CHR$ (4)
    20  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
    30  PRINT D$;\"BSAVE THECHIP,A768,L4\"
    40  PRINT D$;\"OPEN THETEXT\"
    50  PRINT D$;\"WRITE THETEXT\"
    60  PRINT \"HELLO FROM PRODOS\"
    70  PRINT D$;\"CLOSE THETEXT\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut lib_tokens = tokenizer.tokenize(basic_program,2049);
    lib_tokens.push(196);
    disk.save("hello",&lib_tokens,ItemType::ApplesoftTokens,None).expect("error");

    // save the binary
    disk.bsave("thechip",&[6,5,0,2].to_vec(),768,None).expect("error");

    // save the text
    let encoder = prodos::types::Encoder::new(Some(0x0d));
    disk.write_text("thetext",&encoder.encode("HELLO FROM PRODOS").unwrap()).expect("error");
    
    // Check we can go to WOZ1 and back

    let mut woz = woz1::Woz1::create(254, DiskKind::A2_525_16);
    woz.update_from_po(&disk.to_img()).expect("could not form WOZ");
    let other_img = woz.to_po().expect("could not unform WOZ");

    assert_eq!(disk.to_img(),other_img);

    // Check we can go to WOZ2 and back

    let mut woz = woz2::Woz2::create(254, DiskKind::A2_525_16);
    woz.update_from_po(&disk.to_img()).expect("could not form WOZ");
    let other_img = woz.to_po().expect("could not unform WOZ");

    assert_eq!(disk.to_img(),other_img);
}