//! Module containing the Integer BASIC tokenizer

use std::collections::HashMap;

use tree_sitter;
use tree_sitter_integerbasic;
use crate::lang;
use crate::lang::{Navigate,Navigation};
use super::token_maps;
use log::{warn,error};
use crate::{STDRESULT,DYNERR};

/// Handles tokenization of Integer BASIC
pub struct Tokenizer
{
	line: String,
    tokenized_program: Vec<u8>,
    tokenized_line: Vec<u8>,
	tok_map: HashMap<&'static str,u8>,
	detok_map: HashMap<u8,&'static str>,
	config: super::settings::Settings
}

impl Navigate for Tokenizer
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>
    {
		// At this point we assume we have ASCII in self.line

		// Numbers to binary
		if curs.node().kind()=="linenum" || curs.node().kind()=="integer" {
			let cleaned = self.text(curs.node()).replace(" ","");
			// number should always be positive (minus sign is a unary operator),
			// but value is restricted to 32767, so use i16.
			if let Ok(num) = i16::from_str_radix(&cleaned,10) {
				let bytes = i16::to_le_bytes(num);
				if let Some(parent) = curs.node().parent() {
					if parent.kind()!="line" {
						let header = i16::to_string(&num).as_bytes()[0] + 128;
						self.tokenized_line.push(header);
					}
				}
				self.tokenized_line.push(bytes[0]);
				self.tokenized_line.push(bytes[1]);
				return Ok(Navigation::GotoSibling);
			}
			error!("number node did not parse as i16");
			self.line = "ERR".to_string();
			return Ok(Navigation::Exit);
		}
		// Anonymous nodes would go here

		// Positive ASCII tokens
		if let Some(tok) = self.tok_map.get(curs.node().kind()) {
			self.tokenized_line.push(*tok);
			if curs.node().kind().starts_with("com_") {
				warn!("{} is an immediate mode only command",self.text(curs.node()).to_ascii_uppercase());
			}
			return Ok(Navigation::GotoSibling);
		}
		// Variables to upper case and negative ASCII
		if curs.node().kind()=="str_name" || curs.node().kind()=="int_name" {
			let cleaned = self.text(curs.node()).to_uppercase().replace(" ","").as_bytes().to_vec();
			let mut neg = cleaned.iter().map(|b| {
				if *b==36 {
					64 // tokenize $
				} else {
					b+128
				}
			}).collect();
			self.tokenized_line.append(&mut neg);
			return Ok(Navigation::GotoSibling);
		}
		// Strings including tokenized quotes and escapes
		if curs.node().kind()=="string" {
			let mut neg: Vec<u8> = vec![0x28];
			let txt = Self::stringlike_node_to_bytes(&self.text(curs.node()), false);
			neg.append(&mut txt[1..txt.len()-1].to_vec());
			neg.push(0x29);
			self.tokenized_line.append(&mut neg);
			return Ok(Navigation::GotoSibling);
		}
		// Comment text with escapes
		if curs.node().kind()=="comment_text" {
			let mut neg = Self::stringlike_node_to_bytes(&self.text(curs.node()), false);
			self.tokenized_line.append(&mut neg);
			return Ok(Navigation::GotoSibling);
		}

		// If none of the above, look for terminal nodes and strip spaces
		if curs.node().named_child_count()==0 {
			self.tokenized_line.append(&mut self.text(curs.node()).to_uppercase().replace(" ","").as_bytes().to_vec());
			return Ok(Navigation::GotoSibling);
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
			line: String::new(),
            tokenized_line: Vec::<u8>::new(),
            tokenized_program: Vec::<u8>::new(),
			tok_map: HashMap::from(token_maps::TOK_MAP),
			detok_map: HashMap::from(token_maps::DETOK_MAP),
			config: super::settings::Settings::new()
         }
    }
    pub fn set_config(&mut self,config: super::settings::Settings) {
        self.config = config;
    }
	fn text(&self,node: tree_sitter::Node) -> String {
		let rng = std::ops::Range {start: node.range().start_point.column, end: node.range().end_point.column};
		String::from(&self.line[rng])
	}
	/// will invert and capitalize
	fn stringlike_node_to_bytes(txt: &str,trim: bool) -> Vec<u8> {
		let trimmed = match trim { true => txt.trim_start().to_string(), false => txt.to_string() };
		return crate::parse_escaped_ascii(&trimmed, true, true);
	}
	fn tokenize_line(&mut self,parser: &mut tree_sitter::Parser) -> STDRESULT {
		self.tokenized_line = Vec::new();
		let tree = parser.parse(&self.line,None).expect("Error parsing file");
		self.walk(&tree)?;
		if self.line=="ERR" {
			return Err(Box::new(lang::Error::Syntax));
		}
		if self.tokenized_line.len()>126 {
			error!("integer BASIC line too long");
			return Err(Box::new(lang::Error::Syntax));
		}
		self.tokenized_line.insert(0,self.tokenized_line.len() as u8 +2);
		self.tokenized_line.push(1);
		Ok(())
	}
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes
	pub fn tokenize(&mut self,program: String) -> Result<Vec<u8>,DYNERR> {
		self.tokenized_program = Vec::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_integerbasic::language()).expect("error loading integer grammar");
		for line in program.lines() {
			if line.trim().len()==0 {
				continue;
			}
			self.line = String::from(line) + "\n";
			self.tokenize_line(&mut parser)?;
			self.tokenized_program.append(&mut self.tokenized_line);
		}
		Ok(self.tokenized_program.clone())
	}
	/// Detokenize from byte array into a UTF8 string
	pub fn detokenize(&self,img: &[u8]) -> Result<String,DYNERR> {
		const OPEN_QUOTE: u8 = 0x28;
		const CLOSE_QUOTE: u8 = 0x29;
		const REM_TOK: u8 = 93;
		const EOL: u8 = 0x01;
		let mut addr = 0;
		let mut code = String::new();
		let mut line_count = 0;
		while addr < 65536 && addr+2<img.len() && line_count < self.config.detokenizer.max_lines {
			addr += 1; //skip record length
			let line_num: u16 = img[addr] as u16 + img[addr+1] as u16*256;
			code += &(u16::to_string(&line_num) + " ");
			addr += 2;
			let mut escaped: String;
			for rep in 0..=self.config.detokenizer.max_line_length {
				if rep==self.config.detokenizer.max_line_length {
					error!("integer BASIC line is too long");
					return Err(Box::new(lang::Error::Syntax));
				}
				if addr >= img.len() {
					error!("program ended while processing line");
					return Err(Box::new(lang::Error::Detokenization));
				}
				if img[addr]==EOL {
					line_count += 1;
					code += "\n";
					addr += 1;
					break;
				} else if img[addr] == OPEN_QUOTE {
					code += "\"";
					(escaped,addr) = super::bytes_to_escaped_string_ex(&img, addr+1, &self.config.detokenizer.escapes, &[CLOSE_QUOTE,EOL]);
					code += &escaped;
					if img[addr] == CLOSE_QUOTE {
						code += "\"";
						addr += 1;
					}
				} else if img[addr] == REM_TOK {
					if !code.ends_with(" ") {
						code += " ";
					}
					code += "REM"; // real Apple II would add trailing space, breaking symmetry with tokenizer
					(escaped,addr) = super::bytes_to_escaped_string_ex(&img, addr+1, &self.config.detokenizer.escapes, &[EOL]);
					code += &escaped;
				} else if img[addr]<128 {
					if let Some(tok) = self.detok_map.get(&img[addr]) {
						if tok.len()>1 && *tok!="<>" && !code.ends_with(" ") {
							code += " ";
						}
						code += &tok.to_uppercase();
						if tok.len()>1 && *tok!="<>" && !tok.ends_with("(") && !tok.ends_with("=") {
							code += " ";
						}
						addr += 1;
					} else {
						error!("unrecognized integer BASIC token {} encountered",img[addr]);
						return Err(Box::new(lang::Error::Syntax));
					}
				} else if img[addr]>=176 && img[addr]<=185 {
					// next 2 bytes are a binary number
					if addr+2 >= img.len() {
						error!("program ended while processing integer");
						return Err(Box::new(lang::Error::Detokenization));
					}
					code += &u16::to_string(&u16::from_le_bytes([img[addr+1],img[addr+2]]));
					addr += 3;
				} else {
					// this is a variable name
					while img[addr]>=128 {
						code += &String::from_utf8(vec![img[addr]-128]).expect("expected negative ASCII was not found");
						addr += 1;
						if addr >= img.len() {
							error!("program ended while processing variable name");
							return Err(Box::new(lang::Error::Detokenization));
						}
					}
				}
			}
		}
		return Ok(code);
	}
	/// Given a full RAM image, find the Integer program and detokenize it
	pub fn detokenize_from_ram(&self,img: &[u8]) -> Result<String,DYNERR> {
		if img.len() < 0x8000 {
			error!("RAM image too small {}",img.len());
			return Err(Box::new(lang::Error::Detokenization));
		}
		let addr = img[202] as usize + img[203] as usize * 256;
		let himem = img[76] as usize + img[77] as usize * 256;
		self.detokenize(&img[addr..=himem])
	}
}
