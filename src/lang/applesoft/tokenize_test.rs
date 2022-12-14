
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
fn test_tokenizer(test_code: &str,expected: &str) {
	let mut tokenizer = Tokenizer::new();
	// get actual into hex string
	let bytes = tokenizer.tokenize(test_code,2049);
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
