//! # Applesoft analysis and transformations
//! 
//! This module is used by both the CLI and the language server.
//! The Applesoft parser is provided by `tree_sitter_applesoft`.
//! The server compiles to a separate executable, its entry point is in `src/bin/server-applesoft/main.rs`.

mod token_maps;
mod minify_guards;
#[cfg(test)]
mod tokenize_test;
#[cfg(test)]
mod detokenize_test;
#[cfg(test)]
mod minify_test;
#[cfg(test)]
mod renumber_test;
#[cfg(test)]
mod diagnostics_test;
pub mod diagnostics;
pub mod checkpoint;
pub mod tokenizer;
pub mod minifier;
pub mod renumber;
pub mod hovers;
pub mod settings;
pub mod completions;
pub mod semantic_tokens;

use std::fmt::Write;
use std::collections::{HashMap,HashSet};
use lsp_types as lsp;

/// Information about a specific line label.
/// The label itself is a key that maps to this information.
#[derive(Clone)]
pub struct Line {
    /// docstring taken from previous line or end of this line
    rem: Option<String>,
    /// the LSP range of the primary line number
    primary: lsp::Range,
    /// GOSUB statements referencing this line
    gosubs: Vec<lsp::Range>,
    /// GOTO statements referencing this line
    gotos: Vec<lsp::Range>
}

/// Information about a specific variable or function.
/// The name itself is a key that maps to this information.
/// The key is always put in uppercase, but all case variations
/// that occur are saved.
#[derive(Clone)]
pub struct Variable {
    decs: Vec<lsp::Range>,
    defs: Vec<lsp::Range>,
    refs: Vec<lsp::Range>,
    case: HashSet<String>
}

impl Variable {
    pub fn new() -> Self {
        Self {
            decs: Vec::new(),
            defs: Vec::new(),
            refs: Vec::new(),
            case: HashSet::new()
        }
    }
    /// if rng is already in defs or decs, do nothing, otherwise push to refs
    fn push_ref_selectively(&mut self,rng: lsp::Range) {
        if self.decs.contains(&rng) {
            return;
        }
        if self.defs.contains(&rng) {
            return;
        }
        self.refs.push(rng);
    }
}

/// Main structure containing the symbol information
#[derive(Clone)]
pub struct Symbols {
    pub lines: HashMap<i64,Line>,
    pub functions: HashMap<String,Variable>,
    pub scalars: HashMap<String,Variable>,
    pub arrays: HashMap<String,Variable> 
}

impl Symbols {
    pub fn new() -> Self {
        Self {
            lines: HashMap::new(),
            functions: HashMap::new(),
            scalars: HashMap::new(),
            arrays: HashMap::new()
        }
    }
    pub fn docstring(&self,linenum: i64) -> Option<String> {
        match self.lines.get(&linenum) {
            Some(l) => l.rem.clone(),
            None => None
        }
    }
}

pub fn deduce_address(tokens: &[u8]) -> u16 {
    let line2_addr = u16::from_le_bytes([tokens[0],tokens[1]]);
    let mut line2_rel = 4;
    while tokens[line2_rel]>0 {
        line2_rel += 1;
    }
    return line2_addr - line2_rel as u16 - 1;
}

/// Extract variable name from syntax node
/// @param node syntax node to be analyzed, can be var_* or name_*, except for name_amp
/// @param recall is this an implicit array like the argument of RECALL
/// @returns [normalized name,specific name], includes `$`, `%`, or `()`
pub fn var_to_key(node: tree_sitter::Node,recall: bool,line: &str) -> [String;2]
{
    let name_node = match node.named_child(0) {
        Some(child) => child,
        None => node
    };
	let mut n = super::node_text(&name_node,line).replace(" ", "");
    match (name_node.next_named_sibling(),recall) {
        (_,true) => n += "()",
        (Some(sib),_) if sib.kind()=="subscript" => n += "()",
        _ => {}
    };
	return [n.to_uppercase(),n];
}


/// Extract range of the variable name within the node
/// @param node node can be either var_* or name_*
/// @returns range of name part of variable, includes `$` or `%` but not subscript
pub fn name_range(node: tree_sitter::Node) -> tree_sitter::Range
{
    if let Some(name_node) = node.named_child(0) {
        return name_node.range();
    }
    return node.range();
}

/// Starting in some stringlike context defined by `ctx`, where the trigger byte
/// has already been consumed, escape the remaining bytes within that context.
/// If there is a literal hex escape it is put as `\x5CxHH` where H is the hex.
/// Return the escaped string and the index to the terminator.
/// The terminator is not part of the returned string.
pub fn bytes_to_escaped_string_ex(bytes: &[u8], offset: usize, escapes: &[i64], terminator: &[u8], ctx: &str) -> (String,usize)
{
    assert!(ctx=="str" || ctx=="tok_data" || ctx=="tok_rem");
    const QUOTE: u8 = 34;
    const BACKSLASH: u8 = 92;
	let mut ans = String::new();
	let mut idx = offset;
	let mut quotes = 0;
    if ctx=="str" {
        quotes += 1;
    }
	while idx < bytes.len() {
		if ctx == "tok_data" && bytes[idx] == 0 {
			break;
        }
		if ctx == "tok_data" && quotes % 2 == 0 && terminator.contains(&bytes[idx]) {
			break;
        }
		if ctx != "tok_data" && terminator.contains(&bytes[idx]) {
			break;
        }
		if bytes[idx] == QUOTE {
			quotes += 1;
        }
		if bytes[idx] == BACKSLASH && idx + 3 < bytes.len() {
            let is_hex = |x: u8| -> bool {
                x>=48 && x<=57 || x>=65 && x<=70 || x>=97 && x<=102
            };
            if bytes[idx+1]==120 && is_hex(bytes[idx+2]) && is_hex(bytes[idx+3]) {
                ans += "\\x5c";
            } else {
                ans += "\\";
            }
        } else if escapes.contains(&(bytes[idx] as i64)) || bytes[idx] > 126 {
            let mut temp = String::new();
            write!(&mut temp,"\\x{:02x}",bytes[idx]).expect("unreachable");
            ans += &temp;
        } else {
			ans += std::str::from_utf8(&[bytes[idx]]).expect("unreachable");
        }
		idx += 1;
	}
	return (ans,idx);
}

/// Calls `bytes_to_escaped_string_ex` with `escapes` set to [10,13]
pub fn bytes_to_escaped_string(bytes: &[u8], offset: usize, terminator: &[u8], ctx: &str) -> (String,usize)
{
    let escapes = vec![10,13];
    bytes_to_escaped_string_ex(bytes, offset, &escapes, terminator, ctx)
}
