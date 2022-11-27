// test of DOS 3.2 support, which resides in the dos33 disk image module
use std::path::Path;
use a2kit::fs::dos33;
use a2kit::disk_base::TextEncoder;
use a2kit::disk_base::{ItemType,DiskFS};
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

#[test]
fn out_of_space() {
    let mut disk = dos33::Disk::new();
    let big: Vec<u8> = vec![0;0x7f00];
    disk.init32(254,true);
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
