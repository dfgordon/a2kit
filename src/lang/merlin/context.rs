//! Merlin Context module used for analysis and assembly
//! 
//! This module manages containment relationships, builds docstrings, and owns a copy of the
//! assembler handbook.  There are three containment stacks:
//! 
//! 1. Scope stack - stack of `Symbol` structures, globals and macros define a scope
//! 2. Source stack - stack of `Source` structures, such as (Master (Use) (Put) (Put))
//! 3. Folding stack - stack of `Fold` structures, such as (DO (IF (ELSE (LUP))))
//! 
//! In Merlin these are allowed to interleave, e.g., a fold could start in a PUT file
//! and end in the master file.  The LSP rightly forbids this (display would be confusing),
//! so such folds are not reported to the client, even though they are calculated by the server.
//! 
//! The server puts macros on both the scope stack and the fold stack.  As a result, interleaving
//! macro definitions and conditional assembly will be flagged as an error:
//! 
//! ```asm
//! MYMAC   MAC
//!         DO 0
//!         EOM ; fold error
//!         FIN
//! ```
//! 
//! In contrast, Merlin will accept this and process it literally.

use std::sync::Arc;
use std::collections::HashMap;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use crate::lang::merlin::{Symbol,Symbols,Workspace,MerlinVersion,symbol_flags,ProcessorType,SourceType};
use crate::lang::merlin::settings::Settings;
use crate::lang::merlin::handbook::operations::OperationHandbook;
use crate::lang::merlin::handbook::pseudo_ops::PseudoOperationHandbook;
use crate::lang::{Document,node_text,lsp_range};
use crate::lang::server::basic_diag;

/// Actions to be applied before processing the next line.
pub struct Triggers {
    pub unset_children: bool,
    pub unset_vars: bool,
    pub push_vars: bool,
    pub pop_vars: bool,
    pub checkpoint_vars: bool,
    pub unset_pc: bool,
    pub set_pc: Option<usize>,
    pub advance_pc: Option<usize>,
    pub push_pc: bool,
    pub pop_pc: bool
}

#[derive(Clone)]
pub struct Fold {
    /// syntax node kind that started this fold
    pub kind: String,
    /// is the parent fold generating
    pub active: bool,
    /// whether to assemble inside this fold
    pub asm: bool,
    /// whether to generate symbols inside this fold
    pub r#gen: bool,
    /// is this an `END` fold
    pub is_end: bool,
    /// start of the fold
    pub start: lsp::Location,
}

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
    pub trigs: Triggers,
    /// stack of symbols representing the current scope
    symbol_stack: Vec<Symbol>,
    /// stack of document info for include descents
    source_stack: Vec<Source>,
    /// stack of folding ranges for conditionals and loops
    fold_stack: Vec<Fold>,
    /// built and consumed/cleared as lines are processed
    pub running_docstring: String,
    /// helps continue analysis of fold arguments
    pub fold_just_started: bool,
}

impl Triggers {
    pub fn new() -> Self {
        Self {
            unset_pc: false,
            unset_vars: false,
            unset_children: false,
            push_pc: false,
            push_vars: false,
            pop_pc: false,
            pop_vars: false,
            checkpoint_vars: false,
            set_pc: None,
            advance_pc: None
        }
    }
}

impl Fold {
    fn new(kind: String,active: bool,asm: bool,r#gen: bool,is_end: bool,start: lsp::Location) -> Self {
        Self {
            kind,active,asm,r#gen,is_end,start
        }
    }
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
            trigs: Triggers::new(),
            symbol_stack: Vec::new(),
            source_stack: Vec::new(),
            fold_stack: Vec::new(),
            running_docstring: String::new(),
            fold_just_started: false
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
        self.fold_stack = Vec::new();
        self.fold_just_started = false;
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
    /// returns conditional assembly booleans (asm,gen,is_end)
    pub fn cond_asm(&self) -> (bool,bool,bool) {
        match self.fold_stack.last() {
            Some(fold) => (fold.asm || self.fold_just_started,fold.r#gen || self.fold_just_started,fold.is_end),
            None => (true,true,false)
        }
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
    /// column in the current source string, usually 0, but can be negative
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
    /// Enter or exit a folding range and set conditional flags.
    /// kind is the syntax tree node kind.
    /// Panics if the kind is not a folding kind.
    /// Fold starters are END, DUM, DO, IF, ELSE, LUP, MAC
    /// Fold enders are ELSE, FIN, --^, DEND, EOM.
    /// N.b. ELSE both starts and ends, END actually starts, and EOM can produce multiple folding ranges.
    pub fn folding_range(&mut self, kind: &str, rng: lsp::Range, loc: lsp::Location, arg: i64, diagnostics: Option<&mut Vec<lsp::Diagnostic>>) -> Vec<lsp::FoldingRange> {
        let mut ans = Vec::new();
        let mut start_locs = Vec::new();
        let mut diag = Vec::new();
        let (parent_asm,parent_gen,parent_end) = match self.fold_stack.last() {
            Some(fold) => (fold.asm,fold.r#gen,fold.is_end),
            None => (true,true,false)
        };
        if parent_end {
            return ans;
        }
        let active = parent_asm || parent_gen; // this fold is active it the parent is assembling or generating
        let curr_rng = loc.range.clone();
        let curr_uri = loc.uri.clone();
        let fold_depth = self.fold_stack.len();
        match kind {
            "psop_end" => if active {
                self.fold_stack.push(Fold::new(kind.to_string(),true,false,false,true,loc))
            },
            "psop_dum" => self.fold_stack.push(Fold::new(kind.to_string(),active,false,active,false,loc)),
            "psop_do" | "psop_if" => self.fold_stack.push(Fold::new(kind.to_string(),active,parent_asm && arg!=0,parent_gen && arg!=0,false,loc)),
            "psop_else" => {
                let d1 = basic_diag(rng,"unmatched ELSE",lsp::DiagnosticSeverity::ERROR);
                if let Some(prev) = self.fold_stack.last() {
                    if ["psop_do","psop_if","psop_else"].contains(&prev.kind.as_str()) {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                        let (grand_asm,grand_gen) = match self.fold_stack.last() {
                            Some(fold) => (fold.asm,fold.r#gen),
                            None => (true,true)
                        };
                        self.fold_stack.push(Fold::new(kind.to_string(),
                            grand_asm || grand_gen,
                            grand_asm && !parent_asm,
                            grand_gen && !parent_gen,
                            false,loc));
                    } else {
                        diag.push(d1);
                    }
                } else {
                    diag.push(d1);
                }
            },
            "psop_lup" => self.fold_stack.push(Fold::new(kind.to_string(),active,parent_asm,parent_gen,false,loc)),
            "psop_mac" => {
                let mut hole = false;
                let mut some_mac = false;
                let mut some_effect: Option<String> = None;
                for fold in &self.fold_stack {
                    some_mac = some_mac || fold.kind=="psop_mac";
                    hole = some_mac && fold.kind!="psop_mac";
                    if fold.kind=="psop_do" || fold.kind=="psop_if" || fold.kind=="psop_dum" || fold.kind=="psop_else" {
                        some_effect = Some(fold.kind[5..].to_string().to_uppercase());
                    }
                }
                if hole {
                    diag.push(basic_diag(rng,"starting MAC is interrupted by another fold",lsp::DiagnosticSeverity::WARNING));
                }
                if let Some(effect) = some_effect {
                    diag.push(basic_diag(rng,&format!("MAC enclosed in {} is unnecessary and invites trouble",effect),lsp::DiagnosticSeverity::WARNING));
                }
                self.fold_stack.push(Fold::new(kind.to_string(),true,false,true,false,loc))
            },
            "psop_fin" => {
                let d1 = basic_diag(rng, "unmatched FIN",lsp::DiagnosticSeverity::ERROR);
                if let Some(prev) = self.fold_stack.last() {
                    if ["psop_do","psop_if","psop_else"].contains(&prev.kind.as_str()) {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                    } else {
                        diag.push(d1);
                    }
                } else {
                    diag.push(d1);
                }
            },
            "psop_end_lup" => {
                let d1 = basic_diag(rng, "unmatched end of loop",lsp::DiagnosticSeverity::ERROR);
                if let Some(prev) = self.fold_stack.last() {
                    if prev.kind == "psop_lup" {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                    } else {
                        diag.push(d1);
                    }
                } else {
                    diag.push(d1);
                }
            },
            "psop_eom" => {
                let d1 = basic_diag(rng, "unmatched end of macro (EOM terminates all preceding MAC)",lsp::DiagnosticSeverity::ERROR);
                if let Some(prev) = self.fold_stack.last() {
                    if prev.kind == "psop_mac" {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                    } else {
                        diag.push(d1);
                    }
                } else {
                    diag.push(d1);
                }
                while let Some(prev) = self.fold_stack.last() {
                    if prev.kind == "psop_mac" {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                    } else {
                        break;
                    }
                }
                for fold in &self.fold_stack {
                    if fold.kind == "psop_mac" {
                        diag.push(basic_diag(rng,"could not reach starting MAC due to another fold",lsp::DiagnosticSeverity::WARNING));
                    }
                }
            }
            "psop_dend" => {
                let d1 = basic_diag(rng, "unmatched DEND",lsp::DiagnosticSeverity::ERROR);
                if let Some(prev) = self.fold_stack.last() {
                    if prev.kind == "psop_dum" {
                        start_locs.push(prev.start.clone());
                        self.close_one_fold(&mut diag);
                    } else {
                        diag.push(d1);
                    }
                } else {
                    diag.push(d1);
                }
            }
            _ => panic!("unexpected folding range kind")
        };

        self.fold_just_started = self.fold_stack.len() > fold_depth;
        for start_loc in start_locs {
            if start_loc.uri != curr_uri {
                let info = vec![lsp::DiagnosticRelatedInformation {
                    location: start_loc,
                    message: "fold starts here".to_string()
                }];
                let d1 = lsp::Diagnostic::new(curr_rng,Some(lsp::DiagnosticSeverity::WARNING),None,None,
                    "fold starts in another document".to_string(),Some(info),None);
                diag.push(d1);
            } else if curr_rng.start.line > start_loc.range.start.line && curr_rng.start.line > 0 {
                ans.push(lsp::FoldingRange {
                    start_line: start_loc.range.start.line,
                    end_line: curr_rng.start.line - 1,
                    start_character: None,
                    end_character: None,
                    kind: None,
                    collapsed_text: None
                });
            }
        }
        if let Some(diagnostics) = diagnostics {
            diagnostics.append(&mut diag);
        }
        ans
    }
    /// fill diagnostics and possibly the END fold upon reaching end of document
    pub fn close_all_folds(&mut self, doc: Arc<Document>,
        diagnostic_set: &mut HashMap<String,Vec<lsp::Diagnostic>>,
        folding_set: &mut HashMap<String,Vec<lsp::FoldingRange>>) {
        
        let line_count = doc.text.lines().count() as u32;
        let end_line = match line_count > 0 {
            true => line_count -1,
            false => 0
        };
        let end_loc = lsp::Location::new(doc.uri.clone(), lsp::Range::new(
            lsp::Position::new(end_line,0),
            lsp::Position::new(end_line,0)));
        while let Some(fold) = self.fold_stack.pop() {
            let mut new_diag = None;
            if fold.is_end {
                let new_fold = lsp::FoldingRange {
                    start_line: fold.start.range.start.line,
                    end_line: end_loc.range.start.line,
                    start_character: None,
                    end_character: None,
                    kind: None,
                    collapsed_text: None
                };
                let rng = lsp::Range::new(lsp::Position::new(fold.start.range.start.line+1,0),end_loc.range.end);
                new_diag = Some(lsp::Diagnostic::new(rng,
                    Some(lsp::DiagnosticSeverity::HINT),None,None,"assembly disabled by END".to_string(),
                    None,Some(vec![lsp::DiagnosticTag::UNNECESSARY])));
                if fold.start.uri == end_loc.uri {
                    if let Some(v) = folding_set.get_mut(fold.start.uri.as_str()) {
                        v.push(new_fold);
                    } else {
                        folding_set.insert(fold.start.uri.to_string(),vec![new_fold]);
                    }
                }
            } else if self.config.flag.unclosed_folds.is_some() {
                new_diag = Some(basic_diag(fold.start.range, "folding range is never closed",self.config.flag.unclosed_folds.unwrap()));
            }
            if let Some(diag) = new_diag {
                if let Some(v) = diagnostic_set.get_mut(fold.start.uri.as_str()) {
                    v.push(diag);
                } else {
                    diagnostic_set.insert(fold.start.uri.to_string(),vec![diag]);
                }
            }
        }
    }
    pub fn close_one_fold(&mut self, diagnostics: &mut Vec<lsp::Diagnostic>) {
        if let Some(fold) = self.fold_stack.pop() {
            if fold.active && !fold.r#gen {
                let message = match fold.kind.as_str() {
                    "psop_do" => "assembly disabled by DO",
                    "psop_if" => "assembly disabled by IF",
                    "psop_else" => "assembly disabled by ELSE",
                    "psop_end" => "assembly disabled by END",
                    _ => return
                };
                let rng = lsp::Range::new(
                    lsp::Position::new(fold.start.range.start.line+1,0),
                    lsp::Position::new(self.row() as u32,0)
                );
                diagnostics.push(lsp::Diagnostic::new(rng,
                    Some(lsp::DiagnosticSeverity::HINT),None,None,message.to_string(),
                    None,Some(vec![lsp::DiagnosticTag::UNNECESSARY])));
            }
        }
    }
    
    /// advance the row in the current source strings
    pub fn next_row(&mut self) {
        if let Some(src) = self.source_stack.last_mut() {
            src.row += 1;
            self.fold_just_started = false;
        }
    }
    /// adjust the column (0 for normal line, -2 for adjusted line, assuming LSP default position encoding)
    pub fn set_col(&mut self,new_offset: isize) {
        if let Some(src) = self.source_stack.last_mut() {
            src.col = new_offset;
        }
    }
    /// borrow symbol defining current scope
    pub fn curr_scope(&self) -> Option<&Symbol> {
        match self.symbol_stack.last() {
            Some(sym) => Some(sym),
            None => None
        }
    }
    /// borrow symbol defining current scope
    pub fn curr_scope_mut(&mut self) -> Option<&mut Symbol> {
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
            for dependency_txt in popped.dependencies {
                main.dependencies.insert(dependency_txt);
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
    /// Push dependency (nested or referenced macro) onto all current macro scopes.
    /// This is needed to work out whether a macro is referenced indirectly.
    pub fn push_dependency(&mut self,label: &str) {
        for sym in self.symbol_stack.iter_mut().rev() {
            if sym.flags & symbol_flags::MAC == 0 {
                return;
            }
            sym.add_dependency(label);
        }
    }
    /// Push clone of symbol onto the scope stack.
    /// Symbol is only pushed if it is a macro or non-external global.
    /// If we are pushing a global, this function will call `exit_scope` to get out of the
    /// previous global scope, if necessary.
    /// In Merlin, global scopes are never nested, and cannot exist within a macro scope.
    /// Moreover, an equivalence establishes a global scope in all Merlin versions.
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
        match symbols.globals.get(name) { Some(sym) => {
            self.symbol_stack.push(sym.clone());
        } _ => { match symbols.macros.get(name) { Some(sym) => {
            self.symbol_stack.push(sym.clone());
        } _ => {}}}}
    }
    /// Remove symbols from the scope stack, and merge changes with main store.
    /// In Merlin, EOM exits all prior macro scopes, so this can pop multiple scopes at once.
    pub fn exit_scope(&mut self,symbols: &mut Symbols) -> Option<String> {
        if let Some(sym) = self.symbol_stack.pop() {
            log::trace!("exit scope {}",&sym.name);
            let is_macro = sym.flags & symbol_flags::MAC > 0;
            if is_macro {
                let mut outermost = sym.name.to_owned();
                Self::merge_macro(symbols,sym);
                while let Some(test) = self.symbol_stack.last() {
                    if test.flags & symbol_flags::MAC > 0 {
                        if let Some(curr) = self.symbol_stack.pop() {
                            outermost = curr.name.to_owned();
                            Self::merge_macro(symbols, curr);
                        }
                    } else {
                        break;
                    }
                }
                if let Some(outer) = symbols.macros.get(&outermost) {
                    if let Ok(maybe) = symbols.detect_all_duplicates_in_macro(outer) {
                        if let Some(mess) = maybe {
                            return Some(format!("duplicates found while closing scope: {}",mess));
                        }
                    }
                }
            } else {
                Self::merge_global(symbols, sym);
            }
        }
        None
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
    pub fn unused_macros_setting(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.unused_macros
    }
    pub fn unused_macros_in_context_setting(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.unused_macros_in_context
    }
    pub fn unused_labels_setting(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.unused_labels
    }
    pub fn unused_labels_in_context_setting(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.unused_labels_in_context
    }
    pub fn dup_mac_locs(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.dup_mac_locs
    }
    pub fn missing_entries(&self) -> Option<lsp::DiagnosticSeverity> {
        self.config.flag.missing_entries
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
        let doc_uris = ws.get_include_doc(&curs.node(), self.line());
        if doc_uris.len() != 1 {
            log::debug!("do not descend, no distinct match ({})",doc_uris.len());
            return None;
        }
        for doc in &ws.docs {
            if doc.uri == doc_uris[0] {
                return Some((new_typ,Arc::new(doc.to_owned())));
            }
        }
        log::debug!("do not descend, no checkpointed document found");
        return None;
	}
    pub fn is_include(&self) -> bool {
        if let Some(src) = self.curr_source() {
            return match src.typ {
                SourceType::Put | SourceType::Use => true,
                _ => false
            };
        }
        false
    }
}
