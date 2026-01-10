use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs
use std::path::PathBuf;
use tempfile;
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

const EXPECTED_BAS: &str = r#"10 D$ =  CHR$ (4)
20  INPUT "(S)MALL, (B)IG, (R)ENAME/DELETE? ";A$
30  IF A$ = "S" THEN 1000
40  IF A$ = "B" OR A$ = "R" THEN 2000
50  PRINT "INVALID CHOICE"
60  END 
1000  POKE 768,6: POKE 769,5: POKE 770,0: POKE 771,2
1010  PRINT D$;"BSAVE THECHIP,A768,L4"
1020  PRINT D$;"OPEN THETEXT"
1030  PRINT D$;"WRITE THETEXT"
1040  PRINT "HELLO FROM EMULATOR"
1050  PRINT D$;"CLOSE THETEXT"
1999  END 
2000  PRINT D$;"OPEN TREE1,L128"
2010  PRINT D$;"WRITE TREE1,R2000"
2020  PRINT "HELLO FROM TREE 1"
2030  PRINT D$;"CLOSE TREE1"
2040  PRINT D$;"OPEN TREE2,L127"
2050  PRINT D$;"WRITE TREE2,R2000"
2060  PRINT "HELLO FROM TREE 2"
2070  PRINT D$;"WRITE TREE2,R4000"
2080  PRINT "HELLO FROM TREE 2"
2090  PRINT D$;"CLOSE TREE2"
2100  FOR I = 16384 TO 32767: POKE I,256 * ((I - 16384) / 256 -  INT ((I - 16384) / 256)): NEXT 
2110  PRINT D$;"BSAVE SAPLING,A16384,L16384"
2120  IF A$ = "B" THEN  END 
2130  PRINT D$;"DELETE TREE2"
2140  PRINT D$;"RENAME SAPLING,SAP"
2150  PRINT D$;"RENAME TREE1,MYTREE1"
2160  END 
"#;

const EXPECTED_TOKS: &str = r#"
16 08 0A 00 97 3A B2 20 74 65 73 74 20 70 72 6F
67 72 61 6D 00 2A 08 14 00 BA 22 54 45 53 54 20
50 52 4F 47 52 41 4D 22 00 33 08 1E 00 B0 31 30
30 00 39 08 28 00 80 00 57 08 64 00 84 22 45 4E
54 45 52 20 22 3B 41 42 52 41 43 41 44 41 42 52
41 2C 42 2C 43 00 65 08 6E 00 BA 41 3B 42 3B 43
3A 3A BA 00 6B 08 78 00 B1 00 7D 08 82 00 B2 20
75 6E 72 65 61 63 68 61 62 6C 65 00 8B 08 8C 00
AD 41 41 31 32 33 C4 31 30 00 AC 08 96 00 AF 28
22 41 42 52 41 43 41 44 41 42 52 41 22 2C 41 42
52 41 43 41 44 41 42 52 41 29 00 00 00"#;

#[test]
fn host_to_do() -> STDRESULT {
    
    // make a disk image in the temp directory

    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("dos33.do");
    let host_path = PathBuf::from("tests").join("applesoft").join("test.bas");
    cmd.arg("mkdsk")
        .arg("-v").arg("254").arg("-t").arg("do").arg("-o").arg("dos33")
        .arg("-d").arg(dimg_path.clone())
        .assert()
        .success();

    // smart copy an Applesoft source to the image

    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("cp").arg(host_path).arg(dimg_path.clone()).assert().success();

    // check catalog

    let expected = 
r#"DISK VOLUME 254

 A 002 TEST.BAS"#;
    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("catalog")
        .arg("-d").arg(dimg_path.clone())
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));

    // check tokens

    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("get")
        .arg("-d").arg(dimg_path)
        .arg("-t").arg("atok")
        .arg("-f").arg("test.bas")
        .assert()
        .success()
        .stdout(predicate::eq(hex::decode(EXPECTED_TOKS
            .replace(" ","")
            .replace("\n","")
            .replace("\r",""))?));

    Ok(())
}

#[test]
fn basic_to_host() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_glob = PathBuf::from("tests").join("prodos-bigfiles.woz/hello");
    cmd.arg("cp").arg(dimg_glob).arg(dir.path())
        .assert()
        .success();
    let actual_bas = String::from_utf8(std::fs::read(&dir.path().join("HELLO"))?)?;
    assert_eq!(&actual_bas,EXPECTED_BAS);
    Ok(())
}

#[test]
fn glob_recs_to_host() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_glob = PathBuf::from("tests").join("prodos-bigfiles.woz/tree*");
    cmd.arg("cp").arg(dimg_glob).arg(dir.path())
        .assert()
        .success();
    let expected_tree1 = r#"{"fimg_type":"rec","record_length":128,"records":{"2000":["HELLO FROM TREE 1"]}}"#;
    // the records can come out in any order, with only 2 we can just explicitly check both possibilities
    let expected_tree2_1 = r#"{"fimg_type":"rec","record_length":127,"records":{"2000":["HELLO FROM TREE 2"],"4000":["HELLO FROM TREE 2"]}}"#;
    let expected_tree2_2 = r#"{"fimg_type":"rec","record_length":127,"records":{"4000":["HELLO FROM TREE 2"],"2000":["HELLO FROM TREE 2"]}}"#;
    let tree1 = String::from_utf8(std::fs::read(&dir.path().join("TREE1"))?)?;
    let tree2 = String::from_utf8(std::fs::read(&dir.path().join("TREE2"))?)?;
    assert_eq!(&tree1,expected_tree1);
    let tst = predicate::in_iter(vec![expected_tree2_1,expected_tree2_2]);
    assert_eq!(true,tst.eval(tree2.as_str()));
    Ok(())
}

#[test]
fn cpm_host_to_image() -> STDRESULT {
    let mut cmd = Command::cargo_bin("a2kit")?;
    let dir = tempfile::tempdir()?;
    let dimg_path = dir.path().join("osb.imd");
    std::fs::write(dir.path().join("GO.SUB"),"PIP B:PROG1.BAS=A:PROG1.BAS\nB:\nREN PROG2.BAS=PROG1.BAS\n")?;
    std::fs::write(dir.path().join("DATA"),&[0xff,0x01,0xe0,0xd5])?;

    cmd.arg("mkdsk")
        .arg("-t").arg("imd").arg("-o").arg("cpm2")
        .arg("-k").arg("5.25in-osb-sssd")
        .arg("-d").arg(dimg_path.clone())
        .assert()
        .success();

    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("cp")
        .arg(dir.path().join("GO.SUB"))
        .arg(dir.path().join("DATA"))
        .arg(dimg_path.clone())
        .assert().success();

    let expected = 
r#"A: GO       SUB : DATA"#;
    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("catalog")
        .arg("-d").arg(dimg_path.clone())
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));

    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("get")
        .arg("-d").arg(dimg_path.clone())
        .arg("-t").arg("bin")
        .arg("-f").arg("DATA")
        .assert()
        .success()
        .stdout(predicate::eq(vec![0xff,0x01,0xe0,0xd5]));

    cmd = Command::cargo_bin("a2kit")?;
    cmd.arg("get")
        .arg("-d").arg(dimg_path.clone())
        .arg("-t").arg("bin")
        .arg("-f").arg("GO.SUB")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("PIP B:PROG1.BAS=A:PROG1.BAS\r\nB:\r\nREN PROG2.BAS=PROG1.BAS\r\n"));

        Ok(())
}
