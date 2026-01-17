use assert_cmd::cargo; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use tempfile;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

#[test]
fn mk_dos33_do() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
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
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
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
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
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
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
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
fn mk_dos33_2mg_nib() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos2mgnib.2mg");
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("2mg").arg("-o").arg("dos33")
        .arg("-w").arg("nib").arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_dos33_2mg_bad_wrap() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos2mgnib.2mg");
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("2mg").arg("-o").arg("dos33")
        .arg("-w").arg("d13").arg("-d").arg(dimg_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
    Ok(())
}

#[test]
fn mk_prodos_bad_vol() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
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
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("osb.imd");
    cmd.arg("mkdsk")
        .arg("-t").arg("imd").arg("-o").arg("cpm2")
        .arg("-k").arg("5.25in-osb-sssd")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_pascal() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("pasc.dsk");
    cmd.arg("mkdsk")
        .arg("-v").arg("myvol").arg("-t").arg("do").arg("-o").arg("pascal")
        .arg("-k").arg("5.25in-apple-16")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_fat_imd() -> STDRESULT {
    let dir = tempfile::tempdir()?;
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let mut dimg_path = dir.path().join("fat8.imd");
    cmd.arg("mkdsk")
        .arg("-t").arg("imd").arg("-o").arg("fat")
        .arg("-k").arg("5.25in-ibm-ssdd8")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    cmd = cargo::cargo_bin_cmd!("a2kit");
    dimg_path = dir.path().join("fat9.imd");
    cmd.arg("mkdsk")
        .arg("-t").arg("imd").arg("-o").arg("fat")
        .arg("-k").arg("5.25in-ibm-ssdd9")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_fat_td0() -> STDRESULT {
    let dir = tempfile::tempdir()?;
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let mut dimg_path = dir.path().join("fat720.td0");
    cmd.arg("mkdsk")
        .arg("-t").arg("td0").arg("-o").arg("fat")
        .arg("-k").arg("3.5in-ibm-720")
        .arg("-d").arg(dimg_path)
        .arg("-v").arg("volume 1")
        .assert()
        .success();
    cmd = cargo::cargo_bin_cmd!("a2kit");
    dimg_path = dir.path().join("fat1440.td0");
    cmd.arg("mkdsk")
        .arg("-t").arg("td0").arg("-o").arg("fat")
        .arg("-k").arg("3.5in-ibm-1440")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}

#[test]
fn mk_fat_img() -> STDRESULT {
    let dir = tempfile::tempdir()?;
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let mut dimg_path = dir.path().join("fat8.img");
    cmd.arg("mkdsk")
        .arg("-t").arg("img").arg("-o").arg("fat")
        .arg("-k").arg("5.25in-ibm-dsdd8")
        .arg("-d").arg(dimg_path)
        .arg("-v").arg("volume 1")
        .assert()
        .success();
    cmd = cargo::cargo_bin_cmd!("a2kit");
    dimg_path = dir.path().join("fat9.img");
    cmd.arg("mkdsk")
        .arg("-t").arg("img").arg("-o").arg("fat")
        .arg("-k").arg("5.25in-ibm-dsdd9")
        .arg("-d").arg(dimg_path)
        .assert()
        .success();
    Ok(())
}
