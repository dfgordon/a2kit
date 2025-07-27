//! Merlin diagnostics module.
//! 
//! Provides diagnostics and symbol information for a master document and its includes.
//! Also contains the workspace scanner and workspace methods.
//! Used for both CLI and LSP.

use tree_sitter::{Node,TreeCursor};
use workspace::WorkspaceScanner;
use super::{Symbols,Symbol,Workspace,assembly};
use std::io;
use std::io::Read;
use std::collections::HashMap;
use std::sync::Arc;
use lsp_types::{Diagnostic,DiagnosticSeverity,FoldingRange,Location};
use crate::lang::{Navigate,Navigation,Document,lsp_range};
use crate::lang::merlin::context::Context;
use crate::lang::server::{Analysis,basic_diag};
use crate::{DYNERR, STDRESULT};

pub mod macros;
mod pass1;
mod pass2;
mod asm;
pub mod workspace;

fn node_path(node: &Node, source: &str) -> Vec<String> {
    let mut txt = super::super::node_text(node,source);
    if !txt.ends_with(".S") && !txt.ends_with(".s") {
        txt.push_str(".S");
    }
    txt.split("/").map(|x| x.to_string()).collect::<Vec<String>>()
}

/// Return a value indicating the quality of the match of a ProDOS path to a path in the
/// local file system.  Any value >0 means the filename itself matched case insensitively.
/// Higher values mean there were additional matches, such as parent directories.
fn match_prodos_path(node: &Node, source: &str, doc: &Document) -> usize {
    let mut quality = 0;
    if !doc.uri.cannot_be_a_base() {
        let mut doc_segs = std::path::Path::new(doc.uri.path()).iter().rev();
        for node_seg in node_path(node,source).iter().rev() {
            if let Some(doc_seg) = doc_segs.next() {
                if let Some(s) = doc_seg.to_str() {
                    if s.to_lowercase() == node_seg.to_lowercase() {
                        quality += 1;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    quality
}

/// Starting from someplace in an argument tree, go up to find the arg_* node kind.
/// This is useful if we need to adjust the processing based on the specific operation.
fn find_arg_node(node: &Node) -> Option<String> {
    let mut check = node.parent();
    while let Some(parent) = check {
        if parent.kind().starts_with("arg_") {
            return Some(parent.kind().to_string());
        }
        check = parent.parent();
    }
    None
}

/// Get the current value of any symbol or MX by looking around the node.
/// Node type should be `arg_mx` or `label_def`.
fn get_value(node: &Node, symbols: &Symbols, line: &str, scope: Option<&Symbol>) -> Option<i64> {
    let mut new_val: Option<i64> = None;
    // TODO: assign a value based on PC here
    if node.kind() == "arg_mx" {
        if let Some(expr) = node.child(0) {
            new_val = match super::assembly::eval_expr(&expr, line, None, symbols, scope) {
                Ok(v) => Some(v),
                Err(_) => None
            }
        }
    } else if node.kind() == "label_def" {
        if let Some(c2) = node.next_named_sibling() {
            if c2.kind()=="psop_equ" {
                if let Some(c3) = c2.next_named_sibling() {
                    new_val = match super::assembly::eval_expr(&c3, line, None, symbols, scope) {
                        Ok(v) => Some(v),
                        Err(_) => None 
                    };
                }
            }
        }
    }
    new_val
}

/// Update the current value of a variable or MX by looking around the node.
/// Node type should be `arg_mx` or `(label_def (var_label))`.
/// This is intended for use during second and subsequent passes (does not checkpoint).
/// value of `txt` is not used if node is `arg_mx`.
fn update_var_value(txt: &str, node: &Node, symbols: &mut Symbols, line: &str, scope: Option<&Symbol>) {
    let new_val = get_value(node,symbols,line,scope);
    if node.kind() == "arg_mx" {
        symbols.mx.value = new_val;
    } else if node.kind() == "label_def" {
        if let Some(var) = node.named_child(0) {
            if var.kind() == "var_label" {
                if let Some(sym) = symbols.vars.get_mut(txt) {
                    sym.value = new_val;
                }
            }
        }
    }
}

/// Get value of a fold argument and add to diagnostics if there is an issue
fn eval_fold_expr(node: &Node,pc: Option<usize>,symbols: &Symbols,ctx: &Context,in_macro_def: bool,diagnostics: Option<&mut Vec<Diagnostic>>) -> i64 {
    let range = lsp_range(node.range(),ctx.row(),ctx.col());
    let cannot_eval_mess = match in_macro_def {
        true => basic_diag(range,"evaluation was deferred",DiagnosticSeverity::HINT),
        false => basic_diag(range,"extension cannot evaluate, assuming true",DiagnosticSeverity::WARNING)
    };
    let (arg,diag) = match node.next_named_sibling() {
        Some(arg_node) => match node.kind() {
            "psop_if" | "psop_do" => match assembly::eval_conditional(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => (val,None),
                Err(_) => (1,Some(cannot_eval_mess))
            },
            _ =>  match assembly::eval_expr(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => (val,None),
                Err(_) => (1,None)
            }
        },
        None => (1,None)
    };
    if let Some(diag) = diag {
        if let Some(diagnostics) = diagnostics {
            diagnostics.push(diag);
        }
    }
    arg
}

pub struct Analyzer {
    parser: super::MerlinParser,
    ctx: Context,
    scanner: WorkspaceScanner,
    workspace_folders: Vec<lsp_types::Url>,
    asm: asm::Asm,
    pass: usize,
    /// Map from document uri to its diagnostics.
    /// Scope of this set is a master and its includes.
    diagnostic_set: HashMap<String,Vec<Diagnostic>>,
    /// Diagnostics for the document currently being analyzed.
    /// This is a temporary that will be moved into diagnostic_set.
    diagnostics: Vec<Diagnostic>,
    /// Map from document uri to its folding ranges.
    /// Scope of this set is a master and its includes.
    folding_set: HashMap<String,Vec<FoldingRange>>,
    /// Folding ranges for the document currently being analyzed.
    /// This is a temporary that will be moved into folding_set.
    folding: Vec<FoldingRange>,
    /// Map from URI's to their preferred master URI's
    preferred_masters: HashMap<String,String>,
    symbols: super::Symbols
}

impl Analyzer {
    pub fn new() -> Self {
        let ctx = Context::new();
        let asm = asm::Asm::new(&ctx);
        Self {
            parser: super::MerlinParser::new(),
            ctx,
            scanner: WorkspaceScanner::new(),
            workspace_folders: Vec::new(),
            asm,
            pass: 0,
            diagnostic_set: HashMap::new(),
            diagnostics: Vec::new(),
            folding_set: HashMap::new(),
            folding: Vec::new(),
            preferred_masters: HashMap::new(),
            symbols: super::Symbols::new()
        }
    }
    pub fn set_config(&mut self,config: super::settings::Settings) {
        self.ctx.set_config(config);
    }
    pub fn set_preferred_master(&mut self, disp: String, mast: String) {
        self.preferred_masters.insert(disp,mast);
    }
    fn reset_results(&mut self) {
        self.diagnostic_set = HashMap::new();
        self.folding_set = HashMap::new();
        self.diagnostics = Vec::new();
        self.symbols = super::Symbols::new();
        self.folding = Vec::new();
    }
    fn reset_for_pass(&mut self) {
        self.ctx.reset_for_pass();
        self.asm = asm::Asm::new(&self.ctx);
        self.symbols.unset_all_variables();
    }
    pub fn get_symbols(&self) -> super::Symbols {
        self.symbols.clone()
    }
    /// Borrow workspace data from the scanner
    pub fn get_workspace(&self) -> &Workspace {
        self.scanner.get_workspace()
    }
    /// Set the scanner's workspace data, probably from another analyzer
    pub fn set_workspace(&mut self, ws: Workspace) {
        self.scanner.set_workspace(ws);
    }
    /// Scan the last set of workspace folders that were supplied by the client.
    /// If `gather` is false, use only previously checkpointed documents.
    /// N.b. if `gather` is true, checkpointed documents are rolled back to previously saved version.
    pub fn rescan_workspace(&mut self,gather: bool) -> STDRESULT {
        if gather {
            log::debug!("GATHER WORKSPACE DOCUMENTS");
            self.scanner.gather_docs(&self.workspace_folders, 1000)?;
        }
        log::debug!("SCAN WORKSPACE DOCUMENTS");
        self.scanner.scan()
    }
    pub fn rescan_workspace_and_update(&mut self,checkpoints: Vec<Document>) -> STDRESULT {
        log::debug!("GATHER WORKSPACE DOCUMENTS");
        self.scanner.gather_docs(&self.workspace_folders, 1000)?;
        for doc in checkpoints {
            self.scanner.update_doc(&doc);
        }
        log::debug!("SCAN WORKSPACE DOCUMENTS");
        self.scanner.scan()
    }
    /// Move diagnostics for current document from temporary vector to permanent map of vectors.
    /// If no current document do nothing.
    fn move_diagnostics(&mut self) {
        if let Some(curr) = self.ctx.curr_source() {
            let uri = curr.doc.uri.to_string();
            match self.diagnostic_set.get_mut(&uri) {
                Some(v) => v.append(&mut self.diagnostics),
                None => {
                    let mut moved_diag = Vec::new();
                    moved_diag.append(&mut self.diagnostics);
                    self.diagnostic_set.insert(uri,moved_diag);
                }
            }
        }
    }
    /// Move folding ranges for current document from temporary vector to permanent map of vectors.
    /// If no current document do nothing.
    fn move_folds(&mut self) {
        if let Some(curr) = self.ctx.curr_source() {
            let uri = curr.doc.uri.to_string();
            match self.folding_set.get_mut(&uri) {
                Some(v) => v.append(&mut self.folding),
                None => {
                    let mut moved_fold = Vec::new();
                    moved_fold.append(&mut self.folding);
                    self.folding_set.insert(uri,moved_fold);
                }
            }
        }
    }
    /// Carry out actions requested by visitors on the previously processed line.
    /// When finished clear all triggers.
    fn pull_triggers(&mut self) {
        if self.ctx.trigs.push_vars {
            self.symbols.stash_all_variables();
        }
        if self.ctx.trigs.unset_vars {
            self.symbols.unset_all_variables();
        }
        if self.ctx.trigs.unset_children {
            if let Some(scope) = self.ctx.curr_scope_mut() {
                scope.unset_children();
            }
        }
        if self.ctx.trigs.pop_vars {
            self.symbols.restore_all_variables();
        }
        if self.ctx.trigs.checkpoint_vars {
            if let Some(src) = self.ctx.curr_source() {
                let pos = lsp_types::Position::new(self.ctx.row() as u32,0);
                let loc = Location::new(src.doc.uri.clone(), lsp_types::Range::new(pos,pos));
                self.symbols.checkpoint_all_variables(&loc);
            }
        }
        self.ctx.trigs = super::context::Triggers::new();
    }
    fn analyze_recursively(&mut self,typ: super::SourceType,doc: Arc<Document>) -> Result<(),DYNERR> {
        // save diagnostics for the previous source scope
        self.move_diagnostics();
        self.move_folds();
        self.diagnostics = Vec::new();
        self.folding = Vec::new();
        self.ctx.enter_source(typ,Arc::clone(&doc));
        for line in doc.text.lines() {
            log::trace!("analyze row {}",self.ctx.row());
            self.pull_triggers();
            if line.trim_start().len()==0 {
                self.ctx.next_row();
                self.ctx.running_docstring = String::new();
                continue;
            }
            let tree = self.parser.parse(line,&self.symbols)?;
            self.ctx.set_line(self.parser.line().to_string(),"");
            self.ctx.set_col(self.parser.col_offset());
            self.symbols.update_row_data(&doc,self.ctx.row(), self.ctx.col());
            self.walk(&tree)?;
            self.ctx.next_row();
        }
        // save diagnostics for this scope
        self.move_diagnostics();
        self.move_folds();
        self.ctx.exit_source();
        Ok(())
    }
}

impl Navigate for Analyzer {
	/// default descend function
	/// * `curs` expected to be on a PUT or USE pseudo-op node
	/// * returns where to go when we return to master
    fn descend(&mut self, curs: &TreeCursor) -> Result<Navigation,DYNERR> {
		if let Some((typ,include)) = self.ctx.prepare_to_descend(curs,self.scanner.get_workspace()) {
            log::trace!("descending into include {}",include.uri.as_str());
            self.analyze_recursively(typ,include)?;
            log::trace!("ascending out of include");
            if self.pass == 1 {
                self.ctx.trigs.checkpoint_vars = true;
            }
		}
		return Ok(Navigation::GotoSibling);
	}
    fn visit(&mut self,curs: &TreeCursor) -> Result<Navigation,DYNERR> {
        match self.pass {
            1 => pass1::visit_gather(curs, &mut self.ctx, &self.scanner.get_workspace(), &mut self.symbols, &mut self.diagnostics, &mut self.folding),
            2 => pass2::visit_verify(curs, &mut self.ctx, &self.scanner.get_workspace(), &mut self.symbols, &mut self.diagnostics),
            3 => self.asm.visit(curs, &mut self.ctx, &self.scanner.get_workspace(), &mut self.symbols, &mut self.diagnostics),
            _ => panic!("unexpected number of visit passes")
        }
    }
}

impl Analysis for Analyzer {
    fn init_workspace(&mut self,source_dirs: Vec<lsp_types::Url>,volatile_docs: Vec<Document>) -> STDRESULT {
        self.workspace_folders = source_dirs;
        self.scanner.gather_docs(&self.workspace_folders, 1000)?;
        self.scanner.append_volatile_docs(volatile_docs);
        self.scanner.scan()
    }
    fn analyze(&mut self,doc: &Document) -> Result<(),DYNERR> {
        self.reset_results();
        self.ctx.reset_xc();
        self.scanner.update_doc(doc);
        let ws = self.scanner.get_workspace();
        self.symbols.display_doc_type = ws.source_type(&doc.uri, self.ctx.linker_threshold());
        if self.symbols.display_doc_type == super::SourceType::Linker {
            log::info!("skipping linker file {}",doc.uri.to_string());
            return Ok(());
        }
        let preferred = self.preferred_masters.get(&doc.uri.to_string()).cloned();
        let master = Arc::new(ws.get_master(doc, preferred));
        self.symbols.master_doc_uri = master.uri.to_string();
        self.symbols.display_doc_uri = doc.uri.to_string();
        self.symbols.assembler = self.ctx.merlin_version();
        self.symbols.processor = self.ctx.curr_proc();
        log::info!("Use master {}",master.uri.to_string());
        for pass in 1..4 {
            log::debug!("ANALYSIS PASS {}",pass);
            self.pass = pass;
            self.reset_for_pass();
            self.analyze_recursively(super::SourceType::Master,Arc::clone(&master))?;
            if pass==1 {
                self.ctx.close_all_folds(Arc::clone(&master), &mut self.diagnostic_set, &mut self.folding_set);
            }
            // clean up any residual scope (this is a must for global scopes)
            self.ctx.exit_scope(&mut self.symbols);
        }
        log::info!("Assembler: {}",self.symbols.assembler);
        log::info!("Processor: {}",self.symbols.processor);
        log::info!("Globals: {}",self.symbols.globals.len());
        log::info!("Macros: {}",self.symbols.macros.len());
        log::info!("Variables: {}",self.symbols.vars.len());
        Ok(())
    }
    fn update_config(&mut self,json_str: &str) -> STDRESULT {
        match super::settings::parse(json_str) {
            Ok(config) => {
                self.ctx.set_config(config);
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    fn read_stdin(&self) -> String {
        if atty::is(atty::Stream::Stdin) {
            return crate::lang::line_entry(tree_sitter_merlin6502::LANGUAGE.into(), ":");
        }
        let mut program = String::new();
        io::stdin().read_to_string(&mut program).expect("failed to read stdin");
        program
    }
    fn get_diags(&self,doc: &Document) -> Vec<Diagnostic> {
        if let Some(diags) = self.diagnostic_set.get(&doc.uri.to_string()) {
            return diags.clone();
        }
        Vec::new()
    }
    fn get_folds(&self,doc: &Document) -> Vec<FoldingRange> {
        if let Some(folds) = self.folding_set.get(&doc.uri.to_string()) {
            return folds.clone();
        }
        Vec::new()
    }
    fn err_warn_info_counts(&self) -> [usize;3] {
        let mut err = 0;
        let mut warn = 0;
        let mut info = 0;
        for diag in self.diagnostic_set.values() {
            for item in diag {
                match item.severity {
                    Some(DiagnosticSeverity::ERROR) => err += 1,
                    Some(DiagnosticSeverity::WARNING) => warn += 1,
                    Some(DiagnosticSeverity::INFORMATION) => info += 1,
                    _ => {}
                }
            }
        }
        [err,warn,info]
    }
    fn eprint_lines_sexpr(&self,program: &str) {
        crate::lang::eprint_lines_sexpr(tree_sitter_merlin6502::LANGUAGE.into(), program,1);
    }
}

