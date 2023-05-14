
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

mod op_tests {
	#[test]
	fn no_label() {
		let test_code = "   lda #$00  ; rem\n";
		let expected = "A0ECE4E1A0A3A4B0B0A0BB20F2E5ED8D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn with_label() {
		let test_code = "lab   lda #$00  ; rem\r\n";
		let expected = "ECE1E2A0ECE4E1A0A3A4B0B0A0BB20F2E5ED8D";
		super::test_tokenizer(test_code,expected);
	}
}

mod column4 {
	#[test]
	fn only_op() {
		let test_code = "   inx  ; rem\n";
		let expected = "A0E9EEF8A0A0BB20F2E5ED8D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn only_lab() {
		let test_code = "lab  ; rem\n";
		let expected = "ECE1E2A0A0A0BB20F2E5ED8D";
		super::test_tokenizer(test_code,expected);
	}
}

mod strings {
	#[test]
	fn asc_string() {
		let test_code = "  \t ASC  \"123\"\n";
		let expected = "A0C1D3C3A0A2B1B2B3A28D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn lda_char() {
		let test_code = "\tLDA\t'a'\n";
		let expected = "A0CCC4C1A0A7E1A78D";
		super::test_tokenizer(test_code,expected);
	}
}

mod macros {
	#[test]
	fn def() {
		let test_code = "MLI  MAC\n";
		let expected = "CDCCC9A0CDC1C38D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn implicit_call() {
		let test_code = "\tMLI\tREAD;PARMS\n";
		let expected = "A0CDCCC9A0D2C5C1C4BBD0C1D2CDD38D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn explicit_call() {
		let test_code = "\tPMC  MLI,READ;PARMS\n";
		let expected = "A0D0CDC3A0CDCCC9ACD2C5C1C4BBD0C1D2CDD38D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn explicit_call_spc() {
		let test_code = "\tPMC  MLI READ;PARMS\n";
		let expected = "A0D0CDC3A0CDCCC920D2C5C1C4BBD0C1D2CDD38D";
		super::test_tokenizer(test_code,expected);
	}
}

mod trailing {
	#[test]
	fn op_match() {
		let test_code = "    inc16 word\r\n";
		let expected = "A0E9EEE3B1B6A0F7EFF2E48D";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn op_match_w_lab() {
		let test_code = ":ADVANCE inc16 word\r\n";
		let expected = "BAC1C4D6C1CEC3C5A0E9EEE3B1B6A0F7EFF2E48D";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn psop_match() {
		let test_code = "    doit a+b\r\n";
		let expected = "A0E4EFE9F4A0E1ABE28D";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn psop_match_w_lab() {
		let test_code = ":ADVANCE lstNext\r\n";
		let expected = "BAC1C4D6C1CEC3C5A0ECF3F4CEE5F8F48D";
		super::test_tokenizer(test_code, expected);
	}
}

mod expressions {
	#[test]
	fn lr_expr() {
		let test_code = " LDA #VAL*%1001+$FF";
		let expected = "A0CCC4C1A0A3D6C1CCAAA5B1B0B0B1ABA4C6C68D";
		super::test_tokenizer(test_code, expected);
	}
	#[test]
	fn braced_expr() {
		let test_code = " LUP VAL1+{{VAL2+1}*15/$E}\r\n";
		let expected = "A0CCD5D0A0D6C1CCB1ABFBFBD6C1CCB2ABB1FDAAB1B5AFA4C5FD8D";
		super::test_tokenizer(test_code, expected);
	}
}