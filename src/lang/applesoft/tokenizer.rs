//! Module containing the Applesoft tokenizer

use std::collections::HashMap;

use tree_sitter;
use tree_sitter_applesoft;
use crate::lang;
use crate::lang::Visit;
use super::token_maps;
use crate::{STDRESULT,DYNERR};
use log::error;

/// Handles tokenization of Applesoft BASIC
pub struct Tokenizer
{
	line: String,
    tokenized_program: Vec<u8>,
    tokenized_line: Vec<u8>,
    curr_addr: u16,
	tok_map: HashMap<&'static str,u8>,
	detok_map: HashMap<u8,&'static str>
}

impl lang::Visit for Tokenizer
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> lang::WalkerChoice
    {
		// At this point we assume we have ASCII in self.line

		let rng = std::ops::Range {start: curs.node().range().start_point.column, end: curs.node().range().end_point.column};
		let node_str: String = String::from(&self.line[rng]);
		// Primary line number
		if curs.node().kind()=="linenum" {
			if let Some(parent) = curs.node().parent() {
				if parent.kind()=="line" {
					if let Ok(num) = u16::from_str_radix(&node_str.replace(" ",""),10) {
						let bytes = u16::to_le_bytes(num);
						self.tokenized_line.push(bytes[0]);
						self.tokenized_line.push(bytes[1]);
						return lang::WalkerChoice::GotoSibling;
					}
					panic!("linenum node did not parse as a number")
				}
			}
		}
		// Anonymous nodes
		if !curs.node().is_named() {
			let mut cleaned = node_str.to_uppercase().replace(" ","").as_bytes().to_vec();
			self.tokenized_line.append(&mut cleaned);
			return lang::WalkerChoice::GotoSibling;
		}
		// Negative ASCII tokens (except DATA will be intercepted by parent statement)
		if let Some(tok) = self.tok_map.get(curs.node().kind()) {
			self.tokenized_line.push(*tok);
			return lang::WalkerChoice::GotoSibling;
		}
		// Required upper case
		if curs.node().kind().starts_with("name_") || curs.node().kind()=="real" {
			if curs.node().kind()=="name_amp" && curs.node().child_count()>0 {
				// handle overloaded tokens
				return lang::WalkerChoice::GotoChild;
			}
			let mut cleaned = node_str.to_uppercase().replace(" ","").as_bytes().to_vec();
			self.tokenized_line.append(&mut cleaned);
			return lang::WalkerChoice::GotoSibling;
		}
		// Persistent spaces and escapes
		if curs.node().kind()=="statement" {
			if let Some(tok) = curs.node().named_child(0) {
				// Text in the DATA statement is preserved unconditionally, so handle all at once and go out.
				// There is a problem with calculation of end of data in connection with quote parity that
				// cannot be solved in any satisfactory way (ROM handles it inconsistently).
				if tok.kind()=="tok_data" {
					let items: String = String::from(&self.line[std::ops::Range {start: tok.end_byte(),end: curs.node().end_byte()}]);
					self.tokenized_line.push(*self.tok_map.get("tok_data").unwrap());
					self.tokenized_line.append(&mut Self::stringlike_node_to_bytes(&items,false));
					return lang::WalkerChoice::GotoSibling;
				}
			}
		}
		if curs.node().kind()=="str" {
			self.tokenized_line.append(&mut Self::stringlike_node_to_bytes(&node_str, true));
			return lang::WalkerChoice::GotoSibling;
		}
		if curs.node().kind()=="comment_text" {
			self.tokenized_line.append(&mut Self::stringlike_node_to_bytes(&node_str, false));
			return lang::WalkerChoice::GotoSibling;
		}

		// If none of the above, look for terminal nodes and strip spaces
		if curs.node().named_child_count()==0 {
			let mut cleaned = node_str.replace(" ","").as_bytes().to_vec();
			self.tokenized_line.append(&mut cleaned);
			return lang::WalkerChoice::GotoSibling;
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
            curr_addr: 2049,
			tok_map: HashMap::from(token_maps::TOK_MAP),
			detok_map: HashMap::from(token_maps::DETOK_MAP)
         }
    }
	fn stringlike_node_to_bytes(txt: &str,trim: bool) -> Vec<u8> {
		let ans = match trim { true => txt.trim_start().to_string(), false => txt.to_string() };
		return crate::parse_escaped_ascii(&ans, false, false);
	}
	fn tokenize_line(&mut self,parser: &mut tree_sitter::Parser) -> STDRESULT {
		self.tokenized_line = Vec::new();
		let tree = parser.parse(&self.line,None).expect("Error parsing file");
		self.walk(&tree);
		let next_addr = self.curr_addr + self.tokenized_line.len() as u16 + 3;
		let by: [u8;2] = u16::to_le_bytes(next_addr);
		self.tokenized_line.insert(0,by[0]);
		self.tokenized_line.insert(1,by[1]);
		self.tokenized_line.push(0);
		self.curr_addr = next_addr;
		Ok(())
	}
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes
	pub fn tokenize(&mut self,program: &str,start_addr: u16) -> Result<Vec<u8>,DYNERR> {
		self.curr_addr = start_addr;
		self.tokenized_program = Vec::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
		for line in program.lines() {
			if line.trim_start().len()==0 {
				continue;
			}
			self.line = String::from(line) + "\n";
			self.tokenize_line(&mut parser)?;
			self.tokenized_program.append(&mut self.tokenized_line);
		}
		self.tokenized_program.push(0);
		self.tokenized_program.push(0);
		Ok(self.tokenized_program.clone())
	}
	/// Detokenize from byte array into a UTF8 string
	pub fn detokenize(&self,img: &Vec<u8>) -> Result<String,DYNERR> {
		const DATA_TOK: u8 = 131;
		const REM_TOK: u8 = 178;
		const QUOTE: u8 = 34;
		let mut addr = 0;
		let mut code = String::new();
		while addr < 65536 && addr+1<img.len() && (img[addr]!=0 || img[addr+1]!=0) {
			addr += 2; //skip link address
			if addr+1 >= img.len() {
				error!("program ended before end of program marker");
				return Err(Box::new(lang::Error::Detokenization));
			}
			let line_num: u16 = img[addr] as u16 + img[addr+1] as u16*256;
			code += &(u16::to_string(&line_num) + " ");
			addr += 2;
			while addr < img.len() && img[addr]!=0 {
				if img[addr]==QUOTE {
					code += "\"";
					let (escaped,naddr) = super::super::bytes_to_escaped_string(img, addr+1, &[34,0], "str");
					code += &escaped;
					addr = naddr;
					if img[addr]==QUOTE {
						code += "\"";
						addr += 1;
					}
				} else if img[addr]==REM_TOK {
					code += " REM ";
					let (escaped,naddr) = super::super::bytes_to_escaped_string(img, addr+1, &[0], "tok_rem");
					code += &escaped;
					addr = naddr;
				} else if img[addr]==DATA_TOK {
					code += " DATA ";
					let (escaped,naddr) = super::super::bytes_to_escaped_string(img, addr+1, &[58,0], "tok_data");
					code += &escaped;
					addr = naddr;
				} else if img[addr]>127 {
					if let Some(tok) = self.detok_map.get(&img[addr]) {
						code += &(String::from(" ") + &tok.to_uppercase() + " ");
						addr += 1;
					} else {
						error!("unrecognized Applesoft token encountered");
						return Err(Box::new(lang::Error::Detokenization));
					}
				} else {
					code += &String::from_utf8(img[addr..addr+1].to_vec()).expect("expected ASCII was not found");
					addr += 1;
				}
			}
			code += "\n";
			addr += 1;
		}
		return Ok(code);
	}
}
