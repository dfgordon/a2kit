// test of prodos disk image module
use std::path::Path;

#[test]
fn size_of_do() {
    let img = std::fs::read(&Path::new("tests").join("prodos-smallfiles.do")).expect("failed to read test image file");
    let mut disk = a2kit::create_img_from_bytestream(&img,None).expect("file not found");
    assert_eq!(disk.nominal_capacity().unwrap(),280*512);
    assert_eq!(disk.actual_capacity().expect("capacity scan failed"),280*512);
}

#[test]
fn size_of_woz1() {
    let buf = Path::new("tests").join("prodos-bigfiles.woz");
    let woz1_path = buf.to_str().expect("could not get path");
    let mut disk = a2kit::create_img_from_file(woz1_path).expect("could not interpret image");
    assert_eq!(disk.nominal_capacity().unwrap(),280*512);
    assert_eq!(disk.actual_capacity().expect("capacity scan failed"),280*512);
}
