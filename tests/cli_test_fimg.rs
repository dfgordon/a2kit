use assert_cmd::prelude::*; // Add methods on commands
use std::process::{Command,Stdio}; // Run programs
use std::path::Path;
use std::io::Write;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

#[test]
fn pack_non_ascii() -> STDRESULT {
    let txt = std::fs::read(&Path::new("tests").
        join("fimg").join("unicode.txt")).expect("failed to read test file");
    let mut cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("pack")
        .arg("-t").arg("txt")
        .arg("-o").arg("dos33")
        .arg("-f").arg("unicode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().expect("could not spawn pack");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(&txt).expect("Failed to write to stdin");
    });
    
    match child.wait_with_output() {
        Ok(res) => assert_eq!(res.status.success(),false),
        Err(e) => panic!("unexpected error {}",e)
    }
    Ok(())
}

// This compares our AppleSingle with an AppleSingle created using CiderPress II.
// It is byte-for-byte, and so depends on us writing entries in the same order.
// N.b. there is some hard coded masking to ignore the timestamps, which will only
// work for this specific file.
#[test]
fn create_applesingle_prodos() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let expected = std::fs::read(&Path::new("tests").
        join("fimg").join("test-as-create.as")).expect("failed to read test file");
    let child = cmd.arg("get")
        .arg("-d").arg(Path::new("tests").join("prodos-bigfiles.woz"))
        .arg("-t").arg("as")
        .arg("-f").arg("sapling")
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");    
    let output = child.wait_with_output().expect("Failed to read stdout");
    assert_eq!(output.stdout[0..0x51],expected[0..0x51]);
    assert_eq!(output.stdout[0x61..],expected[0x61..]);
    Ok(())
}

// Can we make an AppleSingle from a native file image and go back again,
// of course will not work on sparse files.
#[test]
fn applesingle_invertibility() -> STDRESULT {
    
    // get the native file image
    let mut cmd = Command::cargo_bin("a2kit")?;
    let child = cmd.arg("get")
        .arg("-d").arg(Path::new("tests").join("prodos-smallfiles.do"))
        .arg("-t").arg("any")
        .arg("-f").arg("thechip")
        .stdout(Stdio::piped())
        .spawn()
        .expect("could not spawn get");    
    let fimg = child.wait_with_output().expect("Failed to read stdout").stdout;
    let fimg_original = fimg.clone();

    // unpack as AppleSingle
    let mut cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("unpack")
        .arg("-t").arg("as")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("could not spawn unpack");    
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(&fimg).expect("Failed to write to stdin");
    });
    let r#as = child.wait_with_output().expect("Failed to read stdout").stdout;

    // pack as native file image again
    let mut cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("pack")
        .arg("-t").arg("as")
        .arg("-o").arg("prodos")
        .arg("-f").arg("thechip")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("could not spawn pack");    
    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(&r#as).expect("Failed to write to stdin");
    });
    let fimg_restored = child.wait_with_output().expect("Failed to read stdout").stdout;

    // normalize packing-result with padding and prodos version
    let mut fimg = a2kit::fs::FileImage::from_json(&String::from_utf8(fimg_restored).expect("bad utf8")).expect("could not deserialize");
    let dat = fimg.sequence();
    fimg.desequence(&dat, Some(0));
    fimg.version = vec![0x24];

    assert_eq!(fimg.to_json(None).as_bytes(),fimg_original);
    Ok(())
}
