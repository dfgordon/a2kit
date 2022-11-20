//! Module containing the Merlin "tokenizer"
//! 
//! This is really a text encoder/decoder for Merlin source code, the term "tokenizer" is analogical.
//! Whether we are encoding/decoding or tokenizing/detokenizing we are making a transformation
//! between modern and historical representations.

use tree_sitter;
use tree_sitter_merlin6502;
use crate::lang;
use crate::lang::Visit;

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

		// Column separator strategy is put before c2, before c3, and before c4
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
		if curs.node().kind()=="comment" {
			for _rep in self.columns..4 {
				self.tokenized_line.push(0xa0);
			}
		}
		match curs.field_name() {
			Some("c3") | Some("mac") => {
				self.tokenized_line.push(0xa0);
				self.columns = 3;
			},
			_ => {}
		};

		// If none of the above, look for terminal nodes
		if curs.node().child_count()==0 {
			self.tokenized_line.append(&mut self.text(curs.node()).as_bytes().to_vec());
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
			columns: 0
         }
    }
	fn text(&self,node: tree_sitter::Node) -> String {
		let rng = std::ops::Range {start: node.range().start_point.column, end: node.range().end_point.column};
		String::from(&self.line[rng])
	}
	fn tokenize_line(&mut self,parser: &mut tree_sitter::Parser) {
		self.columns = 1;
		self.tokenized_line = Vec::new();
		let tree = parser.parse(&self.line,None).expect("Error parsing file");
		self.walk(tree);
		if self.tokenized_line.len()>126 {
			panic!("Merlin line too long");
		}
		for curr in &mut self.tokenized_line {
			if *curr<128 && *curr!=32 {
				*curr += 128;
			}
		}
		self.tokenized_line.push(0x8d);
	}
	/// Tokenize a program contained in a UTF8 string, result is an array of bytes
	pub fn tokenize(&mut self,program: String) -> Vec<u8> {
		self.tokenized_program = Vec::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_merlin6502::language()).expect("error loading merlin grammar");
		for line in program.lines() {
			if line.len()==0 {
				self.tokenized_program.push(0x8d);
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
			for rep in 0..256 {
				if rep==255 {
					panic!("Merlin encoding appears to be broken");
				}
				if img[addr]==0x8d {
					code += "\n";
					addr += 1;
					break;
				}
				if img[addr]==0xa0 {
					code += "\t";
					addr += 1;
					break;
				}
				if img[addr]==32 || img[addr]==9 {
					code += &String::from_utf8(vec![img[addr]]).expect("expected ASCII was not found");
					addr += 1;
				}
				if img[addr]<128 {
					panic!("unexpected positive ASCII encountered");
				} else {
					code += &String::from_utf8(vec![img[addr]-128]).expect("expected negative ASCII was not found");
					addr += 1;
				}
			}
		}
		return code;
	}
}
