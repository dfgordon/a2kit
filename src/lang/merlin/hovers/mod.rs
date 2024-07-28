use lsp_types as lsp;
use std::sync::Arc;
use super::settings::Settings;
use super::Symbols;

use crate::lang::{Navigation,Navigate,lsp_range,range_contains_pos,node_text};
use crate::lang::server::{Hovers,path_in_workspace};
use crate::DYNERR;

mod hovers_statements;
mod hovers_addresses;

pub struct HoverProvider {
    config: Settings,
    ws_folder: Vec<lsp::Url>,
    parser: super::MerlinParser,
    markup: lsp::MarkupContent,
    pos: lsp::Position,
    rng: lsp::Range,
    symbols: Arc<Symbols>,
    addresses: hovers_addresses::AddressHovers,
    statements: hovers_statements::StatementHovers
}

fn new_section(hov: &mut String, sec: &str) {
    if hov.len() > 0 && sec.len() > 0 {
        *hov += "\n\n---\n\n"
    }
    *hov += sec;
}

impl HoverProvider {
    pub fn new() -> Self {
        let zero = lsp::Position::new(0,0);
        Self {
            config: Settings::new(),
            ws_folder: Vec::new(),
            parser: super::MerlinParser::new(),
            markup: lsp::MarkupContent {
                kind: lsp::MarkupKind::Markdown,
                value: "".to_string()
            },
            pos: zero.clone(),
            rng: lsp::Range::new(zero.clone(),zero),
            symbols: Arc::new(Symbols::new()),
            addresses: hovers_addresses::AddressHovers::new(),
            statements: hovers_statements::StatementHovers::new()
        }
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn use_shared_symbols(&mut self,sym: Arc<Symbols>) {
        self.symbols = sym;
    }
    pub fn set_workspace_folder(&mut self,uri: Vec<lsp::Url>) {
        self.ws_folder = uri;
    }
}

impl Hovers for HoverProvider {
    fn get(&mut self, line: String, row: isize, raw_col: isize) -> Option<lsp::Hover> {
        self.markup = lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "".to_string()
        };
        self.pos = lsp::Position::new(row as u32,raw_col as u32);
        if let Ok(tree) = self.parser.parse(&line,&self.symbols) {
            if let Ok(()) = self.walk(&tree) {
                if self.markup.value.len() > 0 {
                    return Some(lsp::Hover {
                        contents: lsp::HoverContents::Markup(self.markup.clone()),
                        range: Some(self.rng.clone())
                    });
                }
            }
        }
        None
    }
}

impl Navigate for HoverProvider {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
        self.rng = lsp_range(curs.node().range(),self.pos.line as isize,self.parser.col_offset());
        let txt = node_text(&curs.node(),self.parser.line());
        if range_contains_pos(&self.rng, &self.pos) {

            if self.config.hovers.special_addresses {
                if let Some(hov) = self.addresses.get_from_node(curs, self.parser.line()) {
                    self.markup.value += &hov;
					return Ok(Navigation::Exit);
                }
            }

			if self.config.hovers.mnemonics {
                if let Some(hov) = self.statements.get_op(&curs.node().kind()) {
                    self.markup.value += &hov;
                    return Ok(Navigation::Exit);
                }
			}
			if self.config.hovers.pseudo {
                if let Some(hov) = self.statements.get_psop(&curs.node().kind()) {
                    self.markup.value += &hov;
                    return Ok(Navigation::Exit);
                }
			}
			if curs.node().kind()=="dstring" {
				if txt.as_bytes()[0] < "'".as_bytes()[0] {
					self.markup.value += "negative ASCII dstring";
                }
				else {
					self.markup.value += "positive ASCII dstring";
                }
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "nchar" {
				self.markup.value += "negative ASCII character";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "pchar" {
				self.markup.value += "positive ASCII character";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "data_prefix" {
				self.markup.value += 
"Data prefix, use of `#` is optional.\n\n\
The bit selection window can be shifted left as follows.\n\n
`<` = shift by 0, `>` = shift by 8, `^` = shift by 16.\n\n
The specific pseudo-op determines width and order.";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "imm_prefix" {
				self.markup.value += 
"Immediate mode, the operand is a number, not an address.\n\n\
The bit selection window can be shifted left as follows.\n\n
`#<` = shift by 0, `#>` = shift by 8, `#^` = shift by 16.\n\n
The width of the window is 8 or 16 depending on processor mode.";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "addr_prefix" {
				self.markup.value += "address modifier, `>` = forced 24 bit, `|` = forced 16 bit";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "num_str_prefix" {
				self.markup.value += 
"number prefix, the expression's value is converted to text\n\n\
`#` or `#'` = positive ASCII, `#\"` = negative ASCII\n\n\
add `>` to right justify in 5 column field, e.g. `#'>`";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "macro_ref" {
                if let Some(expansion) = super::diagnostics::macros::expand_macro(&curs.node(), &self.parser.line(), &mut self.symbols, 1) {
                    new_section(&mut self.markup.value, &&["```\n",&expansion,"```"].concat());
                } else {
                    self.markup.value += "unable to expand macro";
                }
                if let Some(sym) = self.symbols.macros.get(&txt) {
                    if sym.defs.len() > 0 && sym.defs[0].uri.to_string()!=self.symbols.display_doc_uri {
                        new_section(&mut self.markup.value,&format!("imported from {}",path_in_workspace(&sym.defs[0].uri,&self.ws_folder)));
                    }
                }
                return Ok(Navigation::Exit);
			}
			if curs.node().kind()=="label_ref" {
                if let Some(sym) = self.symbols.globals.get(&txt) {
                    if let Some(num) = sym.value {
                        self.markup.value += &self.addresses.get(num).or(Some("".to_string())).unwrap();
                    }
                    if sym.defs.len()>0 {
                        if let Some(code) = &sym.defining_code {
                            new_section(&mut self.markup.value, &["```\n",code,"```\n"].concat());
                        }
                        new_section(&mut self.markup.value, &sym.docstring);
                        if sym.defs[0].uri.to_string()!=self.symbols.display_doc_uri {
                            new_section(&mut self.markup.value,&format!("imported from {}",path_in_workspace(&sym.defs[0].uri,&self.ws_folder)));
                        }
                    }
                }
                for (name,sym) in &self.symbols.macros {
                    if sym.children.contains_key(name) {
                        self.markup.value += "scoped to macro ";
                        self.markup.value += name;
                        return Ok(Navigation::Exit);
                    }
                }
				return Ok(Navigation::Exit);
			}
			if curs.node().kind() == "macro_def" {
				self.markup.value += "macro defined right here";
				return Ok(Navigation::Exit);
			}
			if curs.node().kind()=="label_def"
			{
				if let Some(inner) = curs.node().named_child(0) {
                    if inner.kind() == "local_label" {
                        self.markup.value += "local defined right here";
                        return Ok(Navigation::Exit);
                    } else if inner.kind() == "var_label" {
                        self.markup.value += "variable defined right here";
                        return Ok(Navigation::Exit);
                    } else if inner.kind() == "global_label" {
                        for (name,parent) in &self.symbols.macros {
                            if let Some(child) = parent.children.get(&txt) {
                                for def in &child.defs {
                                    if def.range.start.line == self.pos.line && def.uri.to_string() == self.symbols.display_doc_uri {
                                        self.markup.value += "scoped to macro ";
                                        self.markup.value += name;
                                        return Ok(Navigation::Exit);
                                    }
                                }
                            }
                        }
                        if let Some(sym) = self.symbols.globals.get(&txt) {
                            if sym.flags & super::symbol_flags::EXT > 0 {
                                new_section(&mut self.markup.value,&sym.docstring);
                            } else {
                                new_section(&mut self.markup.value,"global defined right here");
                            }
                        } else {
                            self.markup.value += "symbol information missing";
                        }
                    }
                }
				return Ok(Navigation::Exit);
			}
			return Ok(Navigation::GotoChild);

        }
        return Ok(Navigation::GotoChild);
    }
}
