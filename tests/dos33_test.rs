// test of dos33 disk image module
use std::path::Path;
use std::fmt::Write;
use a2kit::dos33;
use a2kit::disk_base::TextEncoder;
use a2kit::disk_base::{ItemType,A2Disk};
use a2kit::applesoft;

#[test]
fn format() {
    // DOS tracks can vary some depending on who did the formatting.
    // We are compatible with CiderPress.  The "last track" field in the VTOC
    // is left with value 18, *as if* a greeting program had been written there.
    let mut disk = dos33::Disk::new();
    disk.format(254,true,18);
    disk.compare(&Path::new("tests").join("dos33-boot.do"),&disk.standardize(0));
}

#[test]
fn read_small() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let img = std::fs::read(&Path::new("tests").join("dos33-smallfiles.dsk")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_disk_from_bytestream(&img);

    // check the BASIC program
    let basic_program = "
    10 D$ =  CHR$ (4)
    20  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
    30  PRINT D$;\"BSAVE THE CHIP,A768,L4\"
    40  PRINT D$;\"OPEN THETEXT\"
    50  PRINT D$;\"WRITE THETEXT\"
    60  PRINT \"HELLO FROM DOS 3.3\"
    70  PRINT D$;\"CLOSE THETEXT\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let lib_tokens = tokenizer.tokenize(basic_program,2049);
    let disk_tokens = emulator_disk.load("hello").expect("error");
    assert_eq!(disk_tokens,(0,lib_tokens));

    // check the binary
    let binary_data = emulator_disk.bload("the chip").expect("error");
    assert_eq!(binary_data,(768,vec![6,5,0,2]));

    // check the sequential text file
    let text = emulator_disk.read_text("thetext").expect("error");
    let encoder = dos33::types::Encoder::new(Some(0x8d));
    assert_eq!(text,(0,encoder.encode("HELLO FROM DOS 3.3").unwrap()));
}

#[test]
fn write_small() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let mut disk = dos33::Disk::new();
    disk.format(254,false,17);

    // save the BASIC program
    let basic_program = "
    10 D$ =  CHR$ (4)
    20  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
    30  PRINT D$;\"BSAVE THE CHIP,A768,L4\"
    40  PRINT D$;\"OPEN THETEXT\"
    50  PRINT D$;\"WRITE THETEXT\"
    60  PRINT \"HELLO FROM DOS 3.3\"
    70  PRINT D$;\"CLOSE THETEXT\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let lib_tokens = tokenizer.tokenize(basic_program,2049);
    disk.save("hello",&lib_tokens,ItemType::ApplesoftTokens,None).expect("error");

    // save the binary
    disk.bsave("the chip",&[6,5,0,2].to_vec(),768,None).expect("error");

    // save the text
    let encoder = dos33::types::Encoder::new(Some(0x8d));
    disk.write_text("thetext",&encoder.encode("HELLO FROM DOS 3.3").unwrap()).expect("error");

    let mut ignore = disk.standardize(0);
    // loop to ignore boot tracks for this test
    for i in 0..3*16*256 {
        ignore.push(i);
    }
    disk.compare(&Path::new("tests").join("dos33-smallfiles.dsk"),&ignore);
}

#[test]
fn out_of_space() {
    let mut disk = dos33::Disk::new();
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(254,true,17);
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
fn read_big() {
    // Formatting: CiderPress, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let img = std::fs::read(&Path::new("tests").join("dos33-bigfiles.do")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_disk_from_bytestream(&img);
    let mut buf: Vec<u8>;

    // check the BASIC program
    let basic_program = "
    10 D$ = CHR$(4)
    20 PRINT D$;\"OPEN TREE1,L128\"
    30 PRINT D$;\"WRITE TREE1,R2000\"
    40 PRINT \"HELLO FROM TREE 1\"
    50 PRINT D$;\"CLOSE TREE1\"
    60 PRINT D$;\"OPEN TREE2,L127\"
    70 PRINT D$;\"WRITE TREE2,R2000\"
    80 PRINT \"HELLO FROM TREE 2\"
    90 PRINT D$;\"WRITE TREE2,R4000\"
    100 PRINT \"HELLO FROM TREE 2\"
    110 PRINT D$;\"CLOSE TREE2\"
    120 FOR I = 16384 TO 32767: POKE I,256*((I-16384)/256 - INT((I-16384)/256)): NEXT
    130 PRINT D$;\"BSAVE SAPLING,A16384,L16384\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut lib_tokens = tokenizer.tokenize(basic_program,2049);
    let disk_tokens = emulator_disk.load("make.big").expect("error");
    lib_tokens.push(0x43); // Virtual II added an extra byte, why?
    assert_eq!(disk_tokens,(0,lib_tokens));

    // TODO: read text records

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
    // Formatting: CiderPress, Writing: Virtual II
    // This tests a small BASIC program, large binary, and two sparse text files
    let mut buf: Vec<u8>;
    let mut disk = dos33::Disk::new();
    disk.format(254,false,18);

    // create and save the BASIC program
    let basic_program = "
    10 D$ = CHR$(4)
    20 PRINT D$;\"OPEN TREE1,L128\"
    30 PRINT D$;\"WRITE TREE1,R2000\"
    40 PRINT \"HELLO FROM TREE 1\"
    50 PRINT D$;\"CLOSE TREE1\"
    60 PRINT D$;\"OPEN TREE2,L127\"
    70 PRINT D$;\"WRITE TREE2,R2000\"
    80 PRINT \"HELLO FROM TREE 2\"
    90 PRINT D$;\"WRITE TREE2,R4000\"
    100 PRINT \"HELLO FROM TREE 2\"
    110 PRINT D$;\"CLOSE TREE2\"
    120 FOR I = 16384 TO 32767: POKE I,256*((I-16384)/256 - INT((I-16384)/256)): NEXT
    130 PRINT D$;\"BSAVE SAPLING,A16384,L16384\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program,2049);
    tokens.push(0x43); // Virtual II added this and counted it, n.b. also the trailing byte it did not count
    disk.save("make.big",&tokens,ItemType::ApplesoftTokens,Some(&vec![0x41])).expect("dimg error");

    // make tree files using random access text module
    let mut records = a2kit::disk_base::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    records = a2kit::disk_base::Records::new(127);
    records.add_record(2000, "HELLO FROM TREE 2");
    records.add_record(4000, "HELLO FROM TREE 2");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,Some(&vec![0xc9])).expect("dimg error");

    disk.compare(&Path::new("tests").join("dos33-bigfiles.do"),&disk.standardize(0));
}

#[test]
fn rename_delete() {
    // Formatting: CiderPress, Writing: Virtual II
    // Adds deletion and renaming to scenario in `write_big`.
    let mut buf: Vec<u8>;
    let mut disk = dos33::Disk::new();
    disk.format(254,true,18);

    // create and save the BASIC program
    let basic_program = "
    10 D$ = CHR$(4)
    20 PRINT D$;\"OPEN TREE1,L128\"
    30 PRINT D$;\"WRITE TREE1,R2000\"
    40 PRINT \"HELLO FROM TREE 1\"
    50 PRINT D$;\"CLOSE TREE1\"
    60 PRINT D$;\"OPEN TREE2,L127\"
    70 PRINT D$;\"WRITE TREE2,R2000\"
    80 PRINT \"HELLO FROM TREE 2\"
    90 PRINT D$;\"WRITE TREE2,R4000\"
    100 PRINT \"HELLO FROM TREE 2\"
    110 PRINT D$;\"CLOSE TREE2\"
    120 FOR I = 16384 TO 32767: POKE I,256*((I-16384)/256 - INT((I-16384)/256)): NEXT
    130 PRINT D$;\"BSAVE SAPLING,A16384,L16384\"
    200 print D$;\"DELETE TREE2\"
    210 print D$;\"RENAME SAPLING,SAP\"
    220 print D$;\"RENAME TREE1,MYTREE1\"";
    let mut tokenizer = applesoft::tokenizer::Tokenizer::new();
    let mut tokens = tokenizer.tokenize(basic_program,2049);
    tokens.push(0x0a); // Virtual II added an extra byte *and* counted it in the length
    disk.save("ren.del",&tokens,ItemType::ApplesoftTokens,None).expect("dimg error");

    // make tree files using random access text module
    let mut records = a2kit::disk_base::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    records = a2kit::disk_base::Records::new(127);
    records.add_record(2000, "HELLO FROM TREE 2");
    records.add_record(4000, "HELLO FROM TREE 2");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,Some(&vec![0xc9])).expect("dimg error");

    // delete and rename
    disk.delete("tree2").expect("dimg error");
    disk.rename("sapling","sap").expect("dimg error");
    disk.rename("tree1","mytree1").expect("dimg error");

    disk.compare(&Path::new("tests").join("dos33-ren-del.do"),&disk.standardize(0));
}