
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
mod op_tests {
	#[test]
	fn no_label() {
		let test_code = "   lda #$00  ; rem\n";
		let expected = "A0ECE4E1A0A3A4B0B0A0BB20F2E5ED8D";
		super::test_tokenizer(test_code,expected);
	}
	#[test]
	fn with_label() {
		let test_code = "lab   lda #$00  ; rem\n";
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
