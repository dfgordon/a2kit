// test of dos33 disk image module
use std::collections::HashMap;
use std::path::Path;
use a2kit::img;
use a2kit::fs::{ChunkSpec,dos33};
use a2kit::disk_base::{ItemType,TextEncoder};
use a2kit::disk_base::DiskFS;
use a2kit::lang::applesoft;
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
        for s in 0..16 {
            let mut all = vec![0;256];
            for i in 0..256 {
                all[i] = i;
            }
            ignore.insert(ChunkSpec::DO([t,s]),all);
        }
    }
}

#[test]
fn format() {
    // DOS tracks can vary some depending on who did the formatting.
    // We are compatible with CiderPress.  The "last track" field in the VTOC
    // is left with value 18, *as if* a greeting program had been written there.
    let img = img::dsk_do::DO::create(35, 16);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    disk.init(254,true,18,35,16);
    disk.compare(&Path::new("tests").join("dos33-boot.do"),&disk.standardize(0));
}

#[test]
fn read_small() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a small BASIC program, binary, and text files
    let img = std::fs::read(&Path::new("tests").join("dos33-smallfiles.dsk")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

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
    let (_z,raw) = emulator_disk.read_text("thetext").expect("error");
    let txt = dos33::types::SequentialText::from_bytes(&raw);
    let encoder = dos33::types::Encoder::new(Some(0x8d));
    assert_eq!(txt.text,encoder.encode("HELLO FROM DOS 3.3").unwrap());
}

#[test]
fn write_small() {
    // Formatting: Copy2Plus, Writing: Virtual II
    // This tests a small BASIC program, binary, and text file
    let img = img::dsk_do::DO::create(35, 16);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    disk.init33(254,false);

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
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos33-smallfiles.dsk"),&ignore);
}

#[test]
fn out_of_space() {
    let img = img::dsk_do::DO::create(35, 16);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    let big: Vec<u8> = vec![0;0x7f00];
    disk.init33(254,true);
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
    // Formatting: Copy2Plus, Writing: MicroM8
    // This tests a small BASIC program, large binary, and two sparse text files
    let img = std::fs::read(&Path::new("tests").join("dos33-bigfiles.do")).expect("failed to read test image file");
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("could not interpret image");
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
    // Formatting: Copy2Plus, Writing: MicroM8
    // This tests a small BASIC program, large binary, and two sparse text files
    let mut buf: Vec<u8>;
    let img = img::dsk_do::DO::create(35, 16);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    disk.init33(254,false);

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
    tokens.push(0x43); // MicroM8 added this and counted it, n.b. also the trailing byte it did not count
    disk.save("make.big",&tokens,ItemType::ApplesoftTokens,Some(&vec![0x41])).expect("dimg error");

    // make tree files directly and from JSON
    let mut records = a2kit::disk_base::Records::new(128);
    records.add_record(2000, "HELLO FROM TREE 1");
    disk.write_records("tree1", &records).expect("dimg error");
    let records = a2kit::disk_base::Records::from_json(JSON_REC).expect("could not parse JSON");
    disk.write_records("tree2", &records).expect("dimg error");

    // write a large binary (sapling terminology is vestigial)
    buf = vec![0;16384];
    for i in 0..16384 {
        buf[i] = (i%256) as u8;
    }
    disk.bsave("sapling",&buf,16384,None).expect("dimg error");

    let mut ignore = disk.standardize(0);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos33-bigfiles.do"),&ignore);
}

#[test]
fn rename_delete() {
    // Formatting: CiderPress, Writing: Virtual II
    // Adds deletion and renaming to scenario in `write_big`.
    let mut buf: Vec<u8>;
    let img = img::dsk_do::DO::create(35, 16);
    let mut disk = dos33::Disk::from_img(Box::new(img));
    disk.init(254,true,18,35,16);

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

#[test]
fn read_big_woz1() {
    // Formatting: Copy2Plus, Writing: MicroM8
    // This tests the same file system information used for read_big and write_big.
    // Here we are simply reading from WOZ1 and DO and making sure we get
    // the same chunks either way.

    let buf = Path::new("tests").join("dos33-bigfiles.woz");
    let woz1_path = buf.to_str().expect("could not get path");
    let disk = a2kit::create_fs_from_file(woz1_path).expect("could not get image");
    let mut ignore = disk.standardize(2);
    ignore_boot_tracks(&mut ignore);
    disk.compare(&Path::new("tests").join("dos33-bigfiles.do"),&ignore);    
}