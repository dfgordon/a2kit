//! Module containing the Applesoft minifier

use json;
use log::error;
use tree_sitter;
use tree_sitter_applesoft;
use std::collections::{HashSet,HashMap};
use crate::lang;
use crate::lang::{Navigate,Navigation};
use super::minify_guards;
use crate::{STDRESULT,DYNERR};

/// minify using safe transformations only
pub const FLAG_SAFE: u64 = 1;
/// minify variables in ampersand expressions
pub const FLAG_AMP_VARS: u64 = 2;
/// delete unnecessary lines (and update references)
pub const FLAG_DEL_LINES: u64 = 4;
/// combine lines if possible
pub const FLAG_COMBINE_LINES: u64 = 8;

const FORBIDS_COMBINING_ANY: [&str;2] = [
	"tok_del",
	"tok_list",
];

const FORBIDS_COMBINING_NEXT: [&str;9] = [
	"tok_data",
	"tok_end",
	"tok_goto",
	"tok_if",
	"tok_then",
	"tok_resume",
	"tok_return",
	"tok_run",
	"tok_stop"
];

/// Handles minification of Applesoft BASIC
pub struct Minifier
{
	line: String,
	write_curs: usize,
	minified_line: String,
    minified_program: String,
	var_guards: json::JsonValue,
	deleted_lines: Vec<usize>,
	all_lines: Vec<usize>,
	line_map: HashMap<usize,usize>,
	curr_linenum: Option<usize>,
	flags: u64,
	pass: usize,
	ends_with_str: bool,
	forbids_combining_next: HashSet<usize>,
	linenum_refs: HashSet<usize>,
	forbids_combining_any: bool
}

impl Navigate for Minifier
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>
    {
		if self.pass==1 {
			self.visit_pass1(curs)
		} else if self.pass==2 {
			self.visit_pass2(curs)
		} else {
			self.visit_pass3(curs)
		}
    }
}

impl Minifier
{
	/// Create a new `Minifier` structure
    pub fn new() -> Self
    {
        Self {
			line: String::new(),
			write_curs: 0,
			minified_line: String::new(),
			minified_program: String::new(),
			var_guards: json::parse(minify_guards::VAR_GUARDS_JSON).expect("json error"),
			deleted_lines: Vec::new(),
			all_lines: Vec::new(),
			line_map: HashMap::new(),
			curr_linenum: None,
			flags: FLAG_SAFE,
			pass: 1,
			ends_with_str: false,
			forbids_combining_any: false,
			forbids_combining_next: HashSet::new(),
			linenum_refs: HashSet::new()
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
	/// Generate map from deleted or absorbed line numbers to replacement line numbers.
	/// Assumes `deleted_lines` and `all_lines` are already built. 
	fn set_line_ref_map(&mut self) -> STDRESULT {
		if self.all_lines.len() == 0 {
			return Ok(());
		}
		let mut curr_idx = 0;
		let mut curr_val = self.all_lines[curr_idx];
		for deleted in &self.deleted_lines {
			while *deleted >= curr_val || self.deleted_lines.contains(&curr_val) {
				curr_idx += 1;
				if curr_idx >= self.all_lines.len() {
					return Err(Box::new(crate::lang::Error::LineNumber));
				}
				curr_val = self.all_lines[curr_idx];
			}
			self.line_map.insert(*deleted,curr_val);
		}
		Ok(())
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
		if level>1 {
			self.flags |= FLAG_DEL_LINES;
		}
		if level>2 {
			self.flags |= FLAG_AMP_VARS;
			self.flags |= FLAG_COMBINE_LINES;
		}
		self.flags
	}

	/// The first pass makes intra-line transformations, which may include deleting the line.
    fn visit_pass1(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		let node_str: String = lang::node_text(&curs.node(),&self.line);

		// capture line numbers
		if curs.node().kind()=="linenum" {
			if let Some(parent) = curs.node().parent() {
				if let Some(num) = lang::node_integer::<usize>(&curs.node(), &self.line) {
					if parent.kind()=="line" {
						self.curr_linenum = Some(num);
						self.all_lines.push(num);
					} else {
						self.linenum_refs.insert(num);
					}
					self.minified_line += &num.to_string();
					return Ok(Navigation::GotoSibling);
				}
			}
			return Err(Box::new(crate::lang::Error::LineNumber));
		}

		// line combining prognosis
		if FORBIDS_COMBINING_ANY.contains(&curs.node().kind()) {
			self.forbids_combining_any = true;
		}
		if FORBIDS_COMBINING_NEXT.contains(&curs.node().kind()) {
			if let Some(linenum) = self.curr_linenum {
				self.forbids_combining_next.insert(linenum);
			}
		}

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
					// if no previous statement we keep token, or delete line
					if self.flags & FLAG_DEL_LINES > 0 && self.curr_linenum.is_some() {
						self.minified_line = String::new();
						self.deleted_lines.push(self.curr_linenum.unwrap());
						return Ok(Navigation::Exit);
					} else {
						self.minified_line += "REM";
						return Ok(Navigation::GotoSibling);
					}
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
				if curr.next_sibling().is_some() {
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
				if next.kind() == ":" {
					return Ok(Navigation::GotoSibling); // trailing node is another separator
				}
			} else {
				return Ok(Navigation::GotoSibling); // there is no trailing node
			}
		}
		if !curs.node().is_named() && node_str==";" {
			if let (Some(statement),Some(next)) = (curs.node().parent(),curs.node().next_sibling()) {
				if let Some(tok) = statement.child(0) {
					if tok.kind() == "tok_print" {
						if next.kind() == ";" {
							return Ok(Navigation::GotoSibling);
						} else if next.kind() == "str" {
							return Ok(Navigation::GotoSibling);
						} else if let Some(prev) = curs.node().prev_named_sibling() {
							let txt = lang::node_text(&prev,&self.line);
							if txt.ends_with("\"") || txt.ends_with(")") || txt.ends_with("$") {
								return Ok(Navigation::GotoSibling);
							}
						}
					}
				}
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

	/// The second pass updates line number references that were spoiled in the first pass
	fn visit_pass2(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		// replace line number references
		if curs.node().kind()=="linenum" {
			if let Some(parent) = curs.node().parent() {
				if parent.kind()!="line" {
					// this is a secondary
					if let Some(num) = lang::node_integer::<usize>(&curs.node(), &self.line) {
						if let Some(new_num) = self.line_map.get(&num) {
							self.minified_line += &self.line[self.write_curs..curs.node().byte_range().start];
							self.minified_line += &new_num.to_string();
							self.write_curs = curs.node().byte_range().end;
							return Ok(Navigation::GotoSibling);
						}
					}
				}
			}
		}
		return Ok(Navigation::GotoChild);
	}

	/// The third pass sets `curr_linenum` and `ends_with_str`
	fn visit_pass3(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		// capture primary line number
		if curs.node().kind()=="line" {
			if let Some(node) = curs.node().child(0) {
				if node.kind()=="linenum" {
					if let Some(num) = lang::node_integer::<usize>(&node, &self.line) {
						self.curr_linenum = Some(num);
					}
				}
			}
			return Ok(Navigation::GotoChild);
		}
		// analyze strings
		if curs.node().kind()=="str" {
			// if trailing nodes at this or any level up to line, keep searching
			let mut curr = curs.node();
			while curr.kind()!="line" {
				if curr.next_sibling().is_some() {
					return Ok(Navigation::GotoSibling);
				}
				if curr.parent()==None {
					break;
				};
				curr = curr.parent().unwrap();
			}
			self.ends_with_str = true;
			return Ok(Navigation::Exit);
		}
		Ok(Navigation::GotoChild)
	}

	/// intra-line transformations
	fn minify_stage1(&mut self,program: &str) -> Result<String,DYNERR> {
		self.minified_program = String::new();
		self.deleted_lines = Vec::new();
		self.all_lines = Vec::new();
		self.line_map = HashMap::new();
		self.forbids_combining_next = HashSet::new();
		self.linenum_refs = HashSet::new();
		self.forbids_combining_any = false;
		self.pass = 1;
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.trim().len()==0 {
				continue;
			}
			self.curr_linenum = None;
			self.minified_line = String::from(line) + "\n";
			for _rep in 0..10 {
				self.line = self.minified_line.clone();
				self.minified_line = String::new();
				let tree = parser.parse(&self.line,None).expect("Error parsing file");
				self.walk(&tree)?;
				if self.minified_line.len()==0 {
					break;
				}
				self.minified_line.push('\n');
				if self.minified_line==self.line {
					break;
				}
			}
			self.minified_program += &self.minified_line;
		}

		Ok(self.minified_program.clone())
	}

	/// repair line number references
	fn minify_stage2(&mut self,program: &str) -> Result<String,DYNERR> {
		self.minified_program = String::new();
		self.pass = 2;
		self.set_line_ref_map()?;
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.trim().len()==0 { // should never be
				continue;
			}
			self.write_curs = 0;
			self.curr_linenum = None;
			self.line = String::from(line) + "\n";
			self.minified_line = String::new();
			let tree = parser.parse(&self.line,None).expect("Error parsing file");
			self.walk(&tree)?;
			self.minified_line += &self.line[self.write_curs..];
			self.minified_program += &self.minified_line;
		}

		Ok(self.minified_program.clone())
	}

	/// combine lines
	fn minify_stage3(&mut self,program: &str) -> Result<String,DYNERR> {
		let max_len = 235; // leave room for a few extra chars
		let mut combining = false;
		let mut partial_line = String::new();
		self.minified_program = String::new();
		self.pass = 3;
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.trim().len()==0 { // should never be
				continue;
			}
			self.curr_linenum = None;
			self.line = String::from(line) + "\n";
			self.minified_line = String::new();
			let tree = parser.parse(&self.line,None).expect("Error parsing file");
			let last_line_ends_with_str = self.ends_with_str;
			self.ends_with_str = false;
			self.walk(&tree)?;
			let still_combining = partial_line.len() + line.len() <= max_len &&
				!self.linenum_refs.contains(&self.curr_linenum.unwrap_or(usize::MAX));
			if combining && still_combining {
				if last_line_ends_with_str {
					partial_line += "\"";
				}
				partial_line += ":";
				if let Some(idx) = line.find(|c: char| !c.is_digit(10)) {
					partial_line += &line[idx..];
				} else {
					return Err(Box::new(crate::lang::Error::ParsingError));
				}
			} else {
				if partial_line.len() > 0 {
					self.minified_program += &partial_line;
					self.minified_program += "\n";
				}
				partial_line = line.to_string();
			}
			combining = match self.curr_linenum {
				Some(linenum) => !self.forbids_combining_next.contains(&linenum),
				None => false
			};
		}
		if partial_line.len() > 0 {
			self.minified_program += &partial_line;
			self.minified_program += "\n";
		}

		Ok(self.minified_program.clone())
	}

	/// Try to reduce the size of a program.  Flags determine the allowed transformations.
	/// An incremental level is the outward facing means of setting flags.
	pub fn minify(&mut self,program: &str) -> Result<String,DYNERR> {
		if self.flags==0 {
			return Ok(program.to_string());
		}
		if self.flags & FLAG_SAFE == 0 {
			error!("incompatible flags");
			return Err(Box::new(crate::commands::CommandError::InvalidCommand));
		}
		//self.minify_stage1(program)
		let stage1 = self.minify_stage1(program)?;
		let stage2 = self.minify_stage2(&stage1)?;
		if self.flags & FLAG_COMBINE_LINES > 0 && !self.forbids_combining_any {
			self.minify_stage3(&stage2)
		} else {
			Ok(stage2)
		}
	}
}
