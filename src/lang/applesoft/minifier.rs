//! Module containing the Applesoft minifier

use json;
use log::error;
use tree_sitter;
use tree_sitter_applesoft;
use crate::lang;
use crate::lang::{Navigate,Navigation};
use super::minify_guards;
use crate::DYNERR;

/// minify using safe transformations only
pub const FLAG_SAFE: u64 = 1;
/// minify variables in ampersand expressions
pub const FLAG_AMP_VARS: u64 = 2;

/// Handles minification of Applesoft BASIC
pub struct Minifier
{
	line: String,
	minified_line: String,
    minified_program: String,
	var_guards: json::JsonValue,
	flags: u64
}

impl Navigate for Minifier
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>
    {
		let node_str: String = lang::node_text(&curs.node(),&self.line);

		// Shorten variable names
		if curs.node().kind().starts_with("name_") && !curs.node().kind().ends_with("amp") {
			let txt = node_str.replace(" ","");
			if txt.len()>3 && (curs.node().kind()=="name_str" || curs.node().kind()=="name_int") {
				self.minified_line += &txt[0..2];
				self.minified_line += &txt[txt.len()-1..txt.len()];
			} else if txt.len()>2 && curs.node().kind()!="name_str" && curs.node().kind()!="name_int" {
				if !self.needs_guard(&txt,curs) {
					self.minified_line += &txt[0..2];
				} else {
					if txt.len() > 4 {
						// if it is longer than 4 characters we gain something by guarding with parenthesis
						self.minified_line += "(";
						self.minified_line += &txt[0..2];
						self.minified_line += ")";
					} else {
						// otherwise do not change it
						self.minified_line += &txt;
					}
				}
			} else {
				self.minified_line += &txt;
			}
			return Ok(Navigation::GotoSibling);
		}

		// REM and DATA and ampersand
		if curs.node().kind()=="statement" {
			if let Some(tok) = curs.node().named_child(0) {
				if tok.kind()=="tok_rem" {
					if let Some(prev) = curs.node().prev_named_sibling() {
						if prev.kind()=="statement" {
							// if there is a previous statement we can drop the whole comment
							return Ok(Navigation::GotoSibling);
						}
					}
					// if no previous statement we have to keep the token
					self.minified_line += "REM";
					return Ok(Navigation::GotoSibling);
				}
				// for DATA always keep everything
				if tok.kind()=="tok_data" {
					self.minified_line += &node_str;
					return Ok(Navigation::GotoSibling);
				}
				// for ampersand keep everything unless flag is set
				if tok.kind()=="tok_amp" && (self.flags & FLAG_AMP_VARS == 0) {
					self.minified_line += &node_str;
					return Ok(Navigation::GotoSibling);
				}
			}
		}

		// Strings
		if curs.node().kind()=="str" {
			// if trailing nodes at this or any level up to line, keep the unquote
			let mut curr = curs.node();
			while curr.kind()!="line" {
				if curr.next_sibling()!=None {
					self.minified_line += node_str.trim_start();
					return Ok(Navigation::GotoSibling);
				}
				if curr.parent()==None {
					break;
				};
				curr = curr.parent().unwrap();
			}
			if node_str.ends_with("\"") && node_str.len()>1 {
				self.minified_line += &node_str[0..node_str.len()-1].trim_start();
			} else {
				self.minified_line += node_str.trim_start();
			}
			return Ok(Navigation::GotoSibling);
		}

		// Extraneous separators
		if !curs.node().is_named() && node_str==":" {
			if let Some(next) = curs.node().next_sibling() {
				if !next.is_named() && lang::node_text(&next, &self.line)==":" {
					return Ok(Navigation::GotoSibling); // trailing node is another separator
				}
			} else {
				return Ok(Navigation::GotoSibling); // there is no trailing node
			}
		}

		// If none of the above, look for terminal nodes and strip spaces
		if curs.node().named_child_count()==0 {
			self.minified_line += &node_str.replace(" ","");
			if curs.node().kind()=="tok_at" {
				self.minified_line += " ";
			}
			return Ok(Navigation::GotoSibling);
		}

		return Ok(Navigation::GotoChild);
    }
}

impl Minifier
{
	/// Create a new `Minifier` structure
    pub fn new() -> Self
    {
        Self {
			line: String::new(),
			minified_line: String::new(),
			minified_program: String::new(),
			var_guards: json::parse(minify_guards::VAR_GUARDS_JSON).expect("json error"),
			flags: FLAG_SAFE
		}
    }
	/// figure out if the short name needs to be guarded against forming a hidden token
	fn needs_guard(&self,clean_str: &str,curs: &tree_sitter::TreeCursor) -> bool {
		let short_str = clean_str[0..2].to_lowercase();
		let cannot_follow = &self.var_guards[short_str];
		if let Some(mut parent) = curs.node().parent() {
			while parent.next_named_sibling()==None {
				if parent.parent()==None {
					return false;
				}
				parent = parent.parent().unwrap();
			}
			let next = parent.next_named_sibling().unwrap();
			return cannot_follow.contains(next.kind());
		}
		return false;
	}
	/// set the minification flags
	pub fn set_flags(&mut self,flags: u64) {
		self.flags = flags;
	}
	/// set minification level, 0 means no transformation, higher levels will
	/// set increasing numbers of flags, the flags are returned
	pub fn set_level(&mut self,level: usize) -> u64 {
		self.flags = 0;
		if level>0 {
			self.flags |= FLAG_SAFE;
		}
		if level>2 {
			self.flags |= FLAG_AMP_VARS;
		}
		self.flags
	}
	/// try to reduce the size of a program using simple transformations
	pub fn minify(&mut self,program: &str) -> Result<String,DYNERR> {
		if self.flags==0 {
			return Ok(program.to_string());
		}
		if self.flags & FLAG_SAFE == 0 {
			error!("incompatible flags");
			return Err(Box::new(crate::commands::CommandError::InvalidCommand));
		}
		self.minified_program = String::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.trim().len()==0 {
				continue;
			}
			self.minified_line = String::from(line) + "\n";
			for _rep in 0..10 {
				self.line = self.minified_line.clone();
				self.minified_line = String::new();
				let tree = parser.parse(&self.line,None).expect("Error parsing file");
				self.walk(&tree)?;
				self.minified_line.push('\n');
				if self.minified_line==self.line {
					break;
				}
			}
			self.minified_program += &self.minified_line;
		}
		Ok(self.minified_program.clone())
	}
}
