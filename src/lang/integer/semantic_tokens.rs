use lsp_types as lsp;
use tree_sitter;
use tree_sitter_integerbasic;
use crate::lang::server::{Tokens,SemanticTokensBuilder};
use crate::lang::{Navigate,Navigation,lsp_range};
use crate::DYNERR;

pub struct SemanticTokensProvider {
    parser: tree_sitter::Parser,
    row: isize,
	line: String,
	builder: SemanticTokensBuilder
}

impl SemanticTokensProvider {
	pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_integerbasic::LANGUAGE.into()).expect("could not start parser");
		Self {
			parser,
			row: 0,
			line: "".to_string(),
			builder: SemanticTokensBuilder::new()
		}
	}
}

impl Tokens for SemanticTokensProvider {
	fn get(&mut self, txt: &str) -> Result<lsp::SemanticTokens,DYNERR> {
		self.builder.reset();
		self.row = 0;
		for line in txt.lines() {
			self.line = line.to_string() + "\n";
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
		let rng = lsp_range(curs.node().range(),self.row,0);
		if ["comment_text","statement_rem"].contains(&curs.node().kind()) // must precede statement handler
		{
			self.builder.process_escapes(curs, &self.line, rng, "comment");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("op_")
		{
			self.builder.push(rng,"keyword");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("statement_")
		{
			self.builder.push(rng,"keyword");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("fcall_")
		{
			self.builder.push(rng,"function");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind()=="linenum"
		{
			self.builder.push(rng,"macro");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind()=="string"
		{
			self.builder.process_escapes(curs, &self.line, rng, "string");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind()=="integer"
		{
			if let Some(prev) = curs.node().prev_named_sibling() {
			    if prev.kind()=="statement_goto" || prev.kind()=="statement_gosub" || prev.kind()=="statement_then_line" {
				    self.builder.push(rng,"macro");
					return Ok(Navigation::GotoSibling);
                }
            }
			self.builder.push(rng,"number");
			return Ok(Navigation::GotoSibling);
		}
		if super::SIMPLE_VAR_TYPES.contains(&curs.node().kind())
		{
			self.builder.push(rng,"variable");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("com_")
		{
			self.builder.push(rng, "keyword");
			return Ok(Navigation::GotoSibling);
		}
		return Ok(Navigation::GotoChild);
    }
}

