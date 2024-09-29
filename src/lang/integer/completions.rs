use lsp_types as lsp;
use super::settings::Settings;
use std::sync::Arc;
use tree_sitter;
use tree_sitter_integerbasic;
use super::Symbols;
use a2_memory_map::{MemoryMap,AddressInfo};

use crate::lang::{Navigation,Navigate,lsp_range,range_contains_pos};
use crate::lang::server::Completions;
use crate::DYNERR;

fn next_line(maybe_prev: Option<&str>,maybe_prev_prev: Option<&str>) -> Vec<lsp::CompletionItem>
{
    let line_patt = regex::Regex::new(r"^\s*[0-9][0-9 ]*").expect("regex failed");
	let prev_prev = match maybe_prev_prev {
		Some(s) => match line_patt.find(s) {
			Some(s) => match s.as_str().to_string().replace(" ","").parse::<u32>() {
				Ok(n) => n,
				Err(_) => 0
			}
			None => 0
		},
		None => 0
	};
	let prev = match maybe_prev {
		Some(s) => match line_patt.find(s) {
			Some(s) => match s.as_str().to_string().replace(" ","").parse::<u32>() {
				Ok(n) => n,
				Err(_) => 0
			}
			None => 0
		},
		None => 0
	};
	let new_num = match prev > prev_prev {
		true => prev + (prev-prev_prev),
		false => prev + 10
	};
	let item: lsp::CompletionItem = lsp::CompletionItem {
		label: new_num.to_string() + " ",
		kind: Some(lsp::CompletionItemKind::CONSTANT),
		..Default::default()
	};
	return vec![item];
}

struct AddressCompletionProvider
{
    config: Settings,
	pokes : Vec<lsp::CompletionItem>,
	peeks : Vec<lsp::CompletionItem>,
	calls : Vec<lsp::CompletionItem>,
	poke_re: regex::Regex,
	peek_re: regex::Regex,
	call_re: regex::Regex
}

impl AddressCompletionProvider {
	pub fn new() -> Self
	{
		let config = Settings::new();
		let [pokes,peeks,calls] = Self::build(&config);
        Self {
            config,
			pokes,
			peeks,
			calls,
			poke_re: regex::Regex::new(r"(?i)POKE\s*$").expect("regex"),
			peek_re: regex::Regex::new(r"(?i)PEEK\s*$").expect("regex"),
			call_re: regex::Regex::new(r"(?i)CALL\s*$").expect("regex"),
        }
	}
    pub fn set_config(&mut self,config: Settings) {
		[self.pokes,self.peeks,self.calls] = Self::build(&config);
        self.config = config;
    }
	fn get_one(_config: &Settings,addr: &u16, addr_entry: &AddressInfo, prefix: &str, postfix: &str) -> lsp::CompletionItem {
		let mut num_addr = *addr as i64;
		num_addr = match num_addr >= (1<<15) { true => num_addr - 1 - u16::MAX as i64 , false => num_addr };
		let mut insert_text = prefix.to_string();
		insert_text += &num_addr.to_string();
		insert_text += postfix;
		let item = lsp::CompletionItem {
			label: match &addr_entry.label {
				Some(lab) => format!("{:8}{}",insert_text,lab),
				None => insert_text.clone()
			},
			kind: Some(lsp::CompletionItemKind::CONSTANT),
			documentation: Some(lsp::Documentation::String(addr_entry.desc.clone())),
			detail: match &addr_entry.brief {
				Some(brief) => Some(brief.clone()),
				None => Some(addr_entry.desc.clone())
			},
			insert_text: match &addr_entry.label {
				Some(_) => Some(insert_text),
				None => None
			},
			insert_text_format: match &addr_entry.label {
				Some(_) => Some(lsp::InsertTextFormat::PLAIN_TEXT),
				None => None
			},
			..Default::default()
		};
		item
	}
	fn build(config: &Settings) -> [Vec<lsp::CompletionItem>;3] {
		let a2map = MemoryMap::new();
		let mut pokes = Vec::new();
		let mut peeks = Vec::new();
		let mut calls = Vec::new();
		for (addr,obj) in a2map.get_all() {
			if let Some(ctx) = &obj.ctx {
				if ctx == "Applesoft" {
					continue;
				}
			}
			if !obj.typ.contains("soft switch") && !obj.typ.contains("routine") {
				pokes.push(Self::get_one(config,addr,obj,"",","));
				peeks.push(Self::get_one(config,addr,obj,"(",")"));
			}
			if obj.typ=="soft switch" {
				pokes.push(Self::get_one(config,addr,obj,"",",0"));
				peeks.push(Self::get_one(config,addr,obj,"(",")"));
			}
			if obj.typ.contains("routine") {
				calls.push(Self::get_one(config,addr,obj,"",""));
			}
		}
		return [pokes,peeks,calls];
	}
	fn get(&self, line: &str, col: usize) -> Vec<lsp::CompletionItem>
	{
		if col > line.len() {
			return Vec::new();
		}
		if col > 4 {
			let statement = line[0..col].to_string();
			if self.poke_re.is_match(&statement) {
				return self.pokes.clone();
			} else if self.peek_re.is_match(&statement) {
				return self.peeks.clone();
			} else if self.call_re.is_match(&statement) {
				return self.calls.clone();
			}
		}
		return vec![];
	}
}

struct StatementCompletionProvider {
    parser: tree_sitter::Parser,
	line: String,
	pos: lsp::Position,
	in_expr: bool,
	in_str: bool,
	in_statement: bool,
	config: Settings
}

impl Navigate for StatementCompletionProvider {
	fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		let rng = lsp_range(curs.node().range(),self.pos.line as isize,0);
		if !range_contains_pos(&rng, &self.pos) {
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().is_error() {
			self.in_statement = true;
			return Ok(Navigation::Exit);
		}
		if curs.node().kind() == "statement" {
			if curs.node().child_count()== 0 {
				self.in_statement = true;
				return Ok(Navigation::Exit);
			} else {
				return Ok(Navigation::GotoChild);
			}
		}
		if curs.node().kind() == "string" {
			self.in_str = true;
			return Ok(Navigation::Exit);
		}
		if let Some(prev) = curs.node().prev_named_sibling() {
			if prev.kind() == "statement" || prev.kind() == "linenum" {
				self.in_statement = true;
				return Ok(Navigation::Exit);
			}
			if prev.kind().starts_with("statement_") || prev.kind().starts_with("string") {
				self.in_expr = true;
				return Ok(Navigation::Exit);
			}
		}
		return Ok(Navigation::GotoChild);
	}
}

impl StatementCompletionProvider {
	fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_integerbasic::language()).expect("could not start Integer parser");
		Self {
			pos: lsp::Position::new(0,0),
			line: String::new(),
			in_expr: false,
			in_statement: false,
			in_str: false,
			parser,
			config: Settings::new()
		}
	}
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
	fn modify(&self,s: &str) -> String
	{
		if self.config.flag.case_sensitive.is_none() && self.config.completions.lower_case {
			return s.to_lowercase();
		}
		return s.to_string();
	}
	fn add_simple(&self,ans: &mut Vec<lsp::CompletionItem>,a2tok: &[&str])
	{
		for raw in a2tok {
			ans.push(lsp::CompletionItem {
				label: self.modify(raw),
				kind: Some(lsp::CompletionItemKind::KEYWORD),
				..Default::default()
			});
		}
	}
	fn add_funcs(&self,ans: &mut Vec<lsp::CompletionItem>,a2tok: &[&str],expr_typ: &str)
	{
		for raw in a2tok {
			let label = format!("{} ({})",self.modify(raw),expr_typ);
			ans.push(lsp::CompletionItem {
				label,
				kind: Some(lsp::CompletionItemKind::FUNCTION),
				insert_text: Some(self.modify(raw) + "(${1})"),
				insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
				..Default::default()
			});
		}
	}
	fn add_procs(&self,ans: &mut Vec<lsp::CompletionItem>,a2tok: &[&str],expr_typ: &str)
	{
		for raw in a2tok {
			let label = format!("{} {}",self.modify(raw),expr_typ);
			ans.push(lsp::CompletionItem {
				label,
				kind: Some(lsp::CompletionItemKind::KEYWORD),
				insert_text: Some(self.modify(raw) + "${0}"),
				insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
				..Default::default()
			});
		}
	}
	fn add_snippet(&self, ans: &mut Vec<lsp::CompletionItem>, lab: &str, snip: &str)
	{
		ans.push(lsp::CompletionItem {
			label: self.modify(lab),
			insert_text: Some(self.modify(snip)),
			insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
			..Default::default()
		});
	}
	fn get(&mut self,line: &str, pos: &lsp::Position, symbols: &Arc<Symbols>) -> Vec<lsp::CompletionItem>
	{
		let mut  ans = Vec::new();
		self.in_expr = false;
		self.in_str = false;
		self.in_statement = false;
		self.pos = pos.clone();

		self.line = line.to_string() + "\n";
		if let Some(tree) = self.parser.parse(&self.line, None) {
			if let Ok(()) = self.walk(&tree) {
				if self.in_str {
					return ans;
				}
		
				if self.in_expr || self.in_statement {
					for var in symbols.vars.values() {
						for lab in &var.case {
							// don't use add_snippet (would modify case)
							ans.push(lsp::CompletionItem {
								label: lab.to_string(),
								kind: match (var.is_array,var.is_string) {
									(true,false) => None,
									_ => Some(lsp::CompletionItemKind::VARIABLE),
								},
								insert_text: match (var.is_array,var.is_string) {
									(true,false) => Some(lab.to_owned() + "(${1:subscript})"),
									_ => None
								},
								insert_text_format: match (var.is_array,var.is_string) {
									(true,false) => Some(lsp::InsertTextFormat::SNIPPET),
									_ => None
								},
								..Default::default()
							});
						}
					}
					self.add_funcs(&mut ans, &["ABS", "PDL", "PEEK", "RND", "SGN"], "aexpr");
					self.add_funcs(&mut ans, &["ASC", "LEN"], "sexpr");
					self.add_snippet(&mut ans, "PEEK (special) (enter, space)", "PEEK");
					self.add_snippet(&mut ans, "SCRN (aexpr,aexpr)", "SCRN (${1:x},${0:y})");
				}
		
				if self.in_statement {
		
					self.add_simple(&mut ans, &["END", "GR", "INPUT",
					"NEXT", "NOTRACE", "POP", "PRINT", "REM", "RETURN",
					"TEXT", "TRACE"]);
					self.add_procs(&mut ans, &["CALL", "COLOR =", "IN#", "PR#", "VTAB"], "aexpr");
		
					self.add_snippet(&mut ans, "CALL special (enter, space)", "CALL");
				
					self.add_snippet(&mut ans, "DSP name", "DSP ${0:name}");
				
					self.add_snippet(&mut ans, "DIM name (subscript)", "DIM ${1:name} (${0:subscript})");
		
					self.add_snippet(&mut ans, "FOR index = first TO last: statement: NEXT", "FOR ${1:I} = ${2:1} TO ${3:last}: ${0}: NEXT");
					self.add_snippet(&mut ans, "FOR index = first TO last STEP s: statement: NEXT", "FOR ${1:I} = ${2:1} TO ${3:last} STEP ${4:step}: ${0}: NEXT");
		
					self.add_snippet(&mut ans, "GOSUB linenum", "GOSUB ${0:linenum}");
		
					self.add_snippet(&mut ans, "GOTO linenum", "GOTO ${0:linenum}");
		
					self.add_snippet(&mut ans, "HLIN aexpr,aexpr AT aexpr", "HLIN ${1:x1},${2:x2} AT ${0:y}");
		
					self.add_snippet(&mut ans, "IF expr THEN statement", "IF ${1} THEN ${0}");
		
					self.add_snippet(&mut ans, "LET var = expr", "LET ${1:var} = ${0:expr}");
					
					self.add_snippet(&mut ans, "LIST linenum, linenum", "LIST ${1:first}, ${0:last}");
		
					self.add_snippet(&mut ans, "NODSP name", "NODSP ${0:name}");
		
					self.add_snippet(&mut ans, "PLOT aexpr,aexpr", "PLOT ${1:x},${0:y}");
		
					self.add_snippet(&mut ans, "POKE aexpr,aexpr", "POKE ${1:addr},${0:val}");
		
					self.add_snippet(&mut ans, "POKE special (enter, space)", "POKE");
					
					self.add_snippet(&mut ans, "VLIN aexpr,aexpr AT aexpr", "VLIN ${1:y1},${2:y2} AT ${0:x}");
				}
			}
		}

		return ans;
	}
}

pub struct CompletionProvider {
	config: Settings,
	statement_tool: StatementCompletionProvider,
	address_tool: AddressCompletionProvider,
	symbols: Arc<Symbols>
}

impl CompletionProvider {
	pub fn new() -> Self {
		Self {
			config: Settings::new(),
			statement_tool: StatementCompletionProvider::new(),
			address_tool: AddressCompletionProvider::new(),
			symbols: Arc::new(Symbols::new())
		}
	}
    pub fn set_config(&mut self,config: Settings) {
		self.statement_tool.set_config(config.clone());
		self.address_tool.set_config(config.clone());
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
} 

impl Completions for CompletionProvider {
 fn get(&mut self,lines: &mut std::str::Lines, ctx: &lsp::CompletionContext, pos: &lsp::Position) -> Result<Vec<lsp::CompletionItem>,String> {
	let mut ans = Vec::new();

	let mut first = pos.line as i64 - 2;
	let prev_prev = match first >= 0 {
		true => lines.nth(first as usize),
		false => None
	};
	first += 1;
	let prev = match (first >= 0,prev_prev) {
		(true,Some(_)) => lines.next(),
		(true,None) => lines.nth(first as usize),
		(false,_) => None
	};
	first += 1;
	let curr_line = match prev {
		Some(_) => lines.next(),
		None => lines.nth(first as usize)
	};

	// items that only need current line
	if let Some(curr) = curr_line {
		if ctx.trigger_kind==lsp::CompletionTriggerKind::INVOKED {
			ans.append(&mut self.statement_tool.get(curr, pos, &self.symbols));
		}
		if let Some(trig) = &ctx.trigger_character {
			if trig.as_str() == " " {
				ans.append(&mut self.address_tool.get(curr,pos.character as usize));
			}
		}
	}

	// items that use the previous lines
	if let Some(trig) = &ctx.trigger_character {
		if trig.as_str() == "\n" {
			ans.append(&mut next_line(prev,prev_prev));
		}
	}

	Ok(ans) 
 }
}