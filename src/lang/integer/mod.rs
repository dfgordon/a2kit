//! # Integer BASIC analysis and transformations
//! 
//! This module is used by both the CLI and the language server.
//! The Integer BASIC parser is provided by `tree_sitter_integerbasic`.
//! The server compiles to a separate executable, its entry point is in `src/bin/server-integerbasic/main.rs`.

mod token_maps;
#[cfg(test)]
mod tokenize_test;
#[cfg(test)]
mod detokenize_test;
#[cfg(test)]
mod renumber_test;
#[cfg(test)]
mod diagnostics_test;
pub mod tokenizer;
pub mod diagnostics;
pub mod checkpoint;
pub mod renumber;
pub mod settings;
pub mod hovers;
pub mod completions;
pub mod semantic_tokens;

use tree_sitter;
use lsp_types::Range;
use std::collections::{HashSet,HashMap};
use std::fmt::Write;
use crate::DYNERR;

pub const SIMPLE_VAR_TYPES: [&str;2] = [
	"str_name",
	"int_name",
];

pub const LEXPR: [&str;4] = [
	"str_name",
	"int_name",
	"str_array",
	"int_array"
	// str_slice is not an lexpr
];

pub const REXPR: [&str;5] = [
	"str_name",
	"int_name",
	"str_array",
	"int_array",
	"str_slice"
];

pub const ARRAY_OPEN: [&str;5] = [
	"open_str",
	"open_int",
	"open_slice",
	"open_dim_str",
	"open_dim_int"
];


/// Information about a specific line label.
/// The label itself is a key that maps to this information.
#[derive(Clone)]
pub struct Line {
    /// docstring taken from previous line or end of this line
    rem: Option<String>,
    /// the LSP range of the primary line number
    primary: Range,
    /// GOSUB statements referencing this line
    gosubs: Vec<Range>,
    /// GOTO statements referencing this line
    gotos: Vec<Range>
}

/// Information about a specific variable.
/// The name itself is a key that maps to this information.
/// The key is always put in uppercase.
/// There are 3 kinds: integers, integer arrays, and strings.
/// A string is an array of characters.
/// There are no arrays of strings.
/// There are two namespaces:
/// 1. Integers and integer arrays share a namespace (A=1 and A(0)=1 refer to the same memory)
/// 2. Strings have their own namespace (A$ = "HELLO" and A=0 refer to different memory)
/// When a string is subscripted, we also label it as an array (`is_array=true`), but this really
/// should be understood as a substring (to repeat, no arrays of strings).
#[derive(Clone)]
pub struct Variable {
    decs: Vec<Range>,
    defs: Vec<Range>,
    refs: Vec<Range>,
    is_array: bool,
    is_string: bool,
    case: HashSet<String>
}

impl Variable {
    pub fn new() -> Self {
        Self {
            decs: Vec::new(),
            defs: Vec::new(),
            refs: Vec::new(),
            is_array: false,
            is_string: false,
            case: HashSet::new()
        }
    }
    /// if rng is already in defs or decs, do nothing, otherwise push to refs
    fn push_ref_selectively(&mut self,rng: Range) {
        if self.decs.contains(&rng) {
            return;
        }
        if self.defs.contains(&rng) {
            return;
        }
        self.refs.push(rng);
    }
}

#[derive(Clone)]
pub struct Symbols {
    pub lines: HashMap<i64,Line>,
    pub vars: HashMap<String,Variable> 
}

impl Symbols {
    pub fn new() -> Self {
        Self {
            lines: HashMap::new(),
            vars: HashMap::new(),
        }
    }
    pub fn docstring(&self,linenum: i64) -> Option<String> {
        match self.lines.get(&linenum) {
            Some(l) => l.rem.clone(),
            None => None
        }
    }
}

/// Escape the bytes in some negative ASCII stringlike context.  The escape value is not inverted.
/// `bytes` are the bytes to escape, literal hex escapes will hex-escape the backslash (`\x5c`)
/// `offset` is the index to start of context, one past the triggering byte
/// `terminator` are characters that close the context
/// Returns escaped string and index to terminator, terminator not included in string
pub fn bytes_to_escaped_string_ex(bytes: &[u8], offset: usize, escapes: &[i64], terminator: &[u8]) -> (String,usize)
{
    const BACKSLASH: u8 = 128+92;
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
        } else if escapes.contains(&(bytes[idx] as i64)) || bytes[idx] > 254 || bytes[idx] < 128 {
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

/// Calls `bytes_to_escaped_string_ex` with `escapes` set to [138,141]
pub fn bytes_to_escaped_string(bytes: &[u8], offset: usize, terminator: &[u8]) -> (String,usize)
{
	let escapes = [138, 141];
	bytes_to_escaped_string_ex(bytes,offset,&escapes,terminator)
}

/// Assuming we are on an opening parenthesis in a DIM statement,
/// return the node of the corresponding closing parenthesis.
/// This is needed because DIM variables are parsed as a flat sequence.
/// @node the `open_dim_str` or `open_dim_int` node
pub fn pass_through_subscript<'a>(node: tree_sitter::Node<'a>,line: &str) -> Option<tree_sitter::Node<'a>> {
    let mut depth = 1;
    let mut maybe_next = node.next_named_sibling();
    while let Some(next) = maybe_next {
        if next.kind().starts_with("open_") {
            depth += 1;
        } else if next.kind().starts_with("close") {
            depth -= 1;
            if depth < 1 {
                break;
            }
        } else if next.kind().starts_with("fcall_") && super::node_text(&node, line).ends_with("(") {
            depth += 1;
        }
        maybe_next = next.next_named_sibling();
    }
    return maybe_next;
}

/// Gather key information for a variable.
/// @param node must be inner type, `str_name` or `int_name`
/// @returns (normalized name,specific name,is_array,is_str), key name includes `$` if a string
pub fn var_to_key(node: tree_sitter::Node,line: &str) -> (String,String,bool,bool)
{
	let next = node.next_named_sibling();
	let child = node.named_child(0);
	let is_array = match next { Some(n) => ARRAY_OPEN.contains(&n.kind()), None => false };
	let is_string = match child { Some(c) => c.kind() == "dollar", None => false };
	let n = super::node_text(&node,line).replace(" ", "");
	return (n.to_uppercase(),n,is_array,is_string);
}

/// Calls var_to_key, going down to find the inner name if necessary
/// @param node can be any LEXPR type
/// @param row current row being analyzed
/// @returns [normalized name,specific name,is_array,is_str,inner range], key name includes `$` if a string
pub fn lexpr_to_key(node: tree_sitter::Node,row: isize,line: &str) -> Result<(String, String, bool, bool, Range),DYNERR>
{
	if SIMPLE_VAR_TYPES.contains(&node.kind()) {
        let vk = var_to_key(node,line);
		return Ok((vk.0,vk.1,vk.2,vk.3,super::lsp_range(node.range(),row,0)));
    }
    if let Some(child) = node.named_child(0) {
        let vk = var_to_key(child,line);
		return Ok((vk.0,vk.1,vk.2,vk.3,super::lsp_range(child.range(),row,0)));
    }
	return Err(Box::new(super::Error::Syntax));
}
