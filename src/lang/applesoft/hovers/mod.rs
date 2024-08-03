use lsp_types as lsp;
use std::sync::Arc;
use tree_sitter;
use tree_sitter_applesoft;
use super::settings::Settings;
use super::Symbols;

use crate::lang::{Navigation,Navigate,lsp_range,range_contains_pos,node_integer};
use crate::lang::server::Hovers;
use crate::DYNERR;

mod hovers_statements;
mod hovers_addresses;

pub struct HoverProvider {
    config: Settings,
    parser: tree_sitter::Parser,
    line: String,
    markup: lsp::MarkupContent,
    pos: lsp::Position,
    rng: lsp::Range,
    symbols: Arc<Symbols>,
    addresses: hovers_addresses::AddressHovers,
    statements: hovers_statements::StatementHovers
}

/// check token node to see if it is overloaded within an ampersand
fn is_overloaded(curs: &tree_sitter::TreeCursor) -> bool {
    if let Some(parent) = curs.node().parent() {
        if let Some(first) = parent.named_child(0) {
            if first.kind() == "tok_amp" && curs.node().kind() != "tok_amp" {
                return true;
            }
        }
    }
    false
}

impl HoverProvider {
    pub fn new() -> Self {
        let zero = lsp::Position::new(0,0);
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_applesoft::language()).expect("could not start Applesoft parser");
        Self {
            config: Settings::new(),
            parser,
            line: String::new(),
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
}

impl Hovers for HoverProvider {
    fn get(&mut self, line: String, row: isize, col: isize) -> Option<lsp::Hover> {
        self.line = line + "\n";
        self.markup = lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "".to_string()
        };
        self.pos = lsp::Position::new(row as u32,col as u32);
        if let Some(tree) = self.parser.parse(&self.line,None) {
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
        self.rng = lsp_range(curs.node().range(),self.pos.line as isize,0);
        if range_contains_pos(&self.rng, &self.pos) {

            if self.config.hovers.special_addresses {
                if let Some(hov) = self.addresses.get_from_node(curs, &self.line) {
                    if self.markup.value.len() > 0 {
                        self.markup.value += "\n\n---\n\n";
                    }
                    self.markup.value += &hov;
                }
            }

            if curs.node().kind() == "linenum" {
                if let Some(num) = node_integer(&curs.node(),&self.line) {
                    if let Some(docstring) = self.symbols.docstring(num) {
                        if self.markup.value.len() > 0 {
                            self.markup.value += "\n\n---\n\n";
                        }
                        self.markup.value += &docstring.to_string();
                    }
                }
            }

            if curs.node().kind().starts_with("tok_") && self.config.hovers.keywords {
                if self.markup.value.len() > 0 {
                    self.markup.value += "\n\n---\n\n";
                }
                if is_overloaded(curs) {
                    self.markup.value += "overloaded token";
                } else if let Some(statement) = self.statements.get(curs.node().kind()) {
                    self.markup.value += &statement;
                }
            }

            if curs.node().kind() == "name_amp" && self.config.hovers.keywords {
                if self.markup.value.len() > 0 {
                    self.markup.value += "\n\n---\n\n";
                }
                if curs.node().named_child_count() == 0 {
                    self.markup.value += "custom command";
                } else {
                    self.markup.value += "overloaded token";
                }
            }

        }
        return Ok(Navigation::GotoChild);
    }
}
