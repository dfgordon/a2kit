//! Merlin checkpoint module.
//! 
//! Provides data to an LSP client while the analyzer runs in another thread

use std::collections::HashMap;
use std::sync::Arc;
use lsp_types as lsp;
use crate::lang::range_contains_pos;
use crate::lang::Document;
use super::{Symbol,Symbols};
use crate::lang::server::Checkpoint;

pub struct CheckpointManager {
    doc: Document,
    symbols: Arc<Symbols>,
    ws_symbols: Arc<HashMap<String,Symbol>>,
    folding_ranges: Vec<lsp::FoldingRange>
}

/// Simple linear search of map values, may recursively call itself
fn find_clicked_in_map(map: &HashMap<String,Symbol>,sel_loc: &lsp::Location) -> Option<Symbol> {
    for v in map.values() {
        for loc in &v.decs {
            if loc.uri == sel_loc.uri && range_contains_pos(&loc.range, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        for loc in &v.defs {
            if loc.uri == sel_loc.uri && range_contains_pos(&loc.range, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        for loc in &v.refs {
            if loc.uri == sel_loc.uri && range_contains_pos(&loc.range, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        if let Some(sym) = find_clicked_in_map(&v.children, sel_loc) {
            return Some(sym)
        }
    }
    None
}

fn find_clicked(symbols: &Arc<Symbols>,sel_loc: &lsp::Location) -> Option<Symbol> {
    if let Some(sym) = find_clicked_in_map(&symbols.globals, sel_loc) {
        return Some(sym);
    }
    if let Some(sym) = find_clicked_in_map(&symbols.macros, sel_loc) {
        return Some(sym);
    }
    if let Some(sym) = find_clicked_in_map(&symbols.vars, sel_loc) {
        return Some(sym);
    }
    None
}

#[allow(deprecated)]
fn create_symbol(name: String,detail: Option<String>,kind: lsp::SymbolKind,rng: &lsp::Range,children: Option<Vec<lsp::DocumentSymbol>>) -> lsp::DocumentSymbol {
    lsp::DocumentSymbol {
        name: name.to_string(),
        detail,
        kind,
        range: rng.clone(),
        selection_range: rng.clone(),
        tags: None,
        children,
        deprecated: None
    }
}

impl Checkpoint for CheckpointManager {
    fn get_doc(&self) -> Document {
        self.doc.clone() 
    }
    fn get_line(&self,row: usize) -> Option<String> {
        let mut row_now = 0;
        let mut lines = self.doc.text.lines();
        while let Some(line) = lines.next() {
            if row_now==row {
                return Some(line.to_string());
            }
            row_now += 1;
        }
        None
    }
    fn get_folding_ranges(&self) -> Vec<lsp_types::FoldingRange> {
        self.folding_ranges.clone()
    }
    fn get_symbols(&self) -> Vec<lsp::DocumentSymbol> {
        let sym = &self.symbols;
        let mut ans = Vec::new();
        for (name,sym) in &sym.globals {
            for loc in &sym.defs {
                if loc.uri.to_string() == self.symbols.display_doc_uri {
                    let mut children = Vec::new();
                    let mut detail: Option<String> = None;
                    if sym.flags & super::symbol_flags::ENT > 0 {
                        detail = Some("entry".to_string());
                    }
                    if sym.flags & super::symbol_flags::EXT > 0 {
                        detail = Some("external".to_string());
                    }
                    for (child_name,child) in &sym.children {
                        for loc_loc in &child.defs {
                            if loc_loc.uri.to_string() == self.symbols.display_doc_uri {
                                children.push(create_symbol(child_name.to_owned(),Some("local".to_string()),lsp::SymbolKind::CONSTANT,&loc_loc.range,None));
                            }
                        }
                    }
                    if sym.flags & super::symbol_flags::SUB > 0 {
                        ans.push(create_symbol(name.to_string(), detail, lsp::SymbolKind::FUNCTION, &loc.range,Some(children)));
                    } else {
                        ans.push(create_symbol(name.to_string(), detail, lsp::SymbolKind::CONSTANT, &loc.range,Some(children)));
                    }
                }
            }
        }
        for (name,sym) in &sym.macros {
            for loc in &sym.defs {
                if loc.uri.to_string() == self.symbols.display_doc_uri {
                    let mut children = Vec::new();
                    for (child_name,child) in &sym.children {
                        for loc_loc in &child.defs {
                            if loc_loc.uri.to_string() == self.symbols.display_doc_uri {
                                children.push(create_symbol(child_name.to_owned(),Some("local".to_string()),lsp::SymbolKind::CONSTANT,&loc_loc.range,None));
                            }
                        }
                    }
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::FUNCTION, &loc.range,Some(children)));
                }
            }
        }
        for (name,sym) in &sym.vars {
            for loc in &sym.defs {
                if loc.uri.to_string() == self.symbols.display_doc_uri {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::VARIABLE, &loc.range,None));
                }
            }
        }
        ans
    }
    fn get_decs(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(sym) = find_clicked(&self.symbols, sel_loc) {
            return sym.decs.clone();
        }
        Vec::new()
    }
    fn get_defs(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(sym) = find_clicked(&self.symbols, sel_loc) {
            return sym.defs.clone();
        }
        Vec::new()
    }
    fn get_refs(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(sym) = find_clicked(&self.symbols, sel_loc) {
            let mut ans = Vec::new();
            if sym.flags & super::symbol_flags::ENT > 0 {
                if let Some(wsym) = self.ws_symbols.get(&sym.name) {
                    ans.append(&mut wsym.refs.clone());
                }
            }
            ans.append(&mut sym.refs.clone());
            return ans;
        }
        Vec::new()
    }
    fn get_renamables(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(sym) = find_clicked(&self.symbols, sel_loc) {
            return [sym.decs.clone(),sym.defs.clone(),sym.refs.clone()].concat();
        }
        Vec::new()
    }
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            doc: Document::from_string("".to_string(),0),
            symbols: Arc::new(Symbols::new()),
            ws_symbols: Arc::new(HashMap::new()),
            folding_ranges: Vec::new()
        }
    }
    pub fn update_doc(&mut self,uri: lsp::Uri, txt: String, version: Option<i32>) {
        self.doc.uri = uri;
        self.doc.text = txt;
        self.doc.version = version;
    }
    pub fn update_symbols(&mut self,sym: Symbols) {
        self.symbols = Arc::new(sym);
    }
    pub fn update_ws_symbols(&mut self,wsym: HashMap<String,Symbol>) {
        self.ws_symbols = Arc::new(wsym);
    }
    pub fn update_folding_ranges(&mut self,folding_ranges: Vec<lsp::FoldingRange>) {
        self.folding_ranges = folding_ranges;
    }
    pub fn shared_symbols(&self) -> Arc<Symbols> {
        Arc::clone(&self.symbols)
    }
    pub fn shared_ws_symbols(&self) -> Arc<HashMap<String,Symbol>> {
        Arc::clone(&self.ws_symbols)
    }
}