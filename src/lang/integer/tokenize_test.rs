// Tokenization is tested against Virtual ][
// This assembly program generates the hex dump, which is then copied
// and pasted directly into the test.
//          ORG   300
// ZPTR     EQU   $06
// SPTR     EQU   $08
// PRGST    EQU   $CA
// PRGEND   EQU   $4C
// PRBYTE   EQU   $FDDA
//          LDA   PRGST
//          STA   ZPTR
//          LDA   PRGST+1
//          STA   ZPTR+1
//          LDA   PRGEND
//          STA   SPTR
//          LDA   PRGEND+1
//          STA   SPTR+1
// :LOOP    LDY   #$00
//          LDA   (ZPTR),Y
//          JSR   PRBYTE
//          CLC
//          LDA   #$01
//          ADC   ZPTR
//          STA   ZPTR
//          LDA   #$00
//          ADC   ZPTR+1
//          STA   ZPTR+1
//          LDA   SPTR
//          CMP   ZPTR
//          BNE   :LOOP
//          LDA   SPTR+1
//          CMP   ZPTR+1
//          BNE   :LOOP
//          RTS
// Following hex can be pasted into the Monitor:
// 300: A5 CA 85 06 A5 CB 85 07
// 308: A5 4C 85 08 A5 4D 85 09
// 310: A0 00 B1 06 20 DA FD 18
// 318: A9 01 65 06 85 06 A9 00
// 320: 65 07 85 07 A5 08 C5 06
// 328: D0 E6 A5 09 C5 07 D0 E0
// 330: 60


#[cfg(test)]
use std::fmt::Write;
#[cfg(test)]
use regex::Regex;
#[cfg(test)]
use super::tokenizer::Tokenizer;


#[cfg(test)]
fn test_tokenizer(test_code: &str,expected: &str) {
	let mut tokenizer = Tokenizer::new();
	// get actual into hex string
	let bytes = tokenizer.tokenize(test_code.to_string()).expect("tokenizer failed");
	let mut listfmt = String::new();
	write!(&mut listfmt,"{:02X?}",bytes).expect("formatting error");
	let re = Regex::new(r"[\[\] ,]").unwrap();
	let actual = re.replace_all(&listfmt,"".to_string());
	assert_eq!(actual,expected);
}

// OUTPUT
mod output_tests {
	#[test]
	fn single_line() {
		let test_code = "10 TEXT\n";
		let expected = "050A004B01";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn multi_line() {
		let test_code = "10 TEXT\r\n20 PRINT \"HELLO\"\n";
		let expected = "050A004B010C14006128C8C5CCCCCF2901";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn print_with_nulls() {
		let test_code = "10 print a,B, ,C;d$;;;E$\n";
		let expected = "130A0062C149C24A49C345C440474745C54001";
		super::test_tokenizer(test_code, expected);
	}
}

mod expression_tests {
	#[test]
	fn simple() {
		let test_code = "10 X = 1 + 1 \n";
		let expected = "0D0A00D871B1010012B1010001";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn negative_numbers() {
		let test_code = "10 X = -1\n";
		let expected = "0A0A00D87136B1010001";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn double_negative() {
		let test_code = "10 X = - - 1\n";
		let expected = "0B0A00D8713636B1010001";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn nested() {
		let test_code = "10 X = 6*(1 + (X1 + X2)*5)\n";
		let expected = "1B0A00D871B606001438B101001238D8B112D8B27214B505007201";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn logic_values() {
		let test_code = "10 COLOR = I/2*(I<32)\n";
		let expected = "120A0066C915B202001438C91CB320007201";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn with_functions() {
		let test_code = "10 X = 6*(abs(X0) + (sgn(X1) + asc(A$))*5)\n";
		let expected = "220A00D871B606001438313FD8B0721238303FD8B172123CC140727214B505007201";
		super::test_tokenizer(test_code, expected);
	}
}

mod graphics_tests {
	#[test]
	fn low_res_statements() {
		let test_code = "10 gr: color=4\n20 X=5:Y=5\n30 plot X,Y\n40 hlin X+1,X+10 at Y\n50 vlin Y+1,Y+10 at X";
		let expected = "0A0A004C0366B40400010F1400D871B5050003D971B5050001081E0067D868D90112280069D812B101006AD812B10A006BD9011232006CD912B101006DD912B10A006ED801";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn low_res_functions() {
		let test_code = "10 C = SCRN(X,Y)";
		let expected = "0B0A00C3713DD83ED97201";
		super::test_tokenizer(test_code, expected);
	}
}

mod control_tests {
	#[test]
	fn binary_ascii_collisions() {
		let test_code = "32 x = 32";
		let expected = "092000D871B3200001";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn goto_gosub_end_return() {
		let test_code = "10 gosub 1000: goto 100\n100 end\n1000 return";
		let expected = "0D0A005CB1E803035FB1640001056400510105E8035B01";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn loop_() {
		let test_code = "10 for i = 1 to LAST: print i: next I";
		let expected = "150A0055C956B1010057CCC1D3D40362C90359C901";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn if_then() {
		let mut test_string = " 1 0  if x > y then 1000\n".to_string();
		test_string += "20 if x < y then 1010\n";
		test_string += "30 if x <> y then 1020\n";
		test_string += "40 if x = y then 1030\n";
		let expected = "0C0A0060D819D924B1E803010C140060D81CD924B1F203010C1E0060D81BD924B1FC03010C280060D816D924B1060401";
		super::test_tokenizer(&test_string, expected);
	}
}
