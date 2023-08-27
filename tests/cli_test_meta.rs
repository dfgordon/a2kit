use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::{Command,Stdio}; // Run programs
use std::io::Write;
use tempfile;
use json;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

const PK_VERS: &str = env!("CARGO_PKG_VERSION");

const WOZ2_PUT_ITEMS: &str = "
{
    \"woz2\": {
        \"info\": {
            \"required_ram\": {
                \"_raw\": \"4000\",
                \"_pretty\": \"64K\"
            }
        },
        \"meta\": {
            \"excellence\": \"remarkable\",
            \"problems\": \"minimal\"
        }
    }
}
";

const WOZ2_EXPECTED: &str = "
{
    \"woz2\": {
        \"info\": {
            \"disk_type\": {
                \"_raw\": \"01\",
                \"_pretty\": \"Apple 5.25 inch\"
            },
            \"write_protected\": \"00\",
            \"synchronized\": \"00\",
            \"cleaned\": \"00\",
            \"creator\": \"a2kit v2.2.0\",
            \"disk_sides\": \"01\",
            \"boot_sector_format\": {
                \"_raw\": \"01\",
                \"_pretty\": \"Boots 16-sector\"
            },
            \"optimal_bit_timing\": \"20\",
            \"compatible_hardware\": {
                \"_raw\": \"0000\",
                \"_pretty\": \"unknown\"
            },
            \"required_ram\": {
                \"_raw\": \"4000\",
                \"_pretty\": \"64K\"
            },
            \"largest_track\": {
                \"_raw\": \"0d00\",
                \"_pretty\": \"13 blocks\"
            }
        },
        \"meta\": {
            \"excellence\": \"remarkable\",
            \"problems\": \"minimal\"
        }
    }
}
";

const WOZ2_EXPECTED_FILTERED: &str = "
{
    \"woz2\": {
        \"info\": {
            \"disk_type\": {
                \"_raw\": \"01\",
                \"_pretty\": \"Apple 5.25 inch\"
            },
            \"write_protected\": \"00\",
            \"synchronized\": \"00\",
            \"cleaned\": \"00\",
            \"creator\": \"a2kit v2.2.0\",
            \"disk_sides\": \"01\",
            \"boot_sector_format\": {
                \"_raw\": \"01\",
                \"_pretty\": \"Boots 16-sector\"
            },
            \"optimal_bit_timing\": \"20\",
            \"compatible_hardware\": {
                \"_raw\": \"0000\",
                \"_pretty\": \"unknown\"
            },
            \"required_ram\": {
                \"_raw\": \"0000\",
                \"_pretty\": \"unknown\"
            },
            \"largest_track\": {
                \"_raw\": \"0d00\",
                \"_pretty\": \"13 blocks\"
            }
        },
        \"meta\": {
            \"excellence\": \"remarkable\",
            \"problems\": \"minimal\"
        }
    }
}
";

#[test]
fn get_meta_do() -> STDRESULT {
    let expected = json::stringify_pretty(json::parse("{\"do\":{}}").expect("json parsing failed"),4);
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos33.do");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("do").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();
    // check the metadata
    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn put_get_meta_woz2() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("woz2.woz");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("woz2").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();

    // set items in the INFO and META chunks
    cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("put")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(WOZ2_PUT_ITEMS.as_bytes()).expect("Failed to write to stdin");
    });
    child.wait_with_output().expect("Failed to read stdout");
    
    // check the metadata
    cmd = Command::cargo_bin("a2kit")?;
    let child = cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");    
    let output = child.wait_with_output().expect("Failed to read stdout");
    // take string to object and back to get the format consistent
    let woz2_exp = WOZ2_EXPECTED.replace("2.2.0",PK_VERS);
    let expected = json::stringify_pretty(json::parse(&woz2_exp).expect("json parsing failed"),4);
    assert_eq!(&String::from_utf8(output.stdout).unwrap().trim_end(),&expected.trim_end());

    Ok(())
}

#[test]
fn put_get_meta_woz2_filtered() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("woz2.woz");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("woz2").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();

    // set items in the INFO and META chunks, but use filter to pass only META
    cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("put")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .arg("-f").arg("/woz2/meta/")
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(WOZ2_PUT_ITEMS.as_bytes()).expect("Failed to write to stdin");
    });
    child.wait_with_output().expect("Failed to read stdout");
    
    // check the metadata
    cmd = Command::cargo_bin("a2kit")?;
    let child = cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");    
    let output = child.wait_with_output().expect("Failed to read stdout");
    // take string to object and back to get the format consistent
    let woz2_exp = WOZ2_EXPECTED_FILTERED.replace("2.2.0",PK_VERS);
    let expected = json::stringify_pretty(json::parse(&woz2_exp).expect("json parsing failed"),4);
    assert_eq!(&String::from_utf8(output.stdout).unwrap().trim_end(),&expected.trim_end());

    Ok(())
}
