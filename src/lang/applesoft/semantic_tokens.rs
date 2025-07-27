use lsp_types as lsp;
use tree_sitter;
use tree_sitter_applesoft;
use crate::lang::server::{Tokens,SemanticTokensBuilder};
use crate::lang::{Navigate,Navigation,lsp_range};
use crate::DYNERR;

const FUNC_NAMES: [&str;25] = [
	"sgn",
	"int",
	"abs",
	"usr",
	"fre",
	"scrnp",
	"pdl",
	"pos",
	"sqr",
	"rnd",
	"log",
	"exp",
	"cos",
	"sin",
	"tan",
	"atn",
	"peek",
	"len",
	"str",
	"val",
	"asc",
	"chr",
	"left",
	"right",
	"mid"
];

pub struct SemanticTokensProvider {
    parser: tree_sitter::Parser,
    row: isize,
	line: String,
	base: SemanticTokensBuilder
}

impl SemanticTokensProvider {
	pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_applesoft::LANGUAGE.into()).expect("could not start parser");
		Self {
			parser,
			row: 0,
			line: "".to_string(),
			base: SemanticTokensBuilder::new()
		}
	}
}

impl Tokens for SemanticTokensProvider {
	fn get(&mut self, txt: &str) -> Result<lsp::SemanticTokens,DYNERR> {
		self.base.reset();
		self.row = 0;
		for line in txt.lines() {
			self.line = line.to_string() + "\n";
			if let Some(tree) = self.parser.parse(&self.line,None) {
				self.walk(&tree)?;
			}
			self.row += 1;
		}
		self.base.clone_result()
	}
}

impl Navigate for SemanticTokensProvider {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		let rng = lsp_range(curs.node().range(),self.row,0);
		if ["comment_text","tok_rem"].contains(&curs.node().kind()) // must precede tok_ handler
		{
			self.base.process_escapes(curs, &self.line, rng, "comment");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("tok_")
		{
            if FUNC_NAMES.contains(&&curs.node().kind()[4..]) {
                self.base.push(rng,"function");
            } else {
			    self.base.push(rng,"keyword");
            }
            return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind() == "linenum"
		{
			self.base.push(rng,"macro");
			return Ok(Navigation::GotoSibling);
		}
		if ["str","data_str","data_literal"].contains(&curs.node().kind())
		{
			self.base.process_escapes(curs, &self.line, rng, "string");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind()=="name_amp"
		{
            if let Some(child) = curs.node().named_child(0) {
                if child.kind().starts_with("tok_") {
                    return Ok(Navigation::GotoChild);
                }
            }
            self.base.push(rng,"string");
            return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind()=="name_fn"
		{
			self.base.push(rng,"function");
			return Ok(Navigation::GotoSibling);
		}
		if ["int","real","data_int","data_real"].contains(&curs.node().kind())
		{
			self.base.push(rng,"number");
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("name_")
		{
			self.base.push(rng, "variable");
			return Ok(Navigation::GotoSibling);
		}
		return Ok(Navigation::GotoChild);
    }
}

