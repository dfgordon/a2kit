//! Module containing the Applesoft minifier

use json;

use tree_sitter;
use tree_sitter_applesoft;
use crate::lang;
use crate::lang::Visit;
use super::minify_guards;
use crate::DYNERR;

/// Handles minification of Applesoft BASIC
pub struct Minifier
{
	line: String,
	minified_line: String,
    minified_program: String,
	var_guards: json::JsonValue
}

impl lang::Visit for Minifier
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> lang::WalkerChoice
    {
		let node_str: String = lang::node_text(curs.node(),&self.line);

		// Shorten variable names
		if curs.node().kind().starts_with("name_") {
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
			return lang::WalkerChoice::GotoSibling;
		}

		// REM and DATA
		if curs.node().kind()=="statement" {
			if let Some(tok) = curs.node().named_child(0) {
				if tok.kind()=="tok_rem" {
					if let Some(prev) = curs.node().prev_named_sibling() {
						if prev.kind()=="statement" {
							// if there is a previous statement we can drop the whole comment
							return lang::WalkerChoice::GotoSibling;
						}
					}
					// if no previous statement we have to keep the token
					self.minified_line += "REM";
					return lang::WalkerChoice::GotoSibling;
				}
				// for DATA keep everything
				if tok.kind()=="tok_data" {
					self.minified_line += &node_str;
					return lang::WalkerChoice::GotoSibling;
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
					return lang::WalkerChoice::GotoSibling;
				}
				if curr.parent()==None {
					break;
				};
				curr = curr.parent().unwrap();
			}
			if node_str.ends_with("\"") {
				self.minified_line += &node_str[0..node_str.len()-1].trim_start();
			} else {
				self.minified_line += node_str.trim_start();
			}
			return lang::WalkerChoice::GotoSibling;
		}

		// Extraneous separators
		if !curs.node().is_named() && node_str==":" {
			if let Some(next) = curs.node().next_sibling() {
				if !next.is_named() && lang::node_text(next, &self.line)==":" {
					return lang::WalkerChoice::GotoSibling; // trailing node is another separator
				}
			} else {
				return lang::WalkerChoice::GotoSibling; // there is no trailing node
			}
		}

		// If none of the above, look for terminal nodes and strip spaces
		if curs.node().named_child_count()==0 {
			self.minified_line += &node_str.replace(" ","");
			return lang::WalkerChoice::GotoSibling;
		}

		return lang::WalkerChoice::GotoChild;
    }
}

impl Minifier
{
	/// Create a new `Tokenizer` structure
    pub fn new() -> Self
    {
        Self {
			line: String::new(),
			minified_line: String::new(),
			minified_program: String::new(),
			var_guards: json::parse(minify_guards::VAR_GUARDS_JSON).expect("json error")
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
	/// try to reduce the size of a program using simple transformations
	pub fn minify(&mut self,program: &str) -> Result<String,DYNERR> {
		self.minified_program = String::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.len()==0 {
				continue;
			}
			self.minified_line = String::from(line) + "\n";
			for _rep in 0..10 {
				self.line = self.minified_line.clone();
				self.minified_line = String::new();
				let tree = parser.parse(&self.line,None).expect("Error parsing file");
				self.walk(&tree);
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
