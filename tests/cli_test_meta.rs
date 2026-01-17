use assert_cmd::cargo; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
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
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos33.do");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("do").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();
    // check the metadata
    cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn put_get_meta_woz2() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("woz2.woz");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("woz2").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();

    // set items in the INFO and META chunks
    cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("put")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .write_stdin(WOZ2_PUT_ITEMS.as_bytes())
        .assert()
        .success();
    
    // check the metadata
    cmd = cargo::cargo_bin_cmd!("a2kit");
    let output = cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .assert()
        .success()
        .get_output().clone();

    // take string to object and back to get the format consistent
    let woz2_exp = WOZ2_EXPECTED.replace("2.2.0",PK_VERS);
    let expected = json::stringify_pretty(json::parse(&woz2_exp).expect("json parsing failed"),4);
    assert_eq!(&String::from_utf8(output.stdout).unwrap().trim_end(),&expected.trim_end());

    Ok(())
}

#[test]
fn put_get_meta_woz2_filtered() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("woz2.woz");
    // first make disk
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("woz2").arg("-o").arg("dos33")
        .arg("-d").arg(&dimg_path)
        .assert()
        .success();

    // set items in the INFO and META chunks, but use filter to pass only META
    cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("put")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .arg("-f").arg("/woz2/meta/")
        .write_stdin(WOZ2_PUT_ITEMS.as_bytes())
        .assert()
        .success();
    
    // check the metadata
    cmd = cargo::cargo_bin_cmd!("a2kit");
    let output = cmd.arg("get")
        .arg("-t").arg("meta").arg("-d").arg(&dimg_path)
        .assert()
        .success()
        .get_output().clone();

    // take string to object and back to get the format consistent
    let woz2_exp = WOZ2_EXPECTED_FILTERED.replace("2.2.0",PK_VERS);
    let expected = json::stringify_pretty(json::parse(&woz2_exp).expect("json parsing failed"),4);
    assert_eq!(&String::from_utf8(output.stdout).unwrap().trim_end(),&expected.trim_end());

    Ok(())
}
