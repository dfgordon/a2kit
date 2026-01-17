use assert_cmd::cargo; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::path::Path;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

#[test]
fn parse_simple_file() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("verify")
        .arg("-t").arg("atxt")
        .pipe_stdin(Path::new("tests").join("applesoft").join("test.bas"))?
        .assert()
        .success()
        .stderr(predicate::str::contains("Passing"));
    Ok(())
}

#[test]
fn invalid_file_type() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("verify")
        .arg("-t").arg("atxt1")
        .pipe_stdin(Path::new("tests").join("applesoft").join("test.bas"))?
        .assert()
        .failure()
        .stderr(predicate::str::contains("atxt1"));
    Ok(())
}

#[test]
fn catalog_cpm() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected = 
r#"
A: POLARIS  BAK : POLARIS  TXT

"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("cpm-smallfiles.dsk"))
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn catalog_cpm_wildcard_full() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected = 
r#"Directory for Drive A: User 0

    Name     Bytes   Recs   Attributes      Name     Bytes   Recs   Attributes\s*
------------ ------ ------ ------------ ------------ ------ ------ ------------

POLARIS  TXT     1k      4 Dir RW\s*

Total Bytes     =      1k  Total Records =       4  Files Found =    1
Total 1k Blocks =      1   Occupied/Tot Entries For Drive A:    2/  48"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("cpm-smallfiles.dsk"))
        .arg("-f").arg("*.txt[full]")
        .assert()
        .success()
        .stdout(predicate::str::is_match(expected).expect("regex error"));
    Ok(())
}

#[test]
fn catalog_dos32() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected = 
r#"DISK VOLUME 254

 I 004 HELLO
 T 010 TREE1
 T 019 TREE2
 B 066 SAPLING"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("dos32-bigfiles.woz"))
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn catalog_dos33() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected = 
r#"DISK VOLUME 254

 A 004 HELLO
 T 010 TREE1
 T 019 TREE2
 B 066 SAPLING"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("dos33-bigfiles.woz"))
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn catalog_prodos() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected =
r#".NEW.DISK

 NAME\s+TYPE\s+BLOCKS\s+MODIFIED\s+CREATED\s+ENDFILE\s+SUBTYPE

 HELLO\s+BAS\s+3.*753\s+2049
 TREE1\s+TXT\s+5.*256018\s+128
 TREE2\s+TXT\s+7.*508018\s+127
 SAPLING\s+BIN\s+33.*16384\s+16384

BLOCKS FREE: 225\s+BLOCKS USED: 55\s+TOTAL BLOCKS: 280"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("prodos-bigfiles.woz"))
        .assert()
        .success()
        .stdout(predicate::str::is_match(expected).expect("regex err"));
    Ok(())
}

#[test]
fn catalog_pascal() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected =
r#"BLANK:
HELLO.TEXT\s+4.*TEXT
TEST2.TEXT\s+4.*TEXT
TEST3.TEXT\s+4.*TEXT

3/3 files<listed/in-dir>, 18 blocks used, 262 unused, 262 in largest"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("pascal-smallfiles.do"))
        .assert()
        .success()
        .stdout(predicate::str::is_match(expected).expect("regex err"));
    Ok(())
}

#[test]
fn catalog_msdos() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected =
r#" Volume in drive A is NEW DISK 1
 Directory of A:\\

DSKBLD\s+BAT\s+378.*
DSKBLD\s+BAS\s+99.*
DIR1\s+<DIR>.*
DIR3\s+<DIR>.*
\s+4 File\(s\)\s+135168 bytes free"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("msdos-ren-del.img"))
        .assert()
        .success()
        .stdout(predicate::str::is_match(expected).expect("regex err"));
    Ok(())
}

#[test]
fn catalog_msdos_wildcard() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected =
r#" Volume in drive A is NEW DISK 1
 Directory of A:\\DIR1\\SUB\*\.\*

SUBDIR1\s+<DIR>.*
\s+1 File\(s\)\s+135168 bytes free"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("msdos-ren-del.img"))
        .arg("-f").arg("/dir1/sub*.*")
        .assert()
        .success()
        .stdout(predicate::str::is_match(expected).expect("regex err"));
    Ok(())
}

// N.b. extensive tokenization tests are in the language modules
#[test]
fn tokenize_stdin() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let test_prog =
r#"10 home
20 print a$
"#;
    let toks: Vec<u8> = vec![7,8,10,0,0x97,0,15,8,0x14,0,0xba,0x41,0x24,0,0,0];
    let output = cmd.arg("tokenize")
        .arg("-t").arg("atxt").arg("-a").arg("2049")
        .write_stdin(test_prog)
        .assert()
        .success()
        .get_output().clone();
        
        assert_eq!(output.stdout,toks);
        
    Ok(())
}

#[test]
fn detokenize_stdin() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let test_prog =
r#"10  HOME 
20  PRINT A$
"#;
    let toks: Vec<u8> = vec![7,8,10,0,0x97,0,15,8,0x14,0,0xba,0x41,0x24,0,0,0];
    let output = cmd.arg("detokenize")
        .arg("-t").arg("atok")
        .write_stdin(toks)
        .assert()
        .success()
        .get_output().clone();

        assert_eq!(String::from_utf8_lossy(&output.stdout),test_prog);
        
    Ok(())
}
