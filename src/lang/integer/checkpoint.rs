//! Integer BASIC checkpoint module.
//! 
//! Provides data to an LSP client while the analyzer runs in another thread

use std::collections::HashMap;
use std::sync::Arc;
use lsp_types as lsp;
use crate::lang::{Document,range_contains_pos};
use super::{Variable,Symbols};
use crate::lang::server::Checkpoint;

pub struct CheckpointManager {
    doc: Document,
    symbols: Arc<Symbols>
}

fn goto_defs(ans: &mut Vec<lsp::Location>,loc: &lsp::Location,refs: &Vec<lsp::Range>,defs: &Vec<lsp::Range>) -> bool {
    for rng in refs {
        if range_contains_pos(rng, &loc.range.start) {
            for def_rng in defs {
                ans.push(lsp::Location::new(loc.uri.clone(),def_rng.clone()));
            }
            return true; // found it
        }
    }
    false
}

fn goto_refs(map: &HashMap<String,Variable>, loc: &lsp::Location) -> Option<Vec<lsp::Location>> {
	for vars in map.values() {
		let mut ans = Vec::new();
		let mut clicked = false;
        // information can be built uselessly many times, but here it isn't too important
        for rng in &vars.refs {
			ans.push(lsp::Location::new(loc.uri.clone(), rng.clone()));
			clicked = clicked || range_contains_pos(rng, &loc.range.start);
		}
		if clicked {
			return Some(ans);
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
        let mut ans = Vec::new();
        let sym = &self.symbols;
        for vars in sym.vars.values() {
            if goto_defs(&mut ans, loc, &vars.refs, &vars.decs) {
                return ans;
            }
        }
        ans
    }
    fn get_defs(&self,loc: &lsp::Location) -> Vec<lsp::Location> {
        let mut ans = Vec::new();
        let sym = &self.symbols;
        for line in sym.lines.values() {
            let combined = [line.gotos.clone(),line.gosubs.clone()].concat();
            for rng in &combined {
                if range_contains_pos(rng, &loc.range.start) {
                    return vec![lsp::Location::new(loc.uri.clone(),line.primary.clone())];
                }
            }
        }
        for vars in sym.vars.values() {
            if goto_defs(&mut ans, loc, &vars.refs, &vars.defs) {
                return ans;
            }
        }
        ans
    }
    fn get_refs(&self,loc: &lsp::Location) -> Vec<lsp::Location> {
        let sym = &self.symbols;
        for line in sym.lines.values() {
            let mut ans = Vec::new();
            let mut clicked = false;
            let combined = [line.gotos.clone(),line.gosubs.clone(),vec![line.primary]].concat();
            // information can be built uselessly many times, but here it isn't too important
            for rng in &combined {
                ans.push(lsp::Location::new(loc.uri.clone(),rng.clone()));
                clicked = clicked || range_contains_pos(rng, &loc.range.start);
            }
            if clicked {
                return ans;
            }
        }
        if let Some(ans) = goto_refs(&sym.vars, loc) {
            return ans;
        }
        vec![]
    }
    fn get_renamables(&self,loc: &lsp_types::Location) -> Vec<lsp_types::Location> {
        let sym = &self.symbols;
        if let Some(ans) = goto_refs(&sym.vars, loc) {
            return ans;
        }
        vec![]
    }
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            doc: Document::from_string("".to_string(),0),
            symbols: Arc::new(Symbols::new())
        }
    }
    pub fn update_doc(&mut self,uri: lsp::Url, txt: String, version: Option<i32>) {
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