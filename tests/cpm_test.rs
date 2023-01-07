// test of CP/M disk image module
use std::path::Path;
use std::str::FromStr;
use a2kit::fs::{cpm,TextEncoder,DiskFS};
use a2kit::img::{dsk_do,names};
use a2kit_macro::DiskStruct;

// Some lines we entered in the emulator using ED.COM.
// One thing to note: if we would use an odd number of
// CP/M "sectors" (128 byte records) ED would leave copious
// trailing data; so we are careful to fill an even number.

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
    let emulator_disk = a2kit::create_fs_from_bytestream(&img).expect("file not found");

    // check text file
    let (_z,raw) = emulator_disk.read_text("POLARIS.TXT").expect("error");
    let txt = cpm::types::SequentialText::from_bytes(&raw);
    let encoder = cpm::types::Encoder::new(vec![]);
    assert_eq!(txt.text,encoder.encode(ED_TEST).unwrap());
    assert_eq!(encoder.decode(&txt.text).unwrap(),String::from(ED_TEST));
}

#[test]
fn write_small() {
    // Formatting: FORMAT.COM, writing: ED.COM, emulator: Virtual II
    // This tests small CP/M text files
    let img = dsk_do::DO::create(35, 16);
    let mut disk = cpm::Disk::from_img(Box::new(img),cpm::types::DiskParameterBlock::create(&names::A2_DOS33_KIND),[2,2,3]);
    disk.format("test",None).expect("failed to format disk");

    // save the text
    disk.write_text("POLARIS.BAK",&Vec::new()).expect("error");
    let txt_data = cpm::types::SequentialText::from_str(ED_TEST).expect("text encode error");
    disk.write_text("POLARIS.TXT",&txt_data.to_bytes()).expect("write error");

    disk.compare(&Path::new("tests").join("cpm-smallfiles.dsk"),&disk.standardize(0));
}

#[test]
fn out_of_space() {
    let img = a2kit::img::dsk_do::DO::create(35,16);
    let mut disk = cpm::Disk::from_img(Box::new(img),cpm::types::DiskParameterBlock::create(&names::A2_DOS33_KIND),[2,2,3]);
    let big: Vec<u8> = vec![0;0x7f00];
    disk.format(&String::from("TEST"),None).expect("could not format");
    disk.bsave("f1",&big,0x800,None).expect("error");
    disk.bsave("f2",&big,0x800,None).expect("error");
    disk.bsave("f3",&big,0x800,None).expect("error");
    match disk.bsave("f4",&big,0x800,None) {
        Ok(l) => assert!(false,"wrote {} but should be disk full",l),
        Err(e) => match e.to_string().as_str() {
            "disk full" => assert!(true),
            _ => assert!(false,"unexpected error")
        }
    }
}
