//! These tests will pass over themselves if the WOZ images are not found.
//! This is done so we don't need to put the images on a public repository,
//! which could cause some licensing problems.  Use
//! `cargo test -- --nocapture` locally to see the warnings for missing images.

use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::{Command,Stdio}; // Run programs
use std::io::Write;
use std::path::{Path,PathBuf};
type STDRESULT = Result<(),Box<dyn std::error::Error>>;

// IMPORTANT: this function should always be used to gather paths otherwise we can have unexpected
// CI failures on the public repository.
fn get_paths(base_name: &str,ext: &str) -> Option<[PathBuf;2]> {
    let img_path = Path::new("tests").join("special").join([base_name,ext].concat());
    let fmt_path = Path::new("tests").join("special").join([base_name,".fmt.json"].concat());
    if std::fs::exists(&img_path).is_ok_and(|x| x) {
        eprintln!("found test image {}",base_name);
        Some([img_path,fmt_path])
    } else {
        eprintln!("test image {} was missing, for public test this is normal",base_name);
        None
    }
}

#[test]
fn catalog_wolfenstein() -> STDRESULT {
    let [img_path,fmt_path] = match get_paths("wolfenstein",".woz") {
        Some([x,y]) => [x,y],
        None => return Ok(())
    };
    let mut cmd = Command::cargo_bin("a2kit")?;
    let expected = 
r#"
DISK VOLUME 1

*A 006 ^HELLO
*I 002 APPLESOFT
*B 034 PIX
*B 034 PICEX
*B 065 SEKTOR
*B 047 ^VOCAB
*B 006 ^CHARSET
 B 024 CASTLE
 B 064 BACKUP
*T 007 ^TEXT
*B 020 @INIT
*B 024 @WOLF
*B 024 ^THINGS
"#;
    cmd.arg("catalog")
        .arg("-d")
        .arg(img_path)
        .arg("--pro")
        .arg(fmt_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn get_wolf_hello() -> STDRESULT {

    let [img_path,fmt_path] = match get_paths("wolfenstein",".woz") {
        Some([x,y]) => [x,y],
        None => return Ok(())
    };
    let expected = "5  PRINT \"\u{0004}NOMON C,I,O\"\n10  PRINT \"\u{0004}BRUN @INIT\": END \n";

    let mut cmd = Command::cargo_bin("a2kit")?;
    let output = cmd.arg("get")
        .arg("-d")
        .arg(img_path)
        .arg("--pro")
        .arg(fmt_path)
        .arg("-f")
        .arg("^hello")
        .arg("-t")
        .arg("atok")
        .assert()
        .success()
        .get_output().clone();

    let mut cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("detokenize")
        .arg("-t").arg("atok")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || {
            stdin.write_all(&output.stdout).expect("Failed to write to stdin");
        });
        
        let output = child.wait_with_output().expect("Failed to read stdout");
        assert_eq!(String::from_utf8_lossy(&output.stdout),expected);
        
    Ok(())
}

#[test]
fn catalog_ultima4() -> STDRESULT {
    let [img_path,fmt_path] = match get_paths("ultima4",".woz") {
        Some([x,y]) => [x,y],
        None => return Ok(())
    };
    let mut cmd = Command::cargo_bin("a2kit")?;
    let expected = 
r#"
DISK VOLUME 254

*B 003 I\x81NIT
*B 026 S\x81UBS
*B 018 S\x81HP0
*B 018 S\x81HP1
*B 006 T\x81BLS
*B 006 H\x81TXT
*B 002 S\x81EL
*B 021 B\x81OOT
*B 002 D\x81ISK
*B 034 B\x81GND
*B 034 A\x81NIM
*B 053 N\x81EWGAME
*B 012 T\x81REE.SPK
*B 004 P\x81RTL.SPK
*B 014 L\x81OOK.SPK
*B 014 F\x81AIR.SPK
*B 016 W\x81AGN.SPK
*B 012 G\x81YPS.SPK
*B 003 T\x81ABL.SPK
*B 037 C\x81RDS
*B 003 N\x81LST
*B 003 N\x81RST
*B 002 N\x81PRT
*B 004 M\x81BSI
*B 008 M\x81BSM
*B 012 M\x81UST
*B 011 M\x81USO
*B 011 M\x81USD
*B 000 M\x81USC
*B 007 M\x81USB
*B 000 U\x81LT4
"#;
    cmd.arg("catalog")
        .arg("-d")
        .arg(img_path)
        .arg("--pro")
        .arg(fmt_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}

#[test]
fn get_ultima4_sel_dasm() -> STDRESULT {
    let [img_path,fmt_path] = match get_paths("ultima4",".woz") {
        Some([x,y]) => [x,y],
        None => return Ok(())
    };

    // we will just compare the first two lines since disassembly output is not a stable concept
    let expected = [
        "_0320    JMP   _0338",
        "         JMP   _039C"
    ];

    let mut cmd = Command::cargo_bin("a2kit")?;
    let output = cmd.arg("get")
        .arg("-d")
        .arg(img_path)
        .arg("--pro")
        .arg(fmt_path)
        .arg("-f")
        .arg("s\\x81el")
        .arg("-t")
        .arg("bin")
        .assert()
        .success()
        .get_output().clone();

    let mut cmd = Command::cargo_bin("a2kit")?;
    let mut child = cmd.arg("dasm")
        .arg("-p").arg("6502")
        .arg("-o").arg("800")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn child process");
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || {
            stdin.write_all(&output.stdout).expect("Failed to write to stdin");
        });
        
        let output = child.wait_with_output().expect("Failed to read stdout");
        let all_lines = String::from_utf8_lossy(&output.stdout);
        let mut lines = all_lines.lines();
        for i in 0..2 {
            assert_eq!(lines.next().unwrap(),expected[i]);
        }
        
    Ok(())
}

#[test]
fn catalog_ultima5() -> STDRESULT {
    let [img_path,fmt_path] = match get_paths("ultima5",".woz") {
        Some([x,y]) => [x,y],
        None => return Ok(())
    };
    let mut cmd = Command::cargo_bin("a2kit")?;
    let expected = 
r#"
/ULTIMA5

 NAME            TYPE BLOCKS MODIFIED         CREATED          ENDFILE SUBTYPE

 DINKEYDOS       SYS      10 20-Feb-88 09:04  18-Jan-88 14:14     4608    8192
 STARTUP         BIN      11 16-Feb-88 16:03  18-Jan-88 14:14     4804   32768
 TEMP.SUBS       BIN       7 16-Feb-88 16:04  18-Jan-88 14:14     2685    5504
 MAIN.SUBS       BIN      17 16-Feb-88 16:05  18-Jan-88 14:14     8156   57344
 HTXT            BIN       5 16-Feb-88 11:14  18-Jan-88 14:14     2048   20480
 FLIPPER         BIN       4 16-Feb-88 16:07  18-Jan-88 14:14     1328    2048
 FLIPPER.DATA    BIN      12 13-Aug-87 18:21  18-Jan-88 14:14     5595   24576
 PRINT.RLE       BIN       5 14-Aug-87 12:01  18-Jan-88 14:14     1775   24576
 U5.PTHTBL       BIN       7 24-Mar-87 19:43  18-Jan-88 14:14     2888   24576
 U5.LOGO.RLE     BIN       5 14-Aug-87 12:55  18-Jan-88 14:14     1855   24576
 FLAMES.RLE      BIN      12 13-Jan-88 17:52  18-Jan-88 14:14     5239   24576
 INTRO.VIEW      BIN       9 16-Feb-88 16:08  18-Jan-88 14:14     3785   38912
 VIEW.TILES      BIN       8 30-Sep-87 15:30  18-Jan-88 14:14     3584   32768
 ABOUT           BIN       4 04-Jan-88 17:35  18-Jan-88 14:14     1507   41984
 ABOUT.U5.RLE    BIN       9 04-Jan-88 17:23  18-Jan-88 14:14     3586   24576
 TRANSFER        BIN       9 16-Feb-88 16:29  18-Jan-88 14:14     3871   32768
 CREATE          BIN      19 18-Jan-88 18:43  18-Jan-88 14:14     9169   25856
 CREATE1.TXT     BIN       3 28-Jan-88 12:50  18-Jan-88 14:14     1008    8192
 C1.ANMTBL       BIN      15 28-Dec-87 23:10  18-Jan-88 14:14     6682   16384
 FONT3.SHPTBL    BIN       4 27-Feb-87 15:50  18-Jan-88 14:14     1135   16384
 MUSIC           BIN      12 08-Feb-88 14:39  08-Feb-88 15:08     5620   32768
 MUDI            BIN       3 18-Feb-88 18:29  17-Feb-88 17:19      860   24576
 MUDR            BIN      10 18-Feb-88 18:39  17-Feb-88 17:19     4312   28160
 MUIN            BIN       1 17-Feb-88 17:16  17-Feb-88 17:19      256   32512
 MUFF            BIN      26 09-Feb-88 16:20  09-Feb-88 16:17    12290       0
 SHAPES          BIN      33 09-Dec-87 21:55  18-Jan-88 14:14    16384   16384
 ENTER.PLAY      BIN       4 16-Feb-88 16:03  18-Jan-88 14:14     1372   45056
 UPDATE.HIMEM    BIN       5 16-Feb-88 16:07  18-Jan-88 14:14     2047   53248

BLOCKS FREE: 3  BLOCKS USED: 277  TOTAL BLOCKS: 280
"#;
    cmd.arg("catalog")
        .arg("-d")
        .arg(img_path)
        .arg("--pro")
        .arg(fmt_path)
        .assert()
        .success()
        .stdout(predicate::str::contains(expected));
    Ok(())
}
