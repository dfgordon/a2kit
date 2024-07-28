//! Test of string-like pseudo operations.

use crate::lang::merlin::MerlinVersion;
use crate::lang::merlin::settings::Settings;

#[cfg(test)]
fn test_assembler(test_code: &str,expected: &str,vers: MerlinVersion) {
    let mut config = Settings::new();
	let mut assembler = super::super::assembly::Assembler::new();
    config.version = vers;
    assembler.set_config(config);
	// get actual into hex string
	let bytes = assembler.spot_assemble(test_code.to_string(),0,1,None).expect("assembler failed");
    let actual = hex::encode_upper(bytes);
	assert_eq!(actual,expected.replace(" ",""));
}

#[test]
fn asc() {
    let test_code = "   asc 'Call me Ishmael',00\n";
    let expected = "43 61 6C 6C 20 6D 65 20 49 73 68 6D 61 65 6C 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   asc 'Call',20,'me',20,'Ishmael',00  ; rem\n";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn asc_neg() {
    let test_code = "   asc \"Call me Ishmael\"\n";
    let expected = "C3 E1 EC EC A0 ED E5 A0 C9 F3 E8 ED E1 E5 EC";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   asc \"Call\",20,\"me Ishmael\"\n";
    let expected = "C3 E1 EC EC 20 ED E5 A0 C9 F3 E8 ED E1 E5 EC";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn dci() {
    let test_code = "   dci 'Call me Ishmael',00\n";
    let expected = "43 61 6C 6C 20 6D 65 20 49 73 68 6D 61 65 EC 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   dci   \"Call\",20,'me Ishmael',00\n";
    let expected = "C3 E1 EC 6C 20 6D 65 20 49 73 68 6D 61 65 EC 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn inv() {
    let test_code = "   inv 'CALL ME ISHMAEL:'\n";
    let expected = "03 01 0C 0C 20 0D 05 20 09 13 08 0D 01 05 0C 3A";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   inv 'CALL ME ISHMAEL',00\n";
    let expected = "03 01 0C 0C 20 0D 05 20 09 13 08 0D 01 05 0C 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn inv_lower() {
    let test_code = "   inv 'Call me Ishmael'\n";
    let expected = "03 61 6C 6C 20 6D 65 20 09 73 68 6D 61 65 6C";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let expected = "03 21 2C 2C 20 2D 25 20 09 33 28 2D 21 25 2C";
    test_assembler(test_code,expected,MerlinVersion::Merlin8);
}

#[test]
fn fls() {
    let test_code = "   fls 'CALL ME ISHMAEL:'\n";
    let expected = "43 41 4C 4C 60 4D 45 60 49 53 48 4D 41 45 4C 7A";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn rev() {
    let test_code = "   rev \"racecars\"\n";
    let expected = "F3 F2 E1 E3 E5 E3 E1 F2";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn str() {
    let test_code = "   str \"Call me Ishmael\"\n";
    let expected = "0F C3 E1 EC EC A0 ED E5 A0 C9 F3 E8 ED E1 E5 EC";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn strl() {
    let test_code = "   strl \"Call me Ishmael\"\n";
    let expected = "0F 00 C3 E1 EC EC A0 ED E5 A0 C9 F3 E8 ED E1 E5 EC";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}
