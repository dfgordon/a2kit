//! # Merlin assmebly language parsing module
//! This uses `tree_sitter_merlin6502` to provide language parsing.
//! Source code "tokenization" (encoding/decoding) is handled here.
//! Syntax checking is in `walker`.

pub mod tokenizer;
mod tokenize_test;
