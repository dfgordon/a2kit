use lsp_types as lsp;
use tree_sitter;
use tree_sitter_merlin6502;
use std::sync::Arc;
use crate::lang::merlin::Symbols;
use crate::lang::server::{Tokens,SemanticTokensBuilder};
use crate::lang::{lsp_range, node_text, Navigate, Navigation};
use crate::DYNERR;

pub struct SemanticTokensProvider {
    parser: tree_sitter::Parser,
    row: isize,
	col: isize,
	line: String,
	curr_macro: Option<String>,
	builder: SemanticTokensBuilder,
	symbols: Arc<Symbols>
}

impl SemanticTokensProvider {
	pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_merlin6502::language()).expect("could not start parser");
		Self {
			parser,
			row: 0,
			col: 0,
			line: String::new(),
			curr_macro: None,
			builder: SemanticTokensBuilder::new(),
			symbols: Arc::new(Symbols::new())
		}
	}
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
}

impl Tokens for SemanticTokensProvider {
	fn get(&mut self, txt: &str) -> Result<lsp::SemanticTokens,DYNERR> {
		self.builder.reset();
		self.row = 0;
		for line in txt.lines() {
			self.col = 0;
			self.line = self.symbols.adjust_line(self.row, line, "\n");
			if self.line.starts_with(super::CALL_TOK) {
				// ASSUMPTION is col will be a byte offset and LSP position encoding is utf-16
				self.col = -2*(super::CALL_TOK.len_utf16() as isize);
			}
			if let Some(tree) = self.parser.parse(&self.line,None) {
				self.walk(&tree)?;
			}
			self.row += 1;
		}
		self.builder.clone_result()
	}
}

impl Navigate for SemanticTokensProvider {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		let mut rng = lsp_range(curs.node().range(),self.row,self.col);
        let knd = curs.node().kind();
		if knd == "macro_def" {
			self.curr_macro = Some(node_text(&curs.node(), &self.line));
			self.builder.push(rng,"macro");
			return Ok(Navigation::GotoSibling);
		}
		if knd == "psop_eom" {
			self.curr_macro = None;
			self.builder.push(rng,"function");
			return Ok(Navigation::GotoSibling);
		}
		if knd == "macro_ref" {
			self.builder.push(rng,"macro");
			return Ok(Navigation::GotoSibling);
		}
		if knd == "global_label" && self.curr_macro.is_some() {
			if let Some(mac) = self.symbols.macros.get(self.curr_macro.as_ref().unwrap()) {
				if mac.children.contains_key(&node_text(&curs.node(), &self.line)) {
					self.builder.push(rng,"parameter");
					return Ok(Navigation::GotoSibling);
				}
			}
		}
		if ["global_label","current_addr"].contains(&knd)
		{
			self.builder.push(rng,"enum");
			return Ok(Navigation::GotoSibling);
		}
		if knd=="local_label"
		{
			self.builder.push(rng,"parameter");
			return Ok(Navigation::GotoSibling);
		}
		if knd=="var_label" || knd=="var_mac"
		{
			self.builder.push(rng,"variable");
			return Ok(Navigation::GotoSibling);
		}
		if ["heading","comment"].contains(&knd)
		{
			self.builder.push(rng,"comment");
			return Ok(Navigation::GotoSibling);
		}
		if knd.starts_with("eop_")
		{
			self.builder.push(rng,"operator");
			return Ok(Navigation::GotoSibling);
		}
		if knd.starts_with("op_")
		{
			if let Some(child) = curs.node().named_child(0) {
				rng.end.character = child.range().start_point.column as u32;
			}
			self.builder.push(rng,"keyword");
			return Ok(Navigation::GotoChild);
		}
		if knd=="mode"
		{
			self.builder.push(rng,"keyword");
			return Ok(Navigation::GotoSibling);
		}
		if ["imm_prefix","addr_prefix","num_str_prefix","data_prefix"].contains(&knd)
		{
			self.builder.push(rng,"keyword");
			return Ok(Navigation::GotoSibling);
		}
		if knd.starts_with("psop_")
		{
			if let Some(child) = curs.node().named_child(0) {
				rng.end.character = child.range().start_point.column as u32;
			}
			self.builder.push(rng,"function");
			return Ok(Navigation::GotoChild);
		}
		if ["dstring","pchar","nchar","filename"].contains(&knd)
		{
			self.builder.push(rng,"string");
			return Ok(Navigation::GotoSibling);
		}
		if ["num","hex_data"].contains(&knd)
		{
			self.builder.push(rng,"number");
			return Ok(Navigation::GotoSibling);
		}
		if knd=="trailing" {
			self.builder.push(rng,"comment");
			return Ok(Navigation::GotoSibling);
		}
		return Ok(Navigation::GotoChild);
    }
}

