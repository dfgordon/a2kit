//! Integer BASIC checkpoint module.
//! 
//! Provides data to an LSP client while the analyzer runs in another thread

use std::collections::HashMap;
use std::sync::Arc;
use lsp_types as lsp;
use crate::lang::{Document,range_contains_pos};
use super::{Variable,Line,Symbols};
use crate::lang::server::Checkpoint;

pub struct CheckpointManager {
    doc: Document,
    symbols: Arc<Symbols>
}

/// Simple linear search of map values
fn find_clicked_in_map(map: &HashMap<String,Variable>,sel_loc: &lsp::Location) -> Option<Variable> {
    for v in map.values() {
        for rng in &v.decs {
            if range_contains_pos(rng, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        for rng in &v.defs {
            if range_contains_pos(rng, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        for rng in &v.refs {
            if range_contains_pos(rng, &sel_loc.range.start) {
                return Some(v.clone());
            }
        }
    }
    None
}

fn find_clicked(symbols: &Arc<Symbols>,sel_loc: &lsp::Location) -> Option<Variable> {
    if let Some(sym) = find_clicked_in_map(&symbols.vars, sel_loc) {
        return Some(sym);
    }
    None
}

fn find_clicked_linenum(map: &HashMap<i64,Line>,sel_loc: &lsp::Location) -> Option<Line> {
    for v in map.values() {
        if range_contains_pos(&v.primary,&sel_loc.range.start) {
            return Some(v.clone());
        }
        for rng in &v.gosubs {
            if range_contains_pos(rng,&sel_loc.range.start) {
                return Some(v.clone());
            }
        }
        for rng in &v.gotos {
            if range_contains_pos(rng,&sel_loc.range.start) {
                return Some(v.clone());
            }
        }
    }
    None
}

#[allow(deprecated)]
fn create_symbol(name: String,detail: Option<String>,kind: lsp::SymbolKind,rng: &lsp::Range) -> lsp::DocumentSymbol {
    lsp::DocumentSymbol {
        name: name.to_string(),
        detail,
        kind,
        range: rng.clone(),
        selection_range: rng.clone(),
        tags: None,
        children: None,
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
        Vec::new()
    }
    fn get_symbols(&self) -> Vec<lsp::DocumentSymbol> {
        let sym = &self.symbols;
        let mut ans = Vec::new();
        for (num,line) in &sym.lines {
            if line.gosubs.len() > 0 {
                ans.push(create_symbol(num.to_string(), line.rem.clone(), lsp::SymbolKind::FUNCTION, &line.primary));
            } else if line.gotos.len() > 0 {
                ans.push(create_symbol(num.to_string(), line.rem.clone(), lsp::SymbolKind::CONSTANT, &line.primary));
            }
        }
        for (name,vars) in &sym.vars {
            // TODO: without this guard client may balk, but how do we end up with null names anyway?
            if name.len()==0 {
                break;
            }
            for rng in &vars.decs {
                if vars.is_string {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::STRING, rng));
                } else if vars.is_array {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::ARRAY, rng));
                }
            }
            for rng in &vars.defs {
                if vars.is_string {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::STRING, rng));
                } else if vars.is_array {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::ARRAY, rng));
                } else {
                    ans.push(create_symbol(name.to_string(), None, lsp::SymbolKind::VARIABLE, rng));
                }
            }
        }
        ans
    }
    fn get_decs(&self,loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(var) = find_clicked(&self.symbols, loc) {
            return var.decs.iter().map(|r| lsp::Location::new(loc.uri.clone(),r.clone())).collect();
        }
        vec![]
    }
    fn get_defs(&self,loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(line) = find_clicked_linenum(&self.symbols.lines, loc) {
            return vec![lsp::Location::new(loc.uri.clone(),line.primary.clone())];
        }
        if let Some(var) = find_clicked(&self.symbols, loc) {
            return var.defs.iter().map(|r| lsp::Location::new(loc.uri.clone(),r.clone())).collect();
        }
        vec![]
    }
    fn get_refs(&self,loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(line) = find_clicked_linenum(&self.symbols.lines, loc) {
            return [line.gotos.clone(),line.gosubs.clone()].concat().iter().map(|r| lsp::Location::new(loc.uri.clone(),r.clone())).collect();
        }
        if let Some(var) = find_clicked(&self.symbols, loc) {
            return var.refs.iter().map(|r| lsp::Location::new(loc.uri.clone(),r.clone())).collect();
        }
        vec![]
    }
    fn get_renamables(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        if let Some(sym) = find_clicked(&self.symbols, sel_loc) {
            let rngs = [sym.decs.clone(),sym.defs.clone(),sym.refs.clone()].concat();
            return rngs.iter().map(|r| lsp::Location::new(sel_loc.uri.clone(),r.clone())).collect();
        }
        Vec::new()
    }
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            doc: Document::from_string("".to_string(),0),
            symbols: Arc::new(Symbols::new())
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
    pub fn shared_symbols(&self) -> Arc<Symbols> {
        Arc::clone(&self.symbols)
    }
}