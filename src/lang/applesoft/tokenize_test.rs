
// Tokenization is tested against Virtual ][
// This assembly program generates the hex dump, which is then copied
// and pasted directly into the test.
//          ORG   300
// ZPTR     EQU   $06
// SPTR     EQU   $08
// PRGST    EQU   $67
// PRGEND   EQU   $AF
// PRBYTE   EQU   $FDDA
//          LDA   PRGST
//          STA   ZPTR
//          LDA   PRGST+1
//          STA   ZPTR+1
//          SEC
//          LDA   PRGEND
//          SBC   #$01
//          STA   SPTR
//          LDA   PRGEND+1
//          SBC   #$00
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

#[cfg(test)]
use std::fmt::Write;
#[cfg(test)]
use regex::Regex;
#[cfg(test)]
use super::tokenizer::Tokenizer;
#[cfg(test)]
use tree_sitter_applesoft;

#[cfg(test)]
fn test_tokenizer(test_code: &str,expected: &str) {
	// we need to test explicitly for syntax errors, sometimes tokenizer can succeed despite them
	super::super::verify_str(tree_sitter_applesoft::language(),test_code).expect("syntax error while tokenizing");
	let mut tokenizer = Tokenizer::new();
	// get actual into hex string
	let bytes = tokenizer.tokenize(test_code,2049).expect("tokenizer failed");
	let mut listfmt = String::new();
	write!(&mut listfmt,"{:02X?}",bytes).expect("formatting error");
	let re = Regex::new(r"[\[\] ,]").unwrap();
	let actual = re.replace_all(&listfmt,"".to_string());
	assert_eq!(actual,expected);
}

mod input_tests {
	#[test]
	fn quote_parity() {
		let test_code = "10 PRINT CHR$(4);\"PREFIX\": INPUT PR$\n";
		let expected = "19080A00BAE72834293B22505245464958223A84505224000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn input_null_prompt() {
		let test_code = "10 input \"\"; a$\n";
		let expected = "0C080A008422223B4124000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn get_multi() {
		let test_code = "10 GET A$,B$,C$\n";
		let expected = "0F080A00BE41242C42242C4324000000";
		super::test_tokenizer(test_code, expected);
	}
}

mod output_tests {
	#[test]
	fn single_line() {
		let test_code = "10 HOME \n";
		let expected = "07080A0097000000";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn multi_line() {
		let test_code = "10 HOME\n20 PRI NT \"HELLO\"  ";
		let expected = "07080A00970014081400BA2248454C4C4F22000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn lower_case_variable() {
		let test_code = "10 HOME\n20 PRINT hello";
		let expected = "07080A00970012081400BA48454C4C4F000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn print_with_nulls() {
		let test_code = "10 print a,b, ,c;d$;;;e$";
		let expected = "15080A00BA412C422C2C433B44243B3B3B4524000000";
		super::test_tokenizer(test_code, expected);
	}
}

mod data_statement_tests {
//describe('Data Statements', async function() {
	// ** in these tests spaces are important **
	#[test]
	fn simple() {
		let test_code = "10 DATA aliteral, \"a string\", 1\n";
		let expected = "1F080A008320616C69746572616C2C20226120737472696E67222C2031000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn trailing_statement() {
		let test_code = "10 DATA aliteral, \"a string\", 1  : PRINT A$\n";
		let expected = "25080A008320616C69746572616C2C20226120737472696E67222C203120203ABA4124000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn float_items() {
		let test_code = "10 data 1.5 e 4 , 100000: print a$\n"; // lower case e is kept, but A2ROM does the same
		let expected = "1C080A008320312E3520652034202C203130303030303ABA4124000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn negative_numbers() {
		let test_code = "10 data - 1.0,-1.1,- 5\n";
		let expected = "16080A0083202D20312E302C2D312E312C2D2035000000";
		super::test_tokenizer(test_code, expected);
	}
}

mod expression_tests {
	#[test]
	fn simple() {
		let test_code = "10 x = 1 + 1\n";
		let expected = "0B080A0058D031C831000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn nested() {
		let test_code = "10 x = 1e6*(1 + (x1 + x2)*5)\n";
		let expected = "19080A0058D0314536CA2831C8285831C8583229CA3529000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn with_functions() {
		let test_code = "10 x = 1e6*(fn cub(x0) + (atn(x1) + cos(x2))*5)\n";
		let expected = "26080A0058D0314536CA28C243554228583029C828E128583129C8DE2858322929CA3529000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn negative_numbers() {
		let test_code = "10 x = -1.0: y = - 2.35\n";
		let expected = "14080A0058D0C9312E303A59D0C9322E3335000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn terminal_sexpr() {
		let test_code = "10 if x then a$ = a$ + \"hello\n";
		let expected = "15080A00AD58C44124D04124C82268656C6C6F000000";
		super::test_tokenizer(test_code, expected);
	}
}

mod graphics_tests {
	#[test]
	fn low_res() {
		let test_code = "10 gr: color=4\n20 x=5:y=5\n30 plot x,y\n40 hlin x+1,x+10 at y\n50 vlin y+1,y+10 at x";
		let expected = "0A080A00883AA034001608140058D0353A59D035001F081E008D582C59002F0828008E58C8312C58C83130C559003F0832008F59C8312C59C83130C558000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn high_res() {
		let test_code = "10 hgr: hcolor=2\n20 x=5:y=5\n30 plot x,y\n40 hplot to x+5,y+5";
		let expected = "0A080A00913A9232001608140058D0353A59D035001F081E008D582C59002D08280093C158C8352C59C835000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn shapes_1() {
		let test_code = "10 shload\n20 rot=0:scale=1:draw 5";
		let expected = "07080A009A001408140098303A99313A9435000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn shapes_2() {
		let test_code = "10 xdraw 1 at 5,y(3)";
		let expected = "0F080A009531C5352C59283329000000";
		super::test_tokenizer(test_code, expected);
	}
}

mod control_tests {
	#[test]
	fn goto_gosub_end_return() {
		let test_code = "10 gosub 1000: goto 100\n100 end\n1000 return";
		let expected = "10080A00B0313030303AAB313030001608640080001C08E803B1000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn on_x_goto_gosub() {
		let test_code = "10 on x goto 10,20,30\n100 on x gosub 110,120,130";
		let expected = "11080A00B458AB31302C32302C33300024086400B458B03131302C3132302C313330000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn loop_() {
		let test_code = "10 for i = 1 to last: print i: next i";
		let expected = "15080A008149D031C14C4153543ABA493A8249000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn onerr_resume() {
		let test_code = "10 onerr goto 1000\n1000 resume";
		let expected = "0C080A00A5AB31303030001208E803A6000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn if_then() {
		let mut test_string = " 1 0  if x > y then 1000\n".to_string();
		test_string += "20 if x < y then 1010\n";
		test_string += "30 if x <> y then 1020\n";
		test_string += "40 if x = y then 1030\n";
		let expected = "0F080A00AD58CF59C431303030001D081400AD58D159C431303130002C081E00AD58D1CF59C431303230003A082800AD58D059C431303330000000";
		super::test_tokenizer(&test_string, expected);
	}
}

mod escapes {
	#[test]
	fn string_escapes() {
		let test_code = "10 print \"\\x0d1\\x0d2\\x0a\\x0a\"";
		let expected = "0F080A00BA220D310D320A0A22000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn terminal_string_escapes() {
		let test_code = "10 print \"\\x0d1\\x0d2\\x0a\\x0a:rem";
		let expected = "12080A00BA220D310D320A0A3A72656D000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn data_escapes() {
		let test_code = "10 data \":\",\\x5Cxff : rem \\\\\\\\";
		let expected = "18080A008320223A222C5C786666203AB2205C5C5C5C000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn data_literal_escapes() {
		let test_code = "10 data literal\\\\x0awith stuff\n";
		let expected = "1B080A0083206C69746572616C5C0A77697468207374756666000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn rem_escapes() {
		let test_code = "10 rem \\x0a\\x0aAAA\\x0a\\x0a";
		let expected = "0F080A00B2200A0A4141410A0A000000";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn dos_escapes() {
		let test_code = "0 PR# 0\n1 PRINT:PRINT \"\\x04BLOAD DATA1,A$4000\":END\n";
		let expected = "080800008A300027080100BA3ABA2204424C4F41442044415441312C412434303030223A80000000";
		super::test_tokenizer(test_code, expected);
	}
}