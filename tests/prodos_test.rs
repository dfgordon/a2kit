// test of prodos disk image module
use std::path::Path;
use a2kit::prodos;
use chrono;

#[test]
fn test_format() {
    let mut disk = prodos::Disk::new(280);
    disk.format(&String::from("NEW.DISK"),true,
        Some(chrono::NaiveDate::from_ymd(2022,08,31).and_hms(3, 48, 0)));
    let actual = disk.to_po_img();
    let expected = std::fs::read(Path::new("tests").join("prodos-blank.po")).expect("failed to read test image file");
    // check this block by block otherwise we get a big printout
    for block in 0..280 {
        let abuf = actual[block*512..(block+1)*512].to_vec();
        let ebuf = expected[block*512..(block+1)*512].to_vec();
        assert_eq!(abuf,ebuf," at block {}",block);
    }
}
