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
    symbols: Arc<Symbols>
}

fn goto_defs(ans: &mut Vec<lsp::Location>,sel_loc: &lsp::Location,refs: &Vec<lsp::Location>,defs: &Vec<lsp::Location>) -> bool {
    for ref_loc in refs {
        if ref_loc.uri == sel_loc.uri {
            if range_contains_pos(&ref_loc.range, &sel_loc.range.start) {
                for def_loc in defs {
                    ans.push(def_loc.clone());
                }
                return true; // found it
            }
        }
    }
    false
}

fn goto_refs(map: &HashMap<String,Symbol>, sel_loc: &lsp::Location) -> Option<Vec<lsp::Location>> {
	for sym in map.values() {
		let mut ans = Vec::new();
		let mut clicked = false;
        // information can be built uselessly many times, but here it isn't too important
        for ref_loc in &sym.refs {
			ans.push(ref_loc.clone());
			clicked = clicked || (ref_loc.uri == sel_loc.uri && range_contains_pos(&ref_loc.range, &sel_loc.range.start));
		}
		if clicked {
			return Some(ans);
        }
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
        let mut ans = Vec::new();
        let syms = &self.symbols;
        for sym in syms.globals.values() {
            if goto_defs(&mut ans, sel_loc, &sym.refs, &sym.decs) {
                return ans;
            }
        }
        ans
    }
    fn get_defs(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        let mut ans = Vec::new();
        let sym = &self.symbols;
        for globals in sym.globals.values() {
            if goto_defs(&mut ans, sel_loc, &globals.refs, &globals.defs) {
                return ans;
            }
            for children in globals.children.values() {
                if goto_defs(&mut ans, sel_loc, &children.refs, &children.defs) {
                    return ans;
                }
            }
        }
        for macros in sym.macros.values() {
            if goto_defs(&mut ans, sel_loc, &macros.refs, &macros.defs) {
                return ans;
            }
            for children in macros.children.values() {
                if goto_defs(&mut ans, sel_loc, &children.refs, &children.defs) {
                    return ans;
                }
            }
        }
        for vars in sym.vars.values() {
            if goto_defs(&mut ans, sel_loc, &vars.refs, &vars.defs) {
                return ans;
            }
        }
        ans
    }
    fn get_refs(&self,sel_loc: &lsp::Location) -> Vec<lsp::Location> {
        let syms = &self.symbols;
        if let Some(ans) = goto_refs(&syms.globals, sel_loc) {
            return ans;
        }
        if let Some(ans) = goto_refs(&syms.macros, sel_loc) {
            return ans;
        }
        if let Some(ans) = goto_refs(&syms.vars, sel_loc) {
            return ans;
        }
        for sym in syms.globals.values() {
            if let Some(ans) = goto_refs(&sym.children, sel_loc) {
                return ans;
            }
        }
        for sym in syms.macros.values() {
            if let Some(ans) = goto_refs(&sym.children, sel_loc) {
                return ans;
            }
        }
        vec![]
    }
    fn get_renamables(&self,sel_loc: &lsp_types::Location) -> Vec<lsp_types::Location> {
        self.get_refs(sel_loc)
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