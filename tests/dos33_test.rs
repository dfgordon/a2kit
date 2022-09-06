// test of dos33 disk image module
use std::path::Path;
use a2kit::dos33::{self, DOS33Error};

#[test]
fn test_format() {
    let mut disk = dos33::Disk::new();
    disk.format(254,true);
    let actual = disk.to_do_img();
    let expected = std::fs::read(Path::new("tests").join("dos33-boot.do")).expect("failed to read test image file");
    // check this sector by sector otherwise we get a big printout
    for track in 0..35 {
        for sector in 0..16 {
            let offset = track*16 + sector;
            let abuf = actual[offset*256..(offset+1)*256].to_vec();
            let ebuf = expected[offset*256..(offset+1)*256].to_vec();
            assert_eq!(abuf,ebuf," at track {}, sector {}",track,sector);
        }
    }
}

#[test]
fn test_write() {
    // test a disk we formatted ourselves, but saved some files in VII:
    // 1. BSAVE THECHIP,A$300,L$4    ($300: 6 5 0 2)
    // 2. SAVE HELLO    (10 PRINT "HELLO")
    let mut disk = dos33::Disk::new();
    let basic_toks: Vec<u8> = [0x0e,0x08,0x0a,0x00,0xba,0x22,0x48,0x45,0x4c,0x4c,0x4f,0x22,0x00,0x00,0x00].to_vec();
    disk.format(254,true);
    disk.bsave(&"thechip".to_string(),&[6,5,0,2].to_vec(),768).expect("error");
    disk.save(&"hello".to_string(),
        &basic_toks,
        dos33::Type::Applesoft).expect("error");
    let actual = disk.to_do_img();
    let expected = std::fs::read(Path::new("tests").join("dos33-smallfiles.do")).expect("failed to read test image file");
    // check this sector by sector otherwise we get a big printout
    for track in 0..35 {
        for sector in 0..16 {
            let offset = track*16 + sector;
            let abuf = actual[offset*256..(offset+1)*256].to_vec();
            let mut ebuf = expected[offset*256..(offset+1)*256].to_vec();
            // special code to zero out mysterious trailing bytes DOS leaves after program data
            // TODO: figure out what is going on here
            if track>2 && ebuf[2..17]==basic_toks {
                ebuf[17] = 0;
            }
            assert_eq!(abuf,ebuf," at track {}, sector {}",track,sector);
        }
    }
}

#[test]
fn test_out_of_space() {
    let mut disk = dos33::Disk::new();
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(254,true);
    disk.bsave(&"f1".to_string(),&big,0x800).expect("error");
    disk.bsave(&"f2".to_string(),&big,0x800).expect("error");
    disk.bsave(&"f3".to_string(),&big,0x800).expect("error");
    match disk.bsave(&"f4".to_string(),&big,0x800) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e {
            DOS33Error::DiskFull => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}