//! # Integer BASIC parsing library
//! This uses `tree_sitter_integerbasic` to provide language parsing.
//! Tokenization and detokenization is handled here.
//! Syntax checking is in a private module at present.

mod token_maps;
mod tokenize_test;
pub mod tokenizer;
