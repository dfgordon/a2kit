#[cfg(test)]
use super::tokenizer::Tokenizer;
#[cfg(test)]
use hex;

#[cfg(test)]
fn test_detokenizer(hex_tokens: &str, expected: &str) {
    let tokens = hex::decode(hex_tokens).expect("hex error");
    let tokenizer = Tokenizer::new();
    let actual = tokenizer.detokenize(&tokens).expect("detokenization error");
    assert_eq!(actual,expected);
}

mod input_statements {
	#[test]
	fn quote_parity() {
		let expected = "10 PRINT \"PREFIX\": INPUT PR$\n";
		let tokens = "120A006128D0D2C5C6C9D8290352D0D24001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn input_null_prompt() {
		let expected = "10 INPUT \"\",A$\n";
		let tokens = "0A0A0053282926C14001";
		super::test_detokenizer(tokens, expected);
	}
}

mod output_tests {
	#[test]
	fn single_line() {
		let expected = "10 TEXT \n";
		let tokens = "050A004B01";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn multi_line() {
		let expected = "10 TEXT \n20 PRINT \"HELLO\"\n";
		let tokens = "050A004B010C14006128C8C5CCCCCF2901";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn print_with_nulls() {
		let expected = "10 PRINT A,B,,C;D$;;;E$\n";
		let tokens = "130A0062C149C24A49C345C440474745C54001";
		super::test_detokenizer(tokens, expected);
	}
}

mod expressions {
	#[test]
	fn simple() {
		let expected = "10 X=1+1\n";
		let tokens = "0D0A00D871B1010012B1010001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn negative_numbers() {
		let expected = "10 X=-1\n";
		let tokens = "0A0A00D87136B1010001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn double_negative() {
		let expected = "10 X=--1\n";
		let tokens = "0B0A00D8713636B1010001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn nested() {
		let expected = "10 X=6*(1+(X1+X2)*5)\n";
		let tokens = "1B0A00D871B606001438B101001238D8B112D8B27214B505007201";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn logic_values() {
		let expected = "10 COLOR=I/2*(I<32)\n";
		let tokens = "120A0066C915B202001438C91CB320007201";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn with_functions() {
		let expected = "10 X=6*( ABS (X0)+( SGN (X1)+ ASC(A$))*5)\n";
		let tokens = "220A00D871B606001438313FD8B0721238303FD8B172123CC140727214B505007201";
		super::test_detokenizer(tokens, expected);
	}
}

mod graphics_tests {
	#[test]
	fn low_res_statements() {
		let expected = "10 GR : COLOR=4\n20 X=5:Y=5\n30 PLOT X,Y\n40 HLIN X+1,X+10 AT Y\n50 VLIN Y+1,Y+10 AT X\n";
		let tokens = "0A0A004C0366B40400010F1400D871B5050003D971B5050001081E0067D868D90112280069D812B101006AD812B10A006BD9011232006CD912B101006DD912B10A006ED801";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn low_res_functions() {
		let expected = "10 C= SCRN(X,Y)\n";
		let tokens = "0B0A00C3713DD83ED97201";
		super::test_detokenizer(tokens, expected);
	}
}

mod control_tests {
	#[test]
	fn binary_ascii_collisions() {
		let expected = "32 X=32\n";
		let tokens = "092000D871B3200001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn goto_gosub_end_return() {
		let expected = "10 GOSUB 1000: GOTO 100\n100 END \n1000 RETURN \n";
		let tokens = "0D0A005CB1E803035FB1640001056400510105E8035B01";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn loop_() {
		let expected = "10 FOR I=1 TO LAST: PRINT I: NEXT I\n";
		let tokens = "150A0055C956B1010057CCC1D3D40362C90359C901";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn if_then() {
		let mut test_string = "10 IF X>Y THEN 1000\n".to_string();
		test_string += "20 IF X<Y THEN 1010\n";
		test_string += "30 IF X<>Y THEN 1020\n";
		test_string += "40 IF X=Y THEN 1030\n";
		let tokens = "0C0A0060D819D924B1E803010C140060D81CD924B1F203010C1E0060D81BD924B1FC03010C280060D816D924B1060401";
		super::test_detokenizer(tokens, &test_string);
	}
}

mod escapes {
	#[test]
	fn string_escapes() {
		let expected = "10 PRINT \"\\x8a1\\x8a2\"\n";
		let tokens = "0B0A0061288AB18AB22901";
		super::test_detokenizer(tokens, expected);
	}
 	#[test]
	fn rem_escapes() {
		let expected = "10 REM \\x8a\\x8aAAA\\x8a\\x8a\n";
		let tokens = "0D0A005DA08A8AC1C1C18A8A01";
		super::test_detokenizer(tokens, expected);
	}
	// no DOS escapes by default
	#[test]
	fn dos_non_escapes() {
		let expected = "0 PR# 0\n1 PRINT : PRINT \"\x04BLOAD DATA1,A$4000\": END \n";
		let tokens = "0800007EB00000011E01006303612884C2CCCFC1C4A0C4C1D4C1B1ACC1A4B4B0B0B029035101";
		super::test_detokenizer(tokens, expected);
	}
}

mod general {
	#[test]
	fn text_and_binary_nums() {
		let expected = "10 TEXT : CALL -936: VTAB 3\n24 PRINT : TAB 30: PRINT \"16-FEB-79\"\n40 REM123:REM456\n";
		let tokens = "100A004B034D36B9A803036FB3030001171800630350B31E00036128B1B6ADC6C5C2ADB7B929010F28005DB1B2B3BAD2C5CDB4B5B601";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn animals_frags() {
		let expected = "160 PRINT : PRINT NEW$;: INPUT \"?\",A$:PREV=CUR: IF NOT LEN(A$) THEN 160:A$=A$(1,1): IF A$#\"Y\" AND A$#\"N\" THEN 160\n\
170 IF A$=\"Y\" THEN CUR=RTPTR: IF A$=\"N\" THEN CUR=WRNGPTR: GOTO 110\n";
		let tokens = "4AA000630361CEC5D74047035328BF2926C14003\
D0D2C5D671C3D5D20360373BC1407224B1A00003\
C14070C1402AB1010023B10100720360C1403A28\
D9291DC1403A28CE2924B1A000012EAA0060C140\
3928D92925C3D5D271D2D4D0D4D20360C1403928\
CE2925C3D5D271D7D2CEC7D0D4D2035FB16E0001";
		super::test_detokenizer(tokens, expected);
	}
	#[test]
	fn color_demo_frags() {
		let expected = "1500 POKE 0,P: POKE 1,D: CALL 2: RETURN \n\
2000 TAB ((40- LEN(A$))/2+1): PRINT A$: PRINT : RETURN \n\
3000 GR : FOR I=0 TO 31: COLOR=I/2: VLIN 0,39 AT I+3: NEXT I\n\
65535 REM*COPYRIGHT 1978 APPLE COMPUTER,INC.*\n";
		let tokens = "18DC0564B0000065D00364B1010065C4034DB202\
00035B0121D007503838B42800133BC140727215\
B2020012B10100720361C1400363035B0129B80B\
4C0355C956B0000057B31F000366C915B2020003\
6CB000006DB327006EC912B303000359C90129FF\
FF5DAAC3CFD0D9D2C9C7C8D4A0B1B9B7B8A0C1D0\
D0CCC5A0C3CFCDD0D5D4C5D2ACC9CEC3AEAA01";
		super::test_detokenizer(tokens, expected);
	}
}