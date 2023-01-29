//! # Applesoft parsing module
//! This uses `tree_sitter_applesoft` to provide language parsing.
//! Tokenization and detokenization is handled here.
//! Syntax checking is in `walker`.

mod token_maps;
mod minify_guards;
mod tokenize_test;
mod minify_test;
mod renumber_test;
pub mod tokenizer;
pub mod minifier;
pub mod renumber;

pub fn deduce_address(tokens: &Vec<u8>) -> u16 {
    let line2_addr = u16::from_le_bytes([tokens[0],tokens[1]]);
    let mut line2_rel = 4;
    while tokens[line2_rel]>0 {
        line2_rel += 1;
    }
    return line2_addr - line2_rel as u16 - 1;
}