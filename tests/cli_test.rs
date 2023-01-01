use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::{Command,Stdio}; // Run programs
use std::path::Path;
use std::fs::File;
use std::io::Write;

#[test]
fn parse_simple_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
    if let Ok(fd) = File::open(Path::new("tests").join("test.bas")) {
        cmd.arg("verify")
            .arg("-t").arg("atxt")
            .stdin(Stdio::from(fd))
            .assert()
            .success()
            .stderr(predicate::str::contains("Syntax OK"));
    }
    Ok(())
}

#[test]
fn invalid_file_type() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
    if let Ok(fd) = File::open(Path::new("tests").join("test.bas")) {
        cmd.arg("verify")
            .arg("-t").arg("atxt1")
            .stdin(Stdio::from(fd))
            .assert()
            .failure()
            .stderr(predicate::str::contains("atxt1"));
    }
    Ok(())
}

#[test]
fn catalog_cpm() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let expected = 
r#"A>USER 0
A>DIR

A: POLARIS  BAK : POLARIS  TXT

found 1 user
"#;
    cmd.arg("catalog")
        .arg("-d").arg(Path::new("tests").join("cpm-smallfiles.dsk"))
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn catalog_dos32() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
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
fn catalog_dos33() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
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
fn catalog_prodos() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
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
fn catalog_pascal() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
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

// N.b. extensive tokenization tests are in the language modules
#[test]
fn tokenize_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let test_prog =
r#"10 home
20 print a$
"#;
    let toks: Vec<u8> = vec![7,8,10,0,0x97,0,15,8,0x14,0,0xba,0x41,0x24,0,0,0];
    let mut child = cmd.arg("tokenize")
        .arg("-t").arg("atxt").arg("-a").arg("2049")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || {
            stdin.write_all(test_prog.as_bytes()).expect("Failed to write to stdin");
        });
        
        let output = child.wait_with_output().expect("Failed to read stdout");
        assert_eq!(output.stdout,toks);
        
    Ok(())
}

#[test]
fn detokenize_stdin() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let test_prog =
r#"10  HOME 
20  PRINT A$
"#;
    let toks: Vec<u8> = vec![7,8,10,0,0x97,0,15,8,0x14,0,0xba,0x41,0x24,0,0,0];
    let mut child = cmd.arg("detokenize")
        .arg("-t").arg("atok")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || {
            stdin.write_all(&toks).expect("Failed to write to stdin");
        });
        
        let output = child.wait_with_output().expect("Failed to read stdout");
        assert_eq!(String::from_utf8_lossy(&output.stdout),test_prog);
        
    Ok(())
}
