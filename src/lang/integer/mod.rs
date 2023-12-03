//! # Integer BASIC parsing module
//! This uses `tree_sitter_integerbasic` to provide language parsing.
//! Tokenization and detokenization is handled here.
//! Syntax checking is in `walker`.

mod token_maps;
mod tokenize_test;
mod detokenize_test;
pub mod tokenizer;

use std::fmt::Write;

/// Escape the bytes in some negative ASCII stringlike context.  The escape value is not inverted.
/// `bytes` are the bytes to escape, literal hex escapes will hex-escape the backslash (`\x5c`)
/// `offset` is the index to start of context, one past the triggering byte
/// `terminator` are characters that close the context
/// Returns escaped string and index to terminator, terminator not included in string
pub fn bytes_to_escaped_string(bytes: &[u8], offset: usize, terminator: &[u8]) -> (String,usize)
{
    const BACKSLASH: u8 = 128+92;
	let escaping_ascii = [138, 141];
	let mut ans = String::new();
	let mut idx = offset;
	while idx < bytes.len() {
		if terminator.contains(&bytes[idx]) {
			break;
        }
		if bytes[idx] == BACKSLASH && idx + 3 < bytes.len() {
            let is_hex = |x_neg: u8| -> bool {
                let x = x_neg - 128;
                x>=48 && x<=57 || x>=65 && x<=70 || x>=97 && x<=102
            };
            if bytes[idx+1]==128+120 && is_hex(bytes[idx+2]) && is_hex(bytes[idx+3]) {
                ans += "\\x5c";
            } else {
                ans += "\\";
            }
        } else if escaping_ascii.contains(&bytes[idx]) || bytes[idx] > 254 || bytes[idx] < 128 {
            let mut temp = String::new();
            write!(&mut temp,"\\x{:02x}",bytes[idx]).expect("unreachable");
            ans += &temp;
        } else {
			ans += std::str::from_utf8(&[bytes[idx]-128]).expect("unreachable");
        }
		idx += 1;
	}
	return (ans,idx);
}
