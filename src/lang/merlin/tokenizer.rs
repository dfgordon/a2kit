//! Module containing the Merlin "tokenizer"
//! 
//! This is really a text encoder/decoder for Merlin source code, the term "tokenizer" is analogical.
//! Whether we are encoding/decoding or tokenizing/detokenizing we are making a transformation
//! between modern and historical representations.

use tree_sitter;
use tree_sitter_merlin6502;
use crate::lang;
use crate::lang::Visit;
use log::{trace,error};
use crate::{STDRESULT,DYNERR};

fn format_tokens(line:&str, sep:char) -> String {
	let widths = [9,6,11];
	let cols = line.split(sep);
	let mut ans = String::new();
	let mut col_idx = 0;
	for col in cols {
		let mut prepadding = 0;
		if let Some(c) = col.chars().next() {
			if c==';' {
				for j in col_idx..3 {
					prepadding += widths[j];
				}
			}
		}
		let w = match col_idx {
			i if i<3 => widths[i],
			_ => 1
		};
		let padding = match w as i32 - col.len() as i32 {
			x if x<1 => 1,
			x => x as usize
		};
		ans += &" ".repeat(prepadding);
		ans += col;
		ans += &" ".repeat(padding);
		col_idx += 1;
	}
	return ans.trim_end().to_string();
}

/// Handles transformations between source encodings used by Merlin and ordinary text editors.
/// Merlin uses negative ASCII for all except spaces.  New line is 0x8d.
/// Spaces in strings or comments are positive ASCII, column separators are a single negative ASCII space.
pub struct Tokenizer
{
	line: String,
    tokenized_program: Vec<u8>,
    tokenized_line: Vec<u8>,
	columns: usize
}

impl lang::Visit for Tokenizer
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> lang::WalkerChoice
    {
		// Two tasks here:
		// 1. convert string to ASCII bytes (to be inverted later)
		// 2. insert column separators
		let parent = match curs.node().parent() {
			Some(p) => p,
			None => return lang::WalkerChoice::GotoChild
		};

		// Handle root level comments and headings.
		// Root level comment needs no separator, Merlin will indent it automatically.
		if ["comment","heading"].contains(&curs.node().kind()) && parent.kind()=="source_file" {
			let txt = lang::node_text(curs.node(), &self.line);
			trace!("visit: {}",txt);
			self.tokenized_line.append(&mut txt.as_bytes().to_vec());
			return lang::WalkerChoice::GotoSibling;
		}

		// items that are at root level + 1
		if let Some(grandparent) = parent.parent() {
			if grandparent.kind()=="source_file" {
				if curs.node().kind().len()>3 {
					if &curs.node().kind()[0..3]=="op_" {
						self.tokenized_line.push(0xa0);
						self.columns = 2;
					}
				}
				if curs.node().kind().len()>5 {
					if &curs.node().kind()[0..5]=="psop_" {
						self.tokenized_line.push(0xa0);
						self.columns = 2;
					}
				}
				if curs.node().kind()=="macro_ref" {
					self.tokenized_line.push(0xa0);
					self.columns = 2;
				}
				if curs.node().kind().len()>4 {
					if &curs.node().kind()[0..4]=="arg_" {
						self.tokenized_line.push(0xa0);
						self.columns = 3;
					}
				}
				if curs.node().kind()=="comment" {
					for _rep in self.columns..4 {
						self.tokenized_line.push(0xa0);
					}
				}
				let txt = lang::node_text(curs.node(), &self.line);
				trace!("visit: {}",txt);
				self.tokenized_line.append(&mut txt.as_bytes().to_vec());
				return lang::WalkerChoice::GotoSibling;	
			}
		}

		return lang::WalkerChoice::GotoChild;
    }
}

impl Tokenizer
{
	/// Create a new `Tokenizer` structure
    pub fn new() -> Self
    {
        Self {
			line: String::new(),
            tokenized_line: Vec::<u8>::new(),
            tokenized_program: Vec::<u8>::new(),
			columns: 0
         }
    }
	fn tokenize_line(&mut self,parser: &mut tree_sitter::Parser) -> STDRESULT {
		self.columns = 1;
		self.tokenized_line = Vec::new();
		let tree = parser.parse(&self.line,None).expect("Error parsing file");
		self.walk(&tree);
		if self.tokenized_line.len()>126 {
			error!("Merlin line too long");
			return Err(Box::new(lang::Error::Syntax));
		}
		for curr in &mut self.tokenized_line {
			if *curr<128 && *curr!=32 {
				*curr += 128; 
			}
		}
		self.tokenized_line.push(0x8d);
		Ok(())
	}
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes
	pub fn tokenize(&mut self,program: String) -> Result<Vec<u8>,DYNERR> {
		self.tokenized_program = Vec::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_merlin6502::language()).expect("error loading merlin grammar");
		for line in program.lines() {
			if line.len()==0 {
				self.tokenized_program.push(0x8d);
				continue;
			}
			self.line = String::from(line) + "\n";
			self.tokenize_line(&mut parser)?;
			self.tokenized_program.append(&mut self.tokenized_line);
		}
		Ok(self.tokenized_program.clone())
	}
	/// Detokenize from byte array into a UTF8 string
	pub fn detokenize(&self,img: &Vec<u8>) -> Result<String,DYNERR> {
		let mut addr = 0;
		let mut line = String::new();
		let mut code = String::new();
		while addr < img.len() {
			if img[addr] == 0x8d {
				line = format_tokens(&line, char::from_u32(256).unwrap());
				code += &line;
				code += "\n";
				addr += 1;
				line = String::new();
			} else if img[addr]==0xa0 {
				line += &char::from_u32(256).unwrap().to_string();
				addr += 1;
			} else if img[addr]==32 || img[addr]==9 {
				line += &char::from_u32(img[addr] as u32).unwrap().to_string();
				addr += 1;
			} else if img[addr]<128 {
				error!("unexpected positive ASCII encountered");
				return Err(Box::new(lang::Error::Syntax));
			} else {
				line += &char::from_u32(img[addr] as u32 - 128).unwrap().to_string();
				addr += 1;
			}
		}
		if line.len() > 0 {
			line = format_tokens(&line,char::from_u32(256).unwrap());
			code += &line;
			code += "\n";
		}
		return Ok(code);
	}
}
