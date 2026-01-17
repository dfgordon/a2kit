use assert_cmd::cargo; // Add methods on commands
use std::path::Path;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

#[test]
fn pack_non_ascii() -> STDRESULT {
    let txt = std::fs::read(&Path::new("tests").
        join("fimg").join("unicode.txt")).expect("failed to read test file");
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    cmd.arg("pack")
        .arg("-t").arg("txt")
        .arg("-o").arg("dos33")
        .arg("-f").arg("unicode")
        .write_stdin(txt)
        .assert()
        .failure();

    Ok(())
}

// This compares our AppleSingle with an AppleSingle created using CiderPress II.
// It is byte-for-byte, and so depends on us writing entries in the same order.
// N.b. there is some hard coded masking to ignore the timestamps, which will only
// work for this specific file.
#[test]
fn create_applesingle_prodos() -> STDRESULT {
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let expected = std::fs::read(&Path::new("tests").
        join("fimg").join("test-as-create.as")).expect("failed to read test file");
    let output = cmd.arg("get")
        .arg("-d").arg(Path::new("tests").join("prodos-bigfiles.woz"))
        .arg("-t").arg("as")
        .arg("-f").arg("sapling")
        .assert()
        .success()
        .get_output().clone();

    assert_eq!(output.stdout[0..0x51],expected[0..0x51]);
    assert_eq!(output.stdout[0x61..],expected[0x61..]);
    Ok(())
}

// Can we make an AppleSingle from a native file image and go back again,
// of course will not work on sparse files.
#[test]
fn applesingle_invertibility() -> STDRESULT {
    
    // get the native file image
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let output = cmd.arg("get")
        .arg("-d").arg(Path::new("tests").join("prodos-smallfiles.do"))
        .arg("-t").arg("any")
        .arg("-f").arg("thechip")
        .assert()
        .success()
        .get_output().clone();
    let fimg = output.stdout;
    let fimg_original = fimg.clone();

    // unpack as AppleSingle
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let output = cmd.arg("unpack")
        .arg("-t").arg("as")
        .write_stdin(fimg)
        .assert()
        .success()
        .get_output().clone();
    let r#as = output.stdout;

    // pack as native file image again
    let mut cmd = cargo::cargo_bin_cmd!("a2kit");
    let output = cmd.arg("pack")
        .arg("-t").arg("as")
        .arg("-o").arg("prodos")
        .arg("-f").arg("thechip")
        .write_stdin(r#as)
        .assert()
        .success()
        .get_output().clone();
    let fimg_restored = output.stdout;

    // normalize packing-result with padding and prodos version
    let mut fimg = a2kit::fs::FileImage::from_json(&String::from_utf8(fimg_restored).expect("bad utf8")).expect("could not deserialize");
    let dat = fimg.sequence();
    fimg.desequence(&dat, Some(0));
    fimg.version = vec![0x24];

    assert_eq!(fimg.to_json(None).as_bytes(),fimg_original);
    Ok(())
}
