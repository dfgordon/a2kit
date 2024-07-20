use std::sync::Arc;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use super::super::{Symbol,Symbols,Workspace,MerlinVersion,symbol_flags,ProcessorType,SourceType};
use super::super::settings::Settings;
use crate::lang::merlin::handbook::operations::OperationHandbook;
use crate::lang::merlin::handbook::pseudo_ops::PseudoOperationHandbook;
use crate::lang::{Document,node_text,lsp_range};

#[derive(Clone)]
pub struct Source {
    pub row: isize,
    pub col: isize,
    pub typ: SourceType,
    pub doc: Arc<Document>,
    pub line: String
}

pub struct Context {
    config: Settings,
    op_book: OperationHandbook,
    psop_book: PseudoOperationHandbook,
    xc_count: usize,
    /// stack of symbols representing the current scope
    symbol_stack: Vec<Symbol>,
    /// stack of document info for include descents
    source_stack: Vec<Source>,
    /// built and consumed/cleared as lines are processed
    pub running_docstring: String
}

impl Source {
    fn new(typ: SourceType,doc: Arc<Document>) -> Self {
        Self {
            row: 0,
            col: 0,
            typ,
            doc,
            line: String::new()
        }
    }
}

impl Context {
    pub fn new() -> Self {
        log::debug!("create context");
        Self {
            config: Settings::new(),
            op_book: OperationHandbook::new(),
            psop_book: PseudoOperationHandbook::new(),
            xc_count: 0,
            symbol_stack: Vec::new(),
            source_stack: Vec::new(),
            running_docstring: String::new()
        }
    }
    pub fn reset_xc(&mut self) {
        self.xc_count = match self.config.version {
            MerlinVersion::Merlin8 => 0,
            _ => 2
        };
    }
    pub fn reset_for_pass(&mut self) {
        self.symbol_stack = Vec::new();
        self.source_stack = Vec::new();
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn get_xc(&self) -> usize {
        self.xc_count
    }
    pub fn set_xc(&mut self,xc_count: usize) {
        self.xc_count = xc_count;
    }
    /// borrow the processor oepration handbook
    pub fn op_handbook(&self) -> &OperationHandbook {
        &self.op_book
    }
    /// borrow the Merlin pseudo-operation handbook
    pub fn psop_handbook(&self) -> &PseudoOperationHandbook {
        &self.psop_book
    }
    /// Borrow the line currently being analyzed.
    /// The line may start with a parsing hint prefix.
    /// Panics if source_stack is empty.
    pub fn line(&self) -> &str {
        &self.source_stack.last().unwrap().line
    }
    /// Set the line currently being analyzed, `term` can be empty.
    /// Do not strip the parsing hint prefix.
    /// Panics if source_stack is empty.
    pub fn set_line(&mut self, line: String, term: &str) {
        self.source_stack.last_mut().unwrap().line = [line,term.to_string()].concat()
    }
    /// frequently used node data (range,text), panics if source_stack is empty
    pub fn node_spec(&self,node: &tree_sitter::Node) -> (lsp::Range,String) {
        (lsp_range(node.range(),self.row(),self.col()) , node_text(node,&self.line()))
    }
    /// case insensitive match to text, panics if source_stack is empty
    pub fn node_match(&self,node: &tree_sitter::Node,test: &str) -> bool {
        node_text(node,&self.line()).to_uppercase() == test.to_uppercase()
    }
    /// row in the current source string
    pub fn row(&self) -> isize {
        match self.source_stack.last() {
            Some(src) => src.row,
            None => 0
        }
    }
    /// column in the current source string, usually 0, but can be -1
    /// for a line that begins with a parser hint.
    pub fn col(&self) -> isize {
        match self.source_stack.last() {
            Some(src) => src.col,
            None => 0
        }
    }
    /// get parameters of the current source string
    pub fn curr_source(&self) -> Option<Source> {
        self.source_stack.last().cloned()
    }
    /// push information about a source string onto the source stack
    pub fn enter_source(&mut self,typ: SourceType,doc: Arc<Document>) {
        self.source_stack.push(Source::new(typ,doc));
    }
    /// return to the previous source string, restoring parameters
    pub fn exit_source(&mut self) -> Option<Source> {
        self.source_stack.pop()
    }
    /// advance the row in the current source strings
    pub fn next_row(&mut self) {
        if let Some(src) = self.source_stack.last_mut() {
            src.row += 1;
        }
    }
    /// adjust the column (0 for normal line, -2 for adjusted line, assuming LSP default position encoding)
    pub fn set_col(&mut self,new_offset: isize) {
        if let Some(src) = self.source_stack.last_mut() {
            src.col = new_offset;
        }
    }
    /// borrow symbol defining current scope
    pub fn curr_scope(&mut self) -> Option<&mut Symbol> {
        match self.symbol_stack.last_mut() {
            Some(sym) => Some(sym),
            None => None
        }
    }
    fn merge_macro(symbols: &mut Symbols,popped: Symbol) {
        if let Some(main) = symbols.macros.get_mut(&popped.name) {
            main.defining_code = popped.defining_code.clone();
            for (child_txt,child_sym) in popped.children {
                main.children.insert(child_txt,child_sym);
            }
        }
    }
    fn merge_global(symbols: &mut Symbols,popped: Symbol) {
        if let Some(main) = symbols.globals.get_mut(&popped.name) {
            for (child_txt,child_sym) in popped.children {
                main.children.insert(child_txt,child_sym);
            }
        }
    }
    /// Push the current line onto all current macro scopes.
    pub fn push_macro_line(&mut self) {
        let line = self.line().to_owned();
        for sym in self.symbol_stack.iter_mut().rev() {
            if sym.flags & symbol_flags::MAC == 0 {
                return;
            }
            if let Some(code) = &mut sym.defining_code {
                *code += &line;
            } else {
                sym.defining_code = Some(line.clone())
            }
        }
    }
    /// Push clone of symbol onto the scope stack.
    /// Symbol is only pushed if it is a macro or non-external global.
    /// If we are pushing a global, this function will call `exit_scope` to get out of the
    /// previous global scope, if necessary.
    /// In Merlin, global scopes are never nested, and cannot exist within a macro scope.
    pub fn enter_scope(&mut self,name: &str,symbols: &mut Symbols) {
        let maybe_global = symbols.globals.get(name);
        if let Some(glob) = maybe_global {
            if glob.flags & symbol_flags::EXT > 0 {
                return;
            }
        }
        log::trace!("enter scope {}",name);
        let needs_exit = match (maybe_global.is_some(),self.symbol_stack.last()) {
            (true,Some(test)) => test.flags & symbol_flags::MAC == 0,
            _ => false
        };
        if needs_exit {
            self.exit_scope(symbols);
        }
        if let Some(sym) = symbols.globals.get(name) {
            self.symbol_stack.push(sym.clone());
        } else if let Some(sym) = symbols.macros.get(name) {
            self.symbol_stack.push(sym.clone());
        }
    }
    /// Remove symbols from the scope stack, and merge changes with main store.
    /// In Merlin, EOM exits all prior macro scopes, so this can pop multiple scopes at once.
    pub fn exit_scope(&mut self,symbols: &mut Symbols) {
        if let Some(sym) = self.symbol_stack.pop() {
            log::trace!("exit scope {}",&sym.name);
            let is_macro = sym.flags & symbol_flags::MAC > 0;
            if is_macro {
                Self::merge_macro(symbols,sym);
                while let Some(test) = self.symbol_stack.last() {
                    if test.flags & symbol_flags::MAC > 0 {
                        if let Some(curr) = self.symbol_stack.pop() {
                            Self::merge_macro(symbols, curr);
                        }
                    } else {
                        break;
                    }
                }
            } else {
                Self::merge_global(symbols, sym);
            }
        }
    }
    pub fn curr_proc(&self) -> ProcessorType {
        match self.xc_count {
            0 => ProcessorType::_6502,
            1 => ProcessorType::_65c02,
            _ => match self.config.version {
                MerlinVersion::Merlin8 => ProcessorType::_65802,
                _ => ProcessorType::_65c816
            }
        }
    }
    pub fn merlin_version(&self) -> MerlinVersion {
        self.config.version.clone()
    }
    pub fn case_sensitivity(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.case_sensitive
    }
    pub fn linker_threshold(&self) -> f64 {
        self.config.linker.detect
    }
	/// Helper for descent callbacks
	/// * param `curs` expected to be on a PUT or USE pseudo-op node
	/// * returns (source type,document) to descend into
    pub fn prepare_to_descend(&mut self, curs: &TreeCursor, ws: &Workspace) -> Option<(SourceType,Arc<Document>)> {
		if self.source_stack.len() != 1 {
            log::debug!("do not descend, recursive");
			return None;
		}
		let mut new_typ = SourceType::Master;
		if curs.node().kind() == "psop_put" {
			new_typ = SourceType::Put;
        }
		if curs.node().kind() == "psop_use" {
			new_typ = SourceType::Use;
        }
		if !["psop_put", "psop_use"].contains(&curs.node().kind()) {
            log::debug!("do not descend, wrong node type {}",curs.node().kind());
			return None;
		}
		if let Some(file_node) = curs.node().next_named_sibling() {
            let mut matches = Vec::new();
            for doc in &ws.docs {
                if super::node_matches_doc(&file_node, &self.line(), &doc) {
                    matches.push(doc);
                }
            }
            if matches.len() == 0 {
                log::debug!("do not descend, no match for {}",node_text(&file_node,&self.line()));
                return None;
            }
            return Some((new_typ,Arc::new(matches[0].to_owned())));
        }
        return None;
	}
}
