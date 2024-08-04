//! Module containing the Merlin "tokenizer"
//! 
//! The purpose of the (de)tokenizer is to move back and forth between the source file one finds
//! on an Apple II disk, and the source file one uses in a modern editor.

use std::sync::Arc;
use crate::lang;
use crate::lang::{Navigate,Navigation};
use log::{trace,error};
use crate::{STDRESULT,DYNERR};

/// Handles transformations between source encodings used by Merlin and ordinary text editors.
/// Merlin uses negative ASCII for all except spaces.  New line is 0x8d.
/// Spaces in strings or comments are positive ASCII, column separators are a single negative ASCII space.
pub struct Tokenizer
{
	parser: super::MerlinParser,
    tokenized_program: Vec<u8>,
    tokenized_line: Vec<u8>,
	columns: usize,
	widths: [usize;3],
	style: super::formatter::ColumnStyle,
	line_sep: String,
	symbols: Arc<super::Symbols>
}

impl lang::Navigate for Tokenizer
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>
    {
		// Three tasks here:
		// 1. convert string to ASCII bytes (to be inverted later)
		// 2. insert column separators
		// 3. clean comments (e.g. replace tabs with spaces)
		let parent = match curs.node().parent() {
			Some(p) => p,
			None => return Ok(Navigation::GotoChild)
		};

		// Handle root level comments and headings.
		// Root level comment needs no separator, Merlin will indent it automatically.
		if ["comment","heading"].contains(&curs.node().kind()) && parent.kind()=="source_file" {
			let txt = lang::node_text(&curs.node(), self.parser.line());
			trace!("visit: {}",txt);
			self.tokenized_line.append(&mut txt.replace("\t"," ").as_bytes().to_vec());
			return Ok(Navigation::GotoSibling);
		}

		// items that are at root level + 1
		if let Some(grandparent) = parent.parent() {
			if grandparent.kind()=="source_file" {
				if curs.node().kind().starts_with("op_") {
					self.tokenized_line.push(0xa0);
					self.columns = 2;
				}
				if curs.node().kind().starts_with("psop_") {
					self.tokenized_line.push(0xa0);
					self.columns = 2;
				}
				if curs.node().kind()=="macro_ref" {
					self.tokenized_line.push(0xa0);
					self.columns = 2;
				}
				if curs.node().kind().starts_with("arg_") {
					self.tokenized_line.push(0xa0);
					self.columns = 3;
				}
				if curs.node().kind()=="comment" {
					for _rep in self.columns..4 {
						self.tokenized_line.push(0xa0);
					}
				}
				let mut txt = lang::node_text(&curs.node(), self.parser.line());
				if txt.starts_with(super::CALL_TOK) {
					txt = txt[super::CALL_TOK.len_utf8()..].to_string();
				}
				if curs.node().kind()=="comment" {
					txt = txt.replace("\t"," ");
				}
				trace!("visit: {}",txt);
				self.tokenized_line.append(&mut txt.as_bytes().to_vec());
				return Ok(Navigation::GotoSibling);	
			}
		}

		return Ok(Navigation::GotoChild);
    }
}

impl Tokenizer
{
	/// Create a new `Tokenizer` structure
    pub fn new() -> Self
    {
        Self {
			parser: super::MerlinParser::new(),
            tokenized_line: Vec::<u8>::new(),
            tokenized_program: Vec::<u8>::new(),
			columns: 0,
			style: super::formatter::ColumnStyle::Variable,
			widths: [9,6,11],
			line_sep: "\n".to_string(),
			symbols: Arc::new(super::Symbols::new())
         }
    }
	pub fn set_config(&mut self,settings: &super::settings::Settings) {
		let c1: usize = settings.columns.c1.try_into().or::<usize>(Ok(9)).unwrap();
		let c2: usize = settings.columns.c2.try_into().or::<usize>(Ok(6)).unwrap();
		let c3: usize = settings.columns.c3.try_into().or::<usize>(Ok(11)).unwrap();
		self.widths = [c1,c2,c3];
	}
	/// Style to use during detokenization, formatting strategy is to tokenize, then
	/// detokenize using the chosen style.
	pub fn set_style(&mut self,style: super::formatter::ColumnStyle) {
		self.style = style;
	}
	pub fn use_shared_symbols(&mut self,sym: Arc<super::Symbols>) {
        self.symbols = sym;
    }
	fn tokenize_line(&mut self, line: &str) -> STDRESULT {
		self.columns = 1;
		self.tokenized_line = Vec::new();
		let tree = self.parser.parse(line, &self.symbols)?;
		self.walk(&tree)?;
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
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes.
	/// In general a full workspace scan is needed for this to succeed, due to the
	/// syntax rules of Merlin 8/16/16+, which allow for overlap of symbols and (pseudo)operations.
	/// If `use_shared_symbols` is not called first, false syntax errors are possible.
	pub fn tokenize(&mut self,program: String) -> Result<Vec<u8>,DYNERR> {
		self.line_sep = match program.find("\r\n") {
			Some(_) => "\r\n".to_string(),
			None => "\n".to_string()
		};
		self.tokenized_program = Vec::new();
		for line in program.lines() {
			if line.len()==0 {
				self.tokenized_program.push(0x8d);
				continue;
			}
			self.tokenize_line(line)?;
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
				line = super::formatter::format_tokens(&line, &self.style, self.widths);
				code += &line;
				code += &self.line_sep;
				addr += 1;
				line = String::new();
			} else if img[addr]==0xa0 {
				line += &super::COLUMN_SEPARATOR.to_string();
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
			line = super::formatter::format_tokens(&line, &self.style, self.widths);
			code += &line;
			code += &self.line_sep;
		}
		return Ok(code);
	}
}
