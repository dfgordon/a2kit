//! Module containing the Integer BASIC tokenizer

use std::collections::HashMap;

use tree_sitter;
use tree_sitter_integerbasic;
use super::super::walker;
use super::super::walker::Visit;
use super::token_maps;

/// Handles tokenization of Integer BASIC
pub struct Tokenizer
{
	line: String,
    tokenized_program: Vec<u8>,
    tokenized_line: Vec<u8>,
	tok_map: HashMap<&'static str,u8>,
	detok_map: HashMap<u8,&'static str>
}

impl walker::Visit for Tokenizer
{
    fn visit(&mut self,curs:&tree_sitter::TreeCursor) -> walker::WalkerChoice
    {
		// At this point we assume we have ASCII in self.line

		// Numbers to binary
		if curs.node().kind()=="linenum" || curs.node().kind()=="integer" {
			let cleaned = self.text(curs.node()).replace(" ","");
			if let Ok(num) = u16::from_str_radix(&cleaned,10) {
				let bytes = u16::to_le_bytes(num);
				if curs.node().kind()=="integer" {
					self.tokenized_line.push(cleaned.as_bytes()[0]+128);
				}
				self.tokenized_line.push(bytes[0]);
				self.tokenized_line.push(bytes[1]);
				return walker::WalkerChoice::GotoSibling;
			}
			panic!("number node did not parse as a number")
		}
		// Anonymous nodes (are there any?)
		// if !curs.node().is_named() {
		// 	let mut cleaned = self.text(curs.node()).to_uppercase().replace(" ","").as_bytes().to_vec();
		// 	self.tokenized_line.append(&mut cleaned);
		// 	return walker::WalkerChoice::GotoSibling;
		// }
		// Positive ASCII tokens
		if let Some(tok) = self.tok_map.get(curs.node().kind()) {
			self.tokenized_line.push(*tok);
			return walker::WalkerChoice::GotoSibling;
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
			return walker::WalkerChoice::GotoSibling;
		}
		// Persistent spaces
		if curs.node().kind()=="string" {
			// formerly tried iterating over children, but children() seemed to skip anonymous nodes
			let cleaned = self.text(curs.node()).to_uppercase().trim().as_bytes().to_vec();
			let mut neg: Vec<u8> = cleaned.iter().map(|b| b+128).collect();
			neg[0] = 0x28;
			neg[cleaned.len()-1] = 0x29;
			self.tokenized_line.append(&mut neg);
			return walker::WalkerChoice::GotoSibling;
		}
		if curs.node().kind()=="comment_text" {
			self.tokenized_line.append(&mut self.text(curs.node()).as_bytes().to_vec().iter().map(|b| b+128).collect());
			return walker::WalkerChoice::GotoSibling;
		}

		// If none of the above, look for terminal nodes and strip spaces
		if curs.node().child_count()==0 {
			self.tokenized_line.append(&mut self.text(curs.node()).replace(" ","").as_bytes().to_vec());
			return walker::WalkerChoice::GotoSibling;
		}

		return walker::WalkerChoice::GotoChild;
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
			detok_map: HashMap::from(token_maps::DETOK_MAP)
         }
    }
	fn text(&self,node: tree_sitter::Node) -> String {
		let rng = std::ops::Range {start: node.range().start_point.column, end: node.range().end_point.column};
		String::from(&self.line[rng])
	}
	fn tokenize_line(&mut self,parser: &mut tree_sitter::Parser) {
		self.tokenized_line = Vec::new();
		let tree = parser.parse(&self.line,None).expect("Error parsing file");
		self.walk(tree);
		if self.tokenized_line.len()>126 {
			panic!("integer BASIC line too long");
		}
		self.tokenized_line.insert(0,self.tokenized_line.len() as u8 +2);
		self.tokenized_line.push(1);
	}
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes
	pub fn tokenize(&mut self,program: String) -> Vec<u8> {
		self.tokenized_program = Vec::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_integerbasic::language()).expect("error loading integer grammar");
		for line in program.lines() {
			if line.len()==0 {
				continue;
			}
			self.line = String::from(line) + "\n";
			self.tokenize_line(&mut parser);
			self.tokenized_program.append(&mut self.tokenized_line);
		}
		return self.tokenized_program.clone();
	}
	/// Detokenize from byte array into a UTF8 string
	pub fn detokenize(&self,img: &Vec<u8>) -> String {
		let mut addr = 0;
		let mut code = String::new();
		while addr < 65536 && addr<img.len() {
			addr += 1; //skip record length
			let line_num: u16 = img[addr] as u16 + img[addr+1] as u16*256;
			code += &(u16::to_string(&line_num) + " ");
			addr += 2;
			while img[addr]!=1 {
				if img[addr]<128 {
					if let Some(tok) = self.detok_map.get(&img[addr]) {
						code += &(String::from(" ") + &tok.to_uppercase() + " ");
					} else {
						panic!("unrecognized integer BASIC token encountered");
					}
				} else {
					if img[addr]>=176 && img[addr]<=185 {
						// next 2 bytes are a binary number
						code += &u16::to_string(&u16::from_le_bytes([img[addr+1],img[addr+2]]));
						addr += 3;
					} else {
						// this is a variable name
						while img[addr]>=128 {
							code += &String::from_utf8(vec![img[addr]-128]).expect("expected negative ASCII was not found");
							addr += 1;
						}
					}
				}
			}
			code += "\n";
			addr += 1;
		}
		return code;
	}
}
