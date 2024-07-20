use lsp_types as lsp;
use super::settings::Settings;
use std::str::Lines;
use std::sync::Arc;
use std::collections::{HashSet,HashMap};
use super::{MerlinVersion, ProcessorType, Symbols};
use a2_memory_map::{MemoryMap,AddressInfo};

use super::handbook::operations::OperationHandbook;
use super::handbook::pseudo_ops::PseudoOperationHandbook;
use crate::lang::server::Completions;

const ADDR_MODE_MAP: [(&str,&str);24] = [
	("imm", "#${0:imm}"),
	("abs", "${0:abs}"),
	("zp", "${0:zp}"),
	("rel", "${0:rel}"),
	("rell", "${0:rell}"),
	("absl", "${0:absl}"),
	("(zp,x)", "(${1:zp},x)$0"),
	("(abs,x)", "(${1:abs},x)$0"),
	("(zp),y", "(${1:zp}),y$0"),
	("zp,x", "${1:zp},x$0"),
	("abs,x", "${1:abs},x$0"),
	("absl,x", "${1:absl},x$0"),
	("zp,y", "${1:zp},y$0"),
	("abs,y", "${1:abs},y$0"),
	("(abs)", "(${1:abs})$0"),
	("(zp)", "(${1:zp})$0"),
	("[d]", "[${1:d}]$0"),
	("[d],y", "[${1:d}],y$0"),
	("d,s", "${1:d},s$0"),
	("(d,s),y", "(${1:d},s),y$0"),
	("xyc", "${1:dstbnk},${0:srcbnk}"),
	("impl", ""),
	("accum", ""),
	("s", "")
];

const RCH: &str = "unreachable was reached";

struct AddressCompletionProvider
{
    config: Settings,
	items : Vec<lsp::CompletionItem>,
	equ_re: regex::Regex
}

impl AddressCompletionProvider {
	pub fn new() -> Self
	{
		let config = Settings::new();
		let items = Self::build(&config);
        Self {
            config,
			items,
			equ_re: regex::Regex::new(r"(?i)(EQU|=)\s+\$$").expect("regex"),
        }
	}
    pub fn set_config(&mut self,config: Settings) {
		self.items = Self::build(&config);
        self.config = config;
    }
	fn get_one(addr: &u16, addr_entry: &AddressInfo, prefix: &str, postfix: &str) -> lsp::CompletionItem {
		let mut num_addr = *addr as i64;
		num_addr = match num_addr < 0 { true => num_addr + 1 + u16::MAX as i64, false => num_addr };
		let mut insert_text = prefix.to_string();
		let hex_str = match num_addr {
			x if x < 256 => format!("{:02X}",x),
			x if x < 65536 => format!("{:04X}",x),
			x => format!("{:X}",x)
		};
		insert_text += &hex_str;
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
	fn build(config: &Settings) -> Vec<lsp::CompletionItem> {
		let a2map = MemoryMap::new();
		let mut items = Vec::new();
		for (addr,obj) in a2map.get_all() {
			if let Some(ctx) = &obj.ctx {
				if !config.completions.ibas && ctx == "Integer BASIC" {
					continue;
				}
				if !config.completions.abas && ctx == "Applesoft" {
					continue;
				}
			}
			items.push(Self::get_one(addr,obj,"",""));
		}
		items
	}
	fn get(&self, line: &str, col: usize) -> Vec<lsp::CompletionItem>
	{
		if col > line.len() {
			return Vec::new();
		}
		if col > 4 {
			let statement = line[0..col].to_string();
			if self.equ_re.is_match(&statement) {
				return self.items.clone();
			}
		}
		return vec![];
	}
}

struct CodeCompletionProvider {
	symbols: Arc<Symbols>,
	op_book: OperationHandbook,
	psop_book: PseudoOperationHandbook,
	addr_mode_map: HashMap<String,String>,
	config: Settings,
	c2_regex: regex::Regex,
	c2_capture: regex::Regex,
	loc_regex: regex::Regex,
	var_regex: regex::Regex,
	c1_glob_regex: regex::Regex,
	c3_arg_regex: regex::Regex,
	c3_lab_regex: regex::Regex,
	pmc_regex: regex::Regex,
	iaddr_regex: regex::Regex,
	daddr_regex: regex::Regex
}

impl CodeCompletionProvider {
	fn new() -> Self {
		let mut addr_mode_map = HashMap::new();
		for (key,val) in ADDR_MODE_MAP {
			addr_mode_map.insert(key.to_string(),val.to_string());
		}
		Self {
			symbols: Arc::new(Symbols::new()),
			op_book: OperationHandbook::new(),
			psop_book: PseudoOperationHandbook::new(),
			addr_mode_map,
			config: Settings::new(),
			c2_regex: regex::Regex::new(r"^\S*\s+[A-Za-z]$").expect(RCH),
			c2_capture: regex::Regex::new(r"^\S*\s+(\S+)").expect(RCH),
			loc_regex: regex::Regex::new(r"(^:$|^\S*\s+\S+\s+:$)").expect(RCH),
			var_regex: regex::Regex::new(r"(^]$|^\S*\s+\S+\s+]$)").expect(RCH),
			c1_glob_regex: regex::Regex::new(r"^[a-zA-Z]$").expect(RCH),
			c3_arg_regex: regex::Regex::new(r"^\S*\s+\S+\s+[a-zA-Z]$").expect(RCH),
			c3_lab_regex: regex::Regex::new(r"^\S*\s+\S+\s+[#(\[<>|^]?[a-zA-Z]$").expect(RCH),
			pmc_regex: regex::Regex::new(r"^\S*\s+[pP][mM][cC]").expect(RCH),
			iaddr_regex: regex::Regex::new(r"^\S*\s+\S+\s+[\(\[]$").expect(RCH),
			daddr_regex: regex::Regex::new(r"^\S*\s+\S+\s+\S+,$").expect(RCH)
		}
	}
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
	fn modify(&self,s: &str,padreq: usize) -> String {
		if let Some(sev) = self.config.flag.case_sensitive {
			if self.config.completions.lower_case && sev != lsp::DiagnosticSeverity::ERROR {
				return [" ".repeat(padreq), s.to_lowercase()].concat();
			}
		}
		return [" ".repeat(padreq), s.to_uppercase()].concat();
	}
	fn add_simple(&self,ans: &mut Vec<lsp::CompletionItem>, a2tok: &Vec<String>) {
		for raw in a2tok {
			ans.push(lsp::CompletionItem {
				label: self.modify(raw,0),
				kind: Some(lsp::CompletionItemKind::KEYWORD),
				detail: match (self.op_book.get(raw),self.psop_book.get(raw)) {
					(Some(op),_) => Some(op.brief),
					(None,Some(psop)) => Some(psop.brief),
					_ => None
				},
				..Default::default()
			});
		}
	}
	fn add_label(&self,ans: &mut Vec<lsp::CompletionItem>,label_set: &HashSet<String>) {
		for raw in label_set {
			if raw.starts_with(":") {
				ans.push(lsp::CompletionItem {
					label: raw[1..].to_string(),
					kind: Some(lsp::CompletionItemKind::CONSTANT),
					detail: Some("local".to_string()),
					..Default::default()
				});
			} else if raw.starts_with("]") {
				ans.push(lsp::CompletionItem {
					label: raw[1..].to_string(),
					kind: Some(lsp::CompletionItemKind::VARIABLE),
					detail: Some("variable".to_string()),
					..Default::default()
				});
			} else {
				ans.push(lsp::CompletionItem {
					label: raw.to_string(),
					kind: Some(lsp::CompletionItemKind::CONSTANT),
					detail: Some("global".to_string()),
					..Default::default()
				});
			}
		}
	}
	fn add_snippet(&self, ans: &mut Vec<lsp::CompletionItem>, raw_lab: &str, raw_snip: &str, tab: bool)	{
		let lab = match tab {
			true => raw_lab.replace("\n",&format!("\n{}"," ".repeat(self.config.columns.c1 as usize))),
			false => raw_lab.to_string()
		};
		let snip = match tab {
			true => raw_snip.replace("\n",&format!("\n{}"," ".repeat(self.config.columns.c1 as usize))),
			false => raw_snip.to_string()
		};
		ans.push(lsp::CompletionItem {
			label: self.modify(&lab,0),
			insert_text: Some(self.modify(&snip,0)),
			insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
			..Default::default()
		});
	}
	fn add_indirect_arg(&self, ans: &mut Vec<lsp::CompletionItem>, op_str: &str, trig: &str, proc: &ProcessorType) {
		if let Some(op) = self.op_book.get(op_str) {
			for mode in op.modes {
				if let Some(snip) = self.addr_mode_map.get(&mode.mnemonic) {
					if mode.processors.contains(proc) && snip.starts_with(trig) {
						ans.push(lsp::CompletionItem {
							label: self.modify(&mode.mnemonic,0),
							detail: Some(self.modify(op_str,0) + " args"),
							kind: Some(lsp::CompletionItemKind::VALUE),
							insert_text: Some(self.modify(&snip,0)),
							insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
							..Default::default()
						});				
					}
				}
			}
		}
	}
	fn add_direct_index(&self, ans: &mut Vec<lsp::CompletionItem>, op_str: &str, proc: &ProcessorType) {
		if let Some(op) = self.op_book.get(op_str) {
			let mut results = HashSet::new();
			for mode in op.modes {
				if let Some(snip) = self.addr_mode_map.get(&mode.mnemonic) {
					if mode.processors.contains(proc) && snip.len()>0 &&
						(mode.mnemonic.ends_with(",x") || mode.mnemonic.ends_with(",y") || mode.mnemonic.ends_with(",s")) &&
						!mode.mnemonic.starts_with("(") && !mode.mnemonic.starts_with("[") {
						results.insert(self.modify(&mode.mnemonic[mode.mnemonic.len()-1..],0));
					}
				}
			}
			for res in results {
				ans.push(lsp::CompletionItem {
					label: res,
					detail: Some(self.modify(op_str,0) + " args"),
					kind: Some(lsp::CompletionItemKind::VALUE),
					..Default::default()
				});
			}
		}
	}
	fn add_psop_args(&self, ans: &mut Vec<lsp::CompletionItem>, psop_str: &str) -> usize {
		let starting_length = ans.len();
		if let Some(psop) = &self.psop_book.get(&psop_str) {
			for arg in &psop.choices {
				if arg.len() > 0 {
					let mut unsupported = false;
					if let Some(v8x) = &psop.v8x {
						if self.config.version == MerlinVersion::Merlin8 {
							unsupported |= v8x.is_match(arg);
						}
					}
					if let Some(v16x) = &psop.v16x {
						if self.config.version == MerlinVersion::Merlin16 {
							unsupported |= v16x.is_match(arg);
						}
					}
					if !unsupported {
						ans.push(lsp::CompletionItem {
							label: arg.to_string(),
							detail: Some(self.modify(psop_str,0) + " args"),
							kind: Some(lsp::CompletionItemKind::ENUM_MEMBER),
							insert_text: Some(self.modify(&arg,0)),
							insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
							..Default::default()
						});		
					}
				}
			}
		}
		ans.len() - starting_length
	}
	fn get(&mut self,line: &str, pos: &lsp::Position, trig: &Option<String>) -> Vec<lsp::CompletionItem> {
		let mut ans = Vec::new();
		let mut simple = Vec::new();
		let mut label = HashSet::new();
		let mut psop_args = 0;
		if line.starts_with("*") {
			return ans;
		}
		if pos.character as usize > line.len() {
			return Vec::new();
		}
		let line_prefix = &line[0..pos.character as usize];
		if self.c2_regex.is_match(line_prefix) {
			// start of column 2 is alpha
			simple.append(&mut self.op_book.completion(&self.symbols.processor));
			for psop in self.psop_book.completion(&self.config.version) {
				let tabs = !line_prefix.starts_with(" ") && !line_prefix.starts_with("\t");
				let arg_spc = match self.config.columns.c2 > psop.len() as i64 {
					true => " ".repeat(self.config.columns.c2 as usize - psop.len()),
					false => " ".to_string()
				};
				if psop == "mac" {
					self.add_snippet(&mut ans, "mac\nlines\n<<<", "mac\n${0:}\n<<<", tabs);
					self.add_snippet(&mut ans, "mac\nlines\neom", "mac\n${0:}\neom", tabs);
				} else if psop == "do" {
					self.add_snippet(&mut ans, &format!("do{}expr\nlines\nfin",arg_spc), &format!("do{}${{1:expr}}\n${{0:}}\nfin",arg_spc), tabs);
				} else if psop == "if" {
					self.add_snippet(&mut ans, &format!("if{}char=var\nlines\nfin",arg_spc), &format!("if{}${{1:char}}=${{2:var}}\n${{0:}}\nfin",arg_spc), tabs);
				} else if psop == "lup" {
					self.add_snippet(&mut ans, &format!("lup{}expr\nlines\n--^",arg_spc), &format!("lup{}${{1:expr}}\n${{0:}}\n--^",arg_spc), tabs);
				} else {
					simple.push(psop);
				}
			}
			for mac in self.symbols.macros.keys() {
				label.insert(mac.to_string());
			}
		}
		if self.loc_regex.is_match(line_prefix) {
			// this will suggest locals that have been used anywhere, not just cursor scope
			for glob in self.symbols.globals.values() {
				for loc in glob.children.keys() {
					label.insert(loc.to_string());
				}
			}
		}
		if self.var_regex.is_match(line_prefix) {
			// suggest any variable appearing in this symbol set
			for var in self.symbols.vars.keys() {
				label.insert(var.to_string());
			}
		}
		if self.c1_glob_regex.is_match(line_prefix) {
			// suggest any global appearing in this symbol set
			for glob in self.symbols.globals.keys() {
				label.insert(glob.to_string());
			}
		}
		if self.c3_arg_regex.is_match(line_prefix) {
			// suggest pseudo-op arguments based on what is in column 2
			if let Some(mtch) = self.c2_capture.find(line_prefix) {
				psop_args = self.add_psop_args(&mut ans, mtch.as_str());
			}
		}
		if psop_args==0 && self.c3_lab_regex.is_match(line_prefix) {
			// suggest a label reference, or, macro reference if PMC is in column 2
			if self.pmc_regex.is_match(line_prefix) {
				for mac in self.symbols.macros.keys() {
					label.insert(mac.to_string());
				}
			} else {
				for glob in self.symbols.globals.keys() {
					label.insert(glob.to_string());
				}
			}
		}
		if self.iaddr_regex.is_match(line_prefix) {
			if let Some(mtch) = self.c2_capture.find(line_prefix) {
				if let Some(trig_char) = trig {
					self.add_indirect_arg(&mut ans, mtch.as_str(), trig_char, &self.symbols.processor);
				}
			}
		}
		if self.daddr_regex.is_match(line_prefix) {
			if let Some(mtch) = self.c2_capture.find(line_prefix) {
				self.add_direct_index(&mut ans, mtch.as_str(), &self.symbols.processor);
			}
		}
		self.add_simple(&mut ans,&simple);
		self.add_label(&mut ans,&label);
		return ans;
	}
}

pub struct CompletionProvider {
	config: Settings,
	code_tool: CodeCompletionProvider,
	address_tool: AddressCompletionProvider
}

impl CompletionProvider {
	pub fn new() -> Self {
		Self {
			config: Settings::new(),
			code_tool: CodeCompletionProvider::new(),
			address_tool: AddressCompletionProvider::new()
		}
	}
    pub fn set_config(&mut self,config: Settings) {
		self.code_tool.set_config(config.clone());
		self.address_tool.set_config(config.clone());
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
		self.code_tool.use_shared_symbols(sym);
    }
} 

impl Completions for CompletionProvider {
	fn get(&mut self,lines: &mut Lines, ctx: &lsp::CompletionContext, pos: &lsp::Position) -> Result<Vec<lsp::CompletionItem>,String> {
		let mut ans = Vec::new();

		if let Some(curr) = lines.nth(pos.line as usize) {
			if ctx.trigger_kind==lsp::CompletionTriggerKind::INVOKED {
				ans.append(&mut self.code_tool.get(curr, pos, &ctx.trigger_character));
			}
			if let Some(trig) = &ctx.trigger_character {
				if trig.as_str() == "$" {
					ans.append(&mut self.address_tool.get(curr,pos.character as usize));
				}
			}
		}

		Ok(ans)
	}
}