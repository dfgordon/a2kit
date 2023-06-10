use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs
use tempfile;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

#[test]
fn mk_dos33_do() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos33.do");
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("do").arg("-o").arg("dos33")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_dos33_bad_ext() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos33badext.po");
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("do").arg("-o").arg("dos33")
        .arg("-d").arg(dimg_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Extension was"));
    Ok(())
}

#[test]
fn mk_prodos_woz1() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("prodoswoz1.woz");
    cmd.arg("mkdsk")
        .arg("-v").arg("new.disk").arg("-t").arg("woz1").arg("-o").arg("prodos")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_prodos_woz2() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("prodoswoz2.woz");
    cmd.arg("mkdsk")
        .arg("-v").arg("new.disk").arg("-t").arg("woz2").arg("-o").arg("prodos")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_prodos_bad_vol() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("prodosbadvol.po");
    cmd.arg("mkdsk")
        .arg("-v").arg("new_disk").arg("-t").arg("po").arg("-o").arg("prodos")
        .arg("-d").arg(dimg_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("bad file name"));
    Ok(())
}

#[test]
fn mk_cpm_osb() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("osb.imd");
    cmd.arg("mkdsk")
        .arg("-t").arg("imd").arg("-o").arg("cpm2")
        .arg("-k").arg("5.25in-osb-sd")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_pascal() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("pasc.dsk");
    cmd.arg("mkdsk")
        .arg("-v").arg("myvol").arg("-t").arg("do").arg("-o").arg("pascal")
        .arg("-k").arg("5.25in")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}
