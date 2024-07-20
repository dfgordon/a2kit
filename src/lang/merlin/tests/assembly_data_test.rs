//! Test of data-like pseudo operations.

use crate::lang::merlin::MerlinVersion;
use crate::lang::merlin::settings::Settings;

#[cfg(test)]
fn test_assembler(test_code: &str,expected: &str,vers: MerlinVersion) {
    let mut config = Settings::new();
	let mut assembler = super::super::assembly::Assembler::new();
    config.version = vers;
    assembler.set_config(config);
	// get actual into hex string
	let bytes = assembler.spot_assemble(test_code.to_string(),0,1).expect("assembler failed");
    let actual = hex::encode_upper(bytes);
	assert_eq!(actual,expected.replace(" ",""));
}

#[test]
fn dfb() {
    let test_code = "   dfb   $030201,>$030201,^$030201\n";
    let expected = "01 02 03";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   dfb   $ff,150,%11\n";
    let expected = "FF 96 03";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn da() {
    let test_code = "   da    $030201,>$030201,^$030201\n";
    let expected = "01 02 02 03 03 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   da    $fded,40000,%101101011\n";
    let expected = "ED FD 40 9C 6B 01";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn ddb() {
    let test_code = "   ddb   $030201,>$030201,^$030201\n";
    let expected = "02 01 03 02 00 03";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   ddb   $fded,40000,>%101101011\n";
    let expected = "FD ED 9C 40 00 01";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn adr() {
    let test_code = "   adr   $04030201,>$04030201,^$04030201\n";
    let expected = "01 02 03 02 03 04 03 04 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   adr   $01fded,100000,%101101011\n";
    let expected = "ED FD 01 A0 86 01 6B 01 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn adrl() {
    let test_code = "   adrl  $04030201,>$04030201,^$04030201\n";
    let expected = "01 02 03 04 02 03 04 00 03 04 00 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   adrl  $01fded,100000,%101101011\n";
    let expected = "ED FD 01 00 A0 86 01 00 6B 01 00 00";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}

#[test]
fn ds() {
    let test_code = "   ds  5,$fded\n";
    let expected = "ED ED ED ED ED";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
    let test_code = "   ds  5,>$fded\n";
    let expected = "FD FD FD FD FD";
    test_assembler(test_code,expected,MerlinVersion::Merlin16Plus);
}
