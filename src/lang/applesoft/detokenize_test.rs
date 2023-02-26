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

mod output_statements {
	//vscode.window.showInformationMessage("Start output statements");
    #[test]
	fn single_line() {
		let expected = "10  HOME \n";
		let tokens = "07080A0097000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn multi_line() {
		let expected = "10  HOME \n20  PRINT \"HELLO\"\n";
		let tokens = "07080A00970014081400BA2248454C4C4F22000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn lower_case_variable() {
		let expected = "10  HOME \n20  PRINT HELLO\n";
		let tokens = "07080A00970012081400BA48454C4C4F000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn print_with_nulls() {
		let expected = "10  PRINT A,B,,C;D$;;;E$\n";
		let tokens = "15080A00BA412C422C2C433B44243B3B3B4524000000";
		super::test_detokenizer(tokens, expected);
	}
}

mod data_statements {
	// ** in these tests spaces are important **
    #[test]
	fn simple() {
		let expected = "10  DATA  aliteral, \"a string\", 1\n";
		let tokens = "1F080A008320616C69746572616C2C20226120737472696E67222C2031000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn trailing_statement() {
		let expected = "10  DATA  aliteral, \"a string\", 1  : PRINT A$\n";
		let tokens = "25080A008320616C69746572616C2C20226120737472696E67222C203120203ABA4124000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn float_items() {
		let expected = "10  DATA  1.5 e 4 , 100000: PRINT A$\n"; // lower case e is kept, but A2ROM does the same
		let tokens = "1C080A008320312E3520652034202C203130303030303ABA4124000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn negative_numbers () {
		let expected = "10  DATA  - 1.0,-1.1,- 5\n";
		let tokens = "16080A0083202D20312E302C2D312E312C2D2035000000";
		super::test_detokenizer(tokens, expected);
	}
}

mod expressions {
    #[test]
	fn simple() {
		let expected = "10 X = 1 + 1\n";
		let tokens = "0B080A0058D031C831000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn nested() {
		let expected = "10 X = 1E6 * (1 + (X1 + X2) * 5)\n";
		let tokens = "19080A0058D0314536CA2831C8285831C8583229CA3529000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn with_functions() {
		let expected = "10 X = 1E6 * ( FN CUB(X0) + ( ATN (X1) +  COS (X2)) * 5)\n";
		let tokens = "26080A0058D0314536CA28C243554228583029C828E128583129C8DE2858322929CA3529000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn negative_numbers() {
		let expected = "10 X =  - 1.0:Y =  - 2.35\n";
		let tokens = "14080A0058D0C9312E303A59D0C9322E3335000000";
		super::test_detokenizer(tokens, expected);
	}
}

mod graphics {
    #[test]
	fn low_res() {
		let expected = "10  GR : COLOR= 4\n20 X = 5:Y = 5\n30  PLOT X,Y\n40  HLIN X + 1,X + 10 AT Y\n50  VLIN Y + 1,Y + 10 AT X\n";
		let tokens = "0A080A00883AA034001608140058D0353A59D035001F081E008D582C59002F0828008E58C8312C58C83130C559003F0832008F59C8312C59C83130C558000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn high_res() {
		let expected = "10  HGR : HCOLOR= 2\n20 X = 5:Y = 5\n30  PLOT X,Y\n40  HPLOT  TO X + 5,Y + 5\n";
		let tokens = "0A080A00913A9232001608140058D0353A59D035001F081E008D582C59002D08280093C158C8352C59C835000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn shapes_1() {
		let expected = "10  SHLOAD \n20  ROT= 0: SCALE= 1: DRAW 5\n";
		let tokens = "07080A009A001408140098303A99313A9435000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn shapes_2() {
		let expected = "10  XDRAW 1 AT 5,Y(3)\n";
		let tokens = "0F080A009531C5352C59283329000000";
		super::test_detokenizer(tokens, expected);
	}
}

mod control {
    #[test]
	fn goto_gosub_end() {
		let expected = "10  GOSUB 1000: GOTO 100\n100  END \n1000  RETURN \n";
		let tokens = "10080A00B0313030303AAB313030001608640080001C08E803B1000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn on_x_goto_gosub() {
		let expected = "10  ON X GOTO 10,20,30\n100  ON X GOSUB 110,120,130\n";
		let tokens = "11080A00B458AB31302C32302C33300024086400B458B03131302C3132302C313330000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn for_loop() {
		let expected = "10  FOR I = 1 TO LAST: PRINT I: NEXT I\n";
		let tokens = "15080A008149D031C14C4153543ABA493A8249000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn onerr_resume() {
		let expected = "10  ONERR  GOTO 1000\n1000  RESUME \n";
		let tokens = "0C080A00A5AB31303030001208E803A6000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn if_then() {
		let mut expected = String::from("10  IF X > Y THEN 1000\n");
		expected += "20  IF X < Y THEN 1010\n";
		expected += "30  IF X <  > Y THEN 1020\n";
		expected += "40  IF X = Y THEN 1030\n";
		let tokens = "0F080A00AD58CF59C431303030001D081400AD58D159C431303130002C081E00AD58D1CF59C431303230003A082800AD58D059C431303330000000";
		super::test_detokenizer(tokens, &expected);
	}
}

mod escapes {
    #[test]
	fn string_escapes () {
		let expected = "10  PRINT \"\\x0d1\\x0d2\\x0a\\x0a\"\n";
		let tokens = "0F080A00BA220D310D320A0A22000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn terminal_string_escapes () {
		let expected = "10  PRINT \"\\x0d1\\x0d2\\x0a\\x0a\n";
		let tokens = "0E080A00BA220D310D320A0A000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn data_escapes () {
		let expected = "10  DATA  1\\x092\\x09 \\\\ \n";
		let tokens = "0F080A00832031093209205C20000000";
		super::test_detokenizer(tokens, expected);
	}
    #[test]
	fn rem_escapes () {
		let expected = "10  REM \\x0a\\x0aAAA\\x0a\\x0a\n";
		let tokens = "0E080A00B20A0A4141410A0A000000";
		super::test_detokenizer(tokens, expected);
	}
}