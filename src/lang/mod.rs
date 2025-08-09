//! # Language Module
//! 
//! This module contains facilities for language transformations and analysis.
//! The root module `lang` contains code for navigating any syntax tree, and
//! general code for interacting with the CLI or the language servers.
//! 
//! The syntax trees are generated using Tree-sitter parsers, which reside in their own crates.
//! 
//! The submodules contain the specific language transformations and analysis.
//!
//! The language servers themselves are compiled to separate executables, and as
//! such, per rust convention, are in src/bin.  In particular, communication with a
//! language client is handled there, not here.

pub mod applesoft;
pub mod integer;
pub mod merlin;
pub mod linenum;
pub mod server;
pub mod disk_server;

use tree_sitter;
use lsp_types as lsp;
use colored::*;
use thiserror::Error;
use std::io;
use std::io::Write;
use num_traits::Num;
use std::str::FromStr;
use std::collections::BTreeMap;
use atty;

use crate::{STDRESULT,DYNERR};
const RCH: &str = "unreachable was reached";

pub enum Navigation {
    GotoSelf,
    GotoChild,
    GotoSibling,
    GotoParentSibling,
    Descend,
    Exit,
    Abort
}

#[derive(Error,Debug)]
pub enum Error {
    #[error("Syntax error")]
    Syntax,
    #[error("Invalid Line Number")]
    LineNumber,
    #[error("Tokenization error")]
    Tokenization,
    #[error("Detokenization error")]
    Detokenization,
    #[error("Parsing error")]
    ParsingError,
    #[error("Path not found")]
    PathNotFound,
    #[error("Out of range")]
    OutOfRange,
    #[error("Could not parse URL")]
    BadUrl
}

/// This works by normalizing to the server's convention, i.e., anything that comes from the
/// client must be normalized.
/// As an example, the client may send "file:///c%3A" while the server generates "file:///C:".
pub fn normalize_client_uri(uri: lsp::Url) -> lsp::Url {
    if let Ok(path) = uri.to_file_path() {
        if let Ok(new_uri) = lsp::Url::from_file_path(path) {
            return new_uri;
        }
    }
    uri
}

/// This works by normalizing to the server's convention, i.e., anything that comes from the
/// client must be normalized.
/// As an example, the client may send "file:///c%3A" while the server generates "file:///C:".
pub fn normalize_client_uri_str(uri: &str) -> Result<lsp::Url,DYNERR> {
    if let Ok(parsed) = lsp::Url::parse(uri) {
        if let Ok(path) = parsed.to_file_path() {
            if let Ok(new_uri) = lsp::Url::from_file_path(path) {
                return Ok(new_uri);
            }
        }
        return Ok(parsed);
    }
    return Err(Box::new(Error::PathNotFound));
}

/// Text document packed up with URI string and version information.
/// This is similar to the LSP `TextDocumentItem`, except that it originates
/// on the server side, or from the CLI.
/// There are internally defined URI's for strings and macros.
#[derive(Clone)]
pub struct Document {
    pub uri: lsp::Url,
    pub version: Option<i32>,
    pub text: String
}

impl Document {
    pub fn new(uri: lsp::Url,text: String) -> Self {
        Self {
            uri,
            version: None,
            text
        }
    }
    pub fn from_string(text: String, id: u64) -> Self {
        Self {
            uri: lsp::Url::from_str(&format!("string:{}",id)).expect(RCH),
            version: None,
            text
        }
    }
    pub fn from_macro(text: String, label: String) -> Self {
        Self {
            uri: lsp::Url::from_str(&format!("macro:{}",label)).expect(RCH),
            version: None,
            text
        }
    }
    pub fn from_file_path(path: &std::path::Path) -> Result<Self,DYNERR> {
        let by = std::fs::read(path)?;
        let text = String::from_utf8(by)?;
        if let Ok(uri) = lsp::Url::from_file_path(path) {
            Ok(Self {
                uri,
                version: None,
                text
            })
        } else {
            Err(Box::new(Error::PathNotFound))
        }
    }
}

pub fn range_contains_pos(rng: &lsp::Range, pos: &lsp::Position) -> bool
{
	if pos.line < rng.start.line || pos.line > rng.end.line {
		return false;
    }
	if pos.line == rng.start.line && pos.character < rng.start.character {
		return false;
    }
	if pos.line == rng.end.line && pos.character > rng.end.character {
		return false;
    }
	return true;
}

pub fn range_contains_range(outer: &lsp::Range, inner: &lsp::Range) -> bool
{
	if inner.start.line < outer.start.line || inner.end.line > outer.end.line {
		return false;
    }
	if inner.start.line == outer.start.line && inner.start.character < outer.start.character {
		return false;
    }
	if inner.end.line == outer.end.line && inner.end.character > outer.end.character {
		return false;
    }
	return true;
}

pub fn translate_pos(pos: &lsp::Position, dl: isize, dc: isize) -> lsp::Position {
    let mut ans = lsp::Position::new(0,0);
    ans.line = match pos.line as isize + dl < 0 {
        true => 0,
        false => (pos.line as isize + dl) as u32
    };
    ans.character = match pos.character as isize + dc < 0 {
        true => 0,
        false => (pos.character as isize + dc) as u32
    };
    ans
}

pub fn range_union(r1: &lsp::Range,r2: &lsp::Range) -> lsp::Range {
    lsp::Range::new(
        lsp::Position::new(
            match r1.start.line < r2.start.line { true => r1.start.line, false => r2.start.line },
            match r1.start.line < r2.start.line || r1.start.line == r2.start.line && r1.start.character < r2.start.character {
                true => r1.start.character,
                false => r2.start.character
            }
        ),
        lsp::Position::new(
            match r2.end.line > r1.end.line { true => r2.end.line, false => r1.end.line },
            match r2.end.line > r1.end.line || r2.end.line == r1.end.line && r2.end.character > r1.end.character {
                true => r2.end.character,
                false => r1.end.character
            }
        )
    )
}

pub fn lsp_range(rng: tree_sitter::Range,row: isize,col: isize) -> lsp::Range {
    lsp::Range {
        start: lsp::Position { line: (row + rng.start_point.row as isize) as u32, character: (col + rng.start_point.column as isize) as u32 },
        end: lsp::Position { line: (row + rng.end_point.row as isize) as u32, character: (col + rng.end_point.column as isize) as u32}
    }
}

/// Get text of the node, returning null string if there is any error
pub fn node_text(node: &tree_sitter::Node,source: &str) -> String {
    if let Ok(ans) = node.utf8_text(source.as_bytes()) {
        return ans.to_string();
    }
    return "".to_string();
}

/// Parse a node that is expected to be an integer literal and put into generic type,
/// if node cannot be parsed return None.  This will ignore all spaces.
pub fn node_integer<T: FromStr>(node: &tree_sitter::Node,source: &str) -> Option<T> {
    let txt = node_text(&node,source).replace(" ","");
    match txt.parse::<T>() {
        Ok(num) => Some(num),
        Err(_) => None
    }
}

/// Parse a node that may use a prefix to indicate radix, e.g., `$0F` or `%00001111`.
/// This will ignore all spaces and underscores, except for an underscore prefix.
pub fn node_radix<T: Num>(node: &tree_sitter::Node, source: &str, hex: &str, bin: &str) -> Option<T> {
    if let Ok(s) = node.utf8_text(source.as_bytes()) {
        let mut trimmed = s.to_string().replace(" ","").replace("_","");
        if s.starts_with("_") && (hex=="_" || bin=="_") {
            trimmed = ["_",&trimmed].concat();
        }
        if trimmed.starts_with(hex) {
            match T::from_str_radix(&trimmed[1..],16) {
                Ok(ans) => Some(ans),
                Err(_) => None
            }
        } else if trimmed.starts_with(bin) {
            match T::from_str_radix(&trimmed[1..],2) {
                Ok(ans) => Some(ans),
                Err(_) => None
            }
        } else {
            match T::from_str_radix(&trimmed,10) {
                Ok(ans) => Some(ans),
                Err(_) => None
            }
        }
    } else {
        None
    }
}

pub fn named_sibling(node: tree_sitter::Node,skip: usize) -> Option<tree_sitter::Node> {

    let mut maybe = Some(node);
    for _i in 0..skip {
        maybe = maybe.unwrap().next_named_sibling();
        if maybe.is_none() {
            return None;
        }
    }
    maybe
}

/// Extend a zero range one code point in either direction, if possible.
/// Finite ranges are not modified.
/// The byte range is not modified in any case.
pub fn extended_range(node: &tree_sitter::Node,end_col: usize) -> tree_sitter::Range {
    let mut ans = node.range();
    if ans.start_point.column == ans.end_point.column {
        if ans.start_point.column > 0 {
            ans.start_point.column -= 1;
        }
        if ans.end_point.column + 1 < end_col {
            ans.end_point.column += 1;
        }
    }
    ans
}

/// Try to update a named boolean from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_bool(maybe_obj: &serde_json::Value, key: &str, curr: &mut bool) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_bool() { Some(x) => *curr = x, _ => {} };
        }
    }
}

/// Try to update a named integer from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_i64(maybe_obj: &serde_json::Value, key: &str, curr: &mut i64) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_i64() { Some(x) => *curr = x, _ => {} };
        }
    }
}

/// Try to update a named float from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_f64(maybe_obj: &serde_json::Value, key: &str, curr: &mut f64) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_f64() { Some(x) => *curr = x, _ => {} };
        }
    }
}

/// Try to update a named optional string from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_string_opt(maybe_obj: &serde_json::Value, key: &str, curr: &mut Option<String>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_str() { Some(x) => *curr = Some(x.to_string()), _ => {} };
        }
    }
}

/// Try to update a named string from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_string(maybe_obj: &serde_json::Value, key: &str, curr: &mut String) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_str() { Some(x) => *curr = x.to_string(), _ => {} };
        }
    }
}

/// Try to update a named severity from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_severity(maybe_obj: &serde_json::Value, key: &str, curr: &mut Option<lsp::DiagnosticSeverity>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            match x.as_str() {
                Some("ignore") => *curr = None,
                Some("hint") => *curr = Some(lsp::DiagnosticSeverity::HINT),
                Some("info") => *curr = Some(lsp::DiagnosticSeverity::INFORMATION),
                Some("warn") => *curr = Some(lsp::DiagnosticSeverity::WARNING),
                Some("error") => *curr = Some(lsp::DiagnosticSeverity::ERROR),
                _ => {}
            }
        }
    }
}

/// Try to update a named list of integers from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_vec(maybe_obj: &serde_json::Value, key: &str, curr: &mut Vec<i64>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            let mut ans: Vec<i64> = Vec::new();
            if let Some(a) = x.as_array() {
                for v in a {
                    match v.as_i64() {
                        Some(i) => ans.push(i),
                        None => return
                    }
                }
                *curr = ans;
            }
        }
    }
}

/// Try to update a named list of strings from a serde `Value` presumed to be an object.
/// If there is any error do not change the value.
pub fn update_json_vec_str(maybe_obj: &serde_json::Value, key: &str, curr: &mut Vec<String>) {
    if let Some(outer) = maybe_obj.as_object() {
        if let Some(x) = outer.get(key) {
            let mut ans: Vec<String> = Vec::new();
            if let Some(a) = x.as_array() {
                for v in a {
                    match v.as_str() {
                        Some(s) => ans.push(s.to_owned()),
                        None => return
                    }
                }
                *curr = ans;
            }
        }
    }
}

pub trait Navigate {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>;
    fn descend(&mut self,_curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
        Ok(Navigation::GotoSibling)
    }
    fn walk(&mut self,tree: &tree_sitter::Tree) -> Result<(),DYNERR>
    {
        let mut curs = tree.walk();
        let mut choice = Navigation::GotoSelf;
        while ! matches!(choice,Navigation::Exit | Navigation::Abort)
        {
            if matches!(choice,Navigation::GotoSelf) {
                choice = self.visit(&curs)?;
            } else if matches!(choice,Navigation::Descend) {
                choice = self.descend(&curs)?;
            } else if matches!(choice,Navigation::GotoChild) && curs.goto_first_child() {
                choice = self.visit(&curs)?;
            } else if matches!(choice,Navigation::GotoParentSibling) && curs.goto_parent() && curs.goto_next_sibling() {
                choice = self.visit(&curs)?;
            } else if matches!(choice,Navigation::GotoSibling) && curs.goto_next_sibling() {
                choice = self.visit(&curs)?;
            } else if curs.goto_next_sibling() {
                choice = self.visit(&curs)?;
            } else if curs.goto_parent() {
                choice = Navigation::GotoSibling;
            } else {
                choice = Navigation::Exit;
            }
        }
        Ok(())
    }
}


/// Simple verify, returns an error if syntax check fails, but does not run full diagnostics.
/// This is used by the CLI to interrupt the pipeline when a bad language file is encountered.
/// Works for any language, provided it is line-oriented.
pub fn verify_str(lang: tree_sitter::Language,code: &str) -> STDRESULT {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang)?;
    let mut iter = code.lines();
    let mut row = 0;
    while let Some(line) = iter.next()
    {
        match parser.parse(String::from(line) + "\n",None) {
            Some(tree) => {
                let curs = tree.walk();
                if curs.node().has_error() {
                    log::error!("syntax error in row {}, use `verify` for more details",row);
                    return Err(Box::new(Error::Syntax));
                }
            },
            None => {
                log::error!("unable to parse row {}",row);
                return Err(Box::new(Error::Syntax));
            }
        }
        row += 1;
    }
    Ok(())
}

pub fn eprint_lines_sexpr(lang: tree_sitter::Language, program: &str, unwraps: usize) {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).expect("Error loading grammar");
    let mut iter = program.lines();
    eprintln!();
    while let Some(line) = iter.next()
    {
        if let Some(tree) = parser.parse(String::from(line) + "\n",None) {
            let mut curs = tree.walk();
            for _i in 0..unwraps {
                curs.goto_first_child();
            }
            eprintln!("{}",line.to_string());
            eprintln!("{}",curs.node().to_sexp());
        }
    }
}

/// Gather program lines from the console, panics if stdin is not the console
pub fn line_entry(lang: tree_sitter::Language,prompt: &str) -> String
{
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).expect("Error loading grammar");
    let mut code = String::new();
    if atty::is(atty::Stream::Stdin) {
        eprintln!("Line entry interface.");
        eprintln!("This is a blind accumulation of lines.");
        eprintln!("Verify occurs when entry is terminated.");
        eprintln!("Accumulated lines can be piped.");
        eprintln!("`bye` terminates.");
        loop {
            eprint!("{} ",prompt);
            let mut line = String::new();
            io::stderr().flush().expect("could not flush stderr");
            io::stdin().read_line(&mut line).expect("could not read stdin");
            if line=="bye\n" || line=="bye\r\n" {
                break;
            }
            code += &line;
        }
        return code;
    } else {
        panic!("line_entry was called with piped input");
    }
}

pub fn eprint_diagnostic(diag: &lsp::Diagnostic, program: &str) {
    // line search not very efficient, perhaps it will do...
    if let Some(sev) = diag.severity {
        if sev == lsp::DiagnosticSeverity::HINT {
            // at present this is used to dim conditional assembly,
            // and we don't want to flag it.
            return;
        }
    }
    let mut lines = program.lines();
    let mut maybe_line = None;
    for _i in 0..diag.range.start.line+1 {
        maybe_line = lines.next();
    }
    let [announcement,squiggle] = match diag.severity {
        Some(lsp::DiagnosticSeverity::ERROR) => ["Error".red(),"^".red()],
        Some(lsp::DiagnosticSeverity::WARNING) => ["Warning".bright_yellow(),"^".bright_yellow()],
        Some(lsp::DiagnosticSeverity::INFORMATION) => ["Information".bright_blue(),"^".bright_blue()],
        _ => ["Unexpected Notice".red(),"^".red()]
    };
    eprintln!("{} on line {}: {}",announcement,diag.range.start.line,diag.message);
    if let Some(line) = maybe_line {
        eprintln!("  {}",line);
        for _i in 0..diag.range.start.character+2 {
            eprint!(" ");
        }
        for _i in diag.range.start.character..diag.range.end.character {
            eprint!("{}",squiggle);
        }    
        eprintln!();
    } 
}

/// This assumes all CRLF have been filtered from `doc`.
/// CRLF in `raw_new` will be changed to LF.
fn replace_range(doc: &mut String, rng: lsp::Range, raw_new: &str) -> STDRESULT {
    let new = raw_new.replace("\r\n","\n");
    let mut start_char = 0;
    let mut end_char = 0;
    let mut curr_line = 0;
    let mut found_start = false;
    let mut found_end = false;
    for line in doc.lines() {
        if rng.start.line == curr_line {
            start_char += rng.start.character;
            found_start = true;
        }
        if !found_start {
            start_char += line.chars().count() as u32 + 1;
        }
        if rng.end.line == curr_line {
            end_char += rng.end.character;
            found_end = true;
            break;
        }
        if !found_end {
            end_char += line.chars().count() as u32 + 1;
        }
        curr_line += 1;
    }
    if found_start && found_end {
        doc.replace_range(start_char as usize..end_char as usize,&new);
        return Ok(());
    }
    // there still could be an insertion at the end
    let line_count = doc.lines().count() as u32;
    if rng.start.line==line_count && rng.start.character==0 && rng.end.line==line_count && rng.end.character==0 {
        doc.push_str(&new);
        return Ok(());
    }
    Err(Box::new(Error::LineNumber))
}

/// Strategy is to sort edits bottom to top and apply in that sequence, this way the
/// meaning of a row doesn't change as we make the replacements.  Overlaps not allowed.
/// This is consistent with the LSP.  Preserves CRLF or LF, unless there is a mixture,
/// in which case LF wins.  Panics if row is out of range.
pub fn apply_edits(doc: &str, edits: &Vec<lsp::TextEdit>, row: u32) -> Result<String,DYNERR> {
    // TODO: check for overlaps
    let line_sep = match doc.split("\r\n").count() == doc.split("\n").count() {
        true => "\r\n",
        false => "\n"
    };
    let mut ans = String::from(doc);
    ans = ans.replace("\r\n","\n");
    let mut sorted = BTreeMap::new();
    let mut idx: u32 = 0; // provide uniqueness in case of repeated insertions or deletions
    for edit in edits {
        let key = (edit.range.start.line,edit.range.start.character,idx);
        sorted.insert(key,edit.clone());
        idx += 1;
    }
    for edit in sorted.values().rev() {
        let offset_rng = lsp::Range::new(
            lsp::Position::new(edit.range.start.line - row,edit.range.start.character),
            lsp::Position::new(edit.range.end.line - row,edit.range.end.character)
        );
        log::trace!("replace {:?}",offset_rng);
        replace_range(&mut ans,offset_rng,&edit.new_text)?;
    }
    if line_sep == "\r\n" {
        ans = ans.replace("\n","\r\n");
    }
    Ok(ans)
}
