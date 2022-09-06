use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::{Command,Stdio}; // Run programs
use std::path::Path;
use std::fs::File;

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
