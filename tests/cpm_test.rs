// test of CP/M disk image module
use std::path::Path;
use std::collections::HashMap;
use std::fmt::Write;

use a2kit::fs::{cpm,DiskFS,Block};
use a2kit::img::{dsk_do,names};
use a2kit::bios::dpb::DiskParameterBlock;

// Some lines we entered in the emulator using ED.COM.
// One thing to note: if we would use an odd number of
// CP/M "sectors" (128 byte records) ED would leave copious
// trailing data; so we are careful to fill an even number.

const RCH: &str = "unreachable was reached";

const ED_TEST: &str =
"From the story \"Polaris\"
by H.P. Lovecraft

Slumber watcher, 'til the spheres
Six and twenty thousand years
Have revolved and I return
To the spot where now I burn.

Other stars anon shall rise
To the axis of the skies
Stars that soothe and stars that bless
With a sweet forgetfulness.

Only when my round is o'er
Shall the past disturb thy door.

-----------------------------
";

#[test]
fn read_small() {
    // Formatting: FORMAT.COM, writing: ED.COM, emulator: Virtual II
    // This tests small CP/M text files
    let img = std::fs::read(&Path::new("tests").join("cpm-smallfiles.dsk")).expect("failed to read test image file");
    let mut emulator_disk = a2kit::create_fs_from_bytestream(&img,None).expect("file not found");

    // check text file
    let fimg = emulator_disk.get("POLARIS.TXT").expect("error");
    let txt = fimg.unpack_txt().expect("bad setup");
    assert_eq!(&txt,ED_TEST);
}

#[test]
fn write_small() {
    // Formatting: FORMAT.COM, writing: ED.COM, emulator: Virtual II
    // This tests small CP/M text files
    let img = dsk_do::DO::create(35, 16);
    let mut disk = cpm::Disk::from_img(Box::new(img),DiskParameterBlock::create(&names::A2_DOS33_KIND),[2,2,3]).expect("bad setup");
    disk.format("test",None).expect("failed to format disk");

    // save the text
    let fimg = disk.new_fimg(None, false, "POLARIS.BAK").expect(RCH);
    disk.put(&fimg).expect(RCH);
    let mut fimg = disk.new_fimg(None, false, "POLARIS.TXT").expect(RCH);
    fimg.pack_txt(ED_TEST).expect(RCH);
    disk.put(&fimg).expect(RCH);

    let ignore = disk.standardize(0);
    disk.compare(&Path::new("tests").join("cpm-smallfiles.dsk"),&ignore);
}

#[test]
fn write_small_timestamps() {
    // Formatting: INITDIR.COM, writing: ED.COM, emulator: AppleWin
    // This tests a small CP/M text file with timestamping, and therefore the disk label also.
    let time = chrono::NaiveDate::from_ymd_opt(1978, 1, 1).unwrap()
        .and_hms_opt(0,43,0).unwrap();
    let img = dsk_do::DO::create(35, 16);
    let mut disk = cpm::Disk::from_img(Box::new(img),DiskParameterBlock::create(&names::A2_DOS33_KIND),[3,1,0]).expect("bad setup");
    disk.format("",Some(time)).expect("failed to format disk");

    // save the text
    let mut fimg = disk.new_fimg(None, true, "POLARIS.TXT").expect(RCH);
    fimg.pack_txt(ED_TEST).expect(RCH);
    disk.put(&fimg).expect(RCH);

    let ignore = disk.standardize(0);
    disk.compare(&Path::new("tests").join("cpm-timestamps.dsk"),&ignore);
}

#[test]
fn out_of_space() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = cpm::Disk::from_img(Box::new(img),DiskParameterBlock::create(&names::A2_DOS33_KIND),[2,2,3]).expect("bad setup");
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("TEST"),None).expect("could not format");
    let mut fimg = disk.new_fimg(None, false, "f1").expect(RCH);
    fimg.pack_bin(&big,None,None).expect(RCH);
    disk.put_at("f1",&mut fimg).expect(RCH);
    disk.put_at("f2",&mut fimg).expect(RCH);
    disk.put_at("f3",&mut fimg).expect(RCH);
    match disk.put_at("f4",&mut fimg) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "disk full" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}

fn get_builder(filename: &str) -> String {
    std::fs::read_to_string(&Path::new("tests").
        join("disk_builders").
        join(filename)).expect("failed to read source code")
}

fn build_ren_del(disk: &mut cpm::Disk) -> HashMap<Block,Vec<usize>> {
    // make the same text that the BASIC program makes
    let mut txt_string = String::new();
    for i in 1..1025 {
        writeln!(txt_string," {} ",i).expect("unreachable");
    }

    let basic = get_builder("msdos_builder.bas");
    let mut basic_fimg = disk.new_fimg(None, false, "DSKBLD.BAS").expect(RCH);
    basic_fimg.pack_txt(&basic).expect(RCH);
    let mut txt_fimg = disk.new_fimg(None, false, "ASCEND.TXT").expect(RCH);
    txt_fimg.pack_txt(&txt_string).expect(RCH);

    disk.put(&basic_fimg).expect(RCH);
    disk.put(&txt_fimg).expect(RCH);
    disk.rename("ASCEND.TXT","ASCEND1.TXT").expect("dimg error");
    disk.put(&txt_fimg).expect(RCH);
    disk.rename("ASCEND.TXT","ASCEND2.TXT").expect("dimg error");
    disk.put(&txt_fimg).expect(RCH);
    disk.rename("ASCEND.TXT","ASCEND3.TXT").expect("dimg error");
    disk.put(&txt_fimg).expect(RCH);
    disk.rename("ASCEND.TXT","ASCEND4.TXT").expect("dimg error");
    
    let ignore = disk.standardize(0);

    disk.delete("ASCEND2.TXT").expect("dimg error");

    ignore
}

#[test]
fn rename_delete_dsk() {
    // Reference disk was created using AppleWin.
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = cpm::Disk::from_img(Box::new(img),DiskParameterBlock::create(&names::A2_DOS33_KIND),[2,2,3]).expect("bad setup");
    disk.format(&String::from("TEST"),None).expect("could not format");

    let _ignore = build_ren_del(&mut disk);

    // MS-BASIC and PIP seem to put some buffered stuff into the data, so we cannot easily test the entire disk.
    // For now just see if the directory came out right.

    let mut emulator_disk = a2kit::create_fs_from_file(&Path::new("tests").join("cpm-ren-del.dsk").to_str().unwrap()).expect("read error");
    for block in 0..1 {
        let addr = Block::CPM((block,3,3));
        let actual = disk.get_img().read_block(addr).expect("bad sector access");
        let expected = emulator_disk.get_img().read_block(addr).expect("bad sector access");
        for row in 0..16 {
            let mut fmt_actual = String::new();
            let mut fmt_expected = String::new();
            let offset = row*32;
            write!(&mut fmt_actual,"{:02X?}",&actual[offset..offset+32].to_vec()).expect("format error");
            write!(&mut fmt_expected,"{:02X?}",&expected[offset..offset+32].to_vec()).expect("format error");
            assert_eq!(fmt_actual,fmt_expected," at block {}, row {}",block,row)
        }
    }

    //disk.compare(&Path::new("tests").join("cpm-ren-del.dsk"),&ignore);
}

