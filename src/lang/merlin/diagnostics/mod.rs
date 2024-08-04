//! Merlin diagnostics module.
//! 
//! Provides diagnostics and symbol information for a master document and its includes.
//! Also contains the workspace scanner and workspace methods.
//! Used for both CLI and LSP.

use tree_sitter;
use workspace::WorkspaceScanner;
use super::Workspace;
use std::io;
use std::io::Read;
use std::collections::HashMap;
use std::sync::Arc;
use lsp_types::{Diagnostic,DiagnosticSeverity,FoldingRange};
use crate::lang::{Navigate,Navigation,Document};
use crate::lang::merlin::context::Context;
use crate::lang::server::Analysis;
use crate::{DYNERR, STDRESULT};
use log::{info,trace};

pub mod macros;
mod labels;
mod syntax;
mod addressing;
mod pseudo;
pub mod workspace;

fn node_path(node: &tree_sitter::Node, source: &str) -> Vec<String> {
    let mut txt = super::super::node_text(node,source);
    if !txt.ends_with(".S") && !txt.ends_with(".s") {
        txt.push_str(".S");
    }
    txt.split("/").map(|x| x.to_string()).collect::<Vec<String>>()
}

/// Return a value indicating the quality of the match of a ProDOS path to a path in the
/// local file system.  Any value >0 means the filename itself matched case insensitively.
/// Higher values mean there were additional matches, such as parent directories.
fn match_prodos_path(node: &tree_sitter::Node, source: &str, doc: &Document) -> usize {
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
fn find_arg_node(node: &tree_sitter::Node) -> Option<String> {
    let mut check = node.parent();
    while let Some(parent) = check {
        if parent.kind().starts_with("arg_") {
            return Some(parent.kind().to_string());
        }
        check = parent.parent();
    }
    None
}

pub struct Analyzer {
    parser: super::MerlinParser,
    ctx: Context,
    scanner: WorkspaceScanner,
    workspace_folders: Vec<lsp_types::Url>,
    addr_mode_visitor: addressing::AddressModeSentry,
    pseudo_op_visitor: pseudo::PseudoOpSentry,
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
    /// This is a temporary that will be moved into diagnostic_set.
    folding: Vec<FoldingRange>,
    /// Map from URI's to their preferred master URI's
    preferred_masters: HashMap<String,String>,
    symbols: super::Symbols
}

impl Analyzer {
    pub fn new() -> Self {
        let ctx = Context::new();
        let addr_mode_visitor = addressing::AddressModeSentry::new(&ctx);
        Self {
            parser: super::MerlinParser::new(),
            ctx,
            scanner: WorkspaceScanner::new(),
            workspace_folders: Vec::new(),
            addr_mode_visitor,
            pseudo_op_visitor: pseudo::PseudoOpSentry::new(),
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
        self.diagnostics = Vec::new();
        self.symbols = super::Symbols::new();
    }
    fn reset_for_pass(&mut self) {
        self.ctx.reset_for_pass();
        self.pseudo_op_visitor = pseudo::PseudoOpSentry::new();
        self.addr_mode_visitor = addressing::AddressModeSentry::new(&self.ctx);
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
    pub fn rescan_workspace(&mut self,gather: bool) -> STDRESULT {
        if gather {
            log::debug!("GATHER WORKSPACE DOCUMENTS");
            self.scanner.gather_docs(&self.workspace_folders, 1000)?;
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
    fn analyze_recursively(&mut self,typ: super::SourceType,doc: Arc<Document>) -> Result<(),DYNERR> {
        // save diagnostics for the previous source scope
        self.move_diagnostics();
        self.move_folds();
        self.diagnostics = Vec::new();
        self.folding = Vec::new();
        self.ctx.enter_source(typ,Arc::clone(&doc));
        for line in doc.text.lines() {
            trace!("analyze row {}",self.ctx.row());
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
    fn descend(&mut self, curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if let Some((typ,include)) = self.ctx.prepare_to_descend(curs,self.scanner.get_workspace()) {
            trace!("descending into include {}",include.uri.as_str());
            self.analyze_recursively(typ,include)?;
            trace!("ascending out of include");
		}
		return Ok(Navigation::GotoSibling);
	}
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
        match self.pass {
            1 => labels::visit_gather(curs, &mut self.ctx, &self.scanner.get_workspace(), &mut self.symbols, &mut self.diagnostics),
            2 => labels::visit_verify(curs, &mut self.ctx, &self.scanner.get_workspace(), &mut self.symbols, &mut self.diagnostics),
            3 => {
                syntax::visit(curs, &self.ctx, &mut self.diagnostics);
                self.addr_mode_visitor.visit(curs, &mut self.ctx, &mut self.diagnostics);
                self.pseudo_op_visitor.visit(curs, &mut self.ctx, &mut self.diagnostics, &mut self.folding);
                Ok(Navigation::GotoChild)
            }
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
            info!("skipping linker file {}",doc.uri.to_string());
            return Ok(());
        }
        let preferred = self.preferred_masters.get(&doc.uri.to_string()).cloned();
        let master = Arc::new(ws.get_master(doc, preferred));
        self.symbols.master_doc_uri = master.uri.to_string();
        self.symbols.display_doc_uri = doc.uri.to_string();
        self.symbols.assembler = self.ctx.merlin_version();
        self.symbols.processor = self.ctx.curr_proc();
        info!("Use master {}",master.uri.to_string());
        for pass in 1..4 {
            log::debug!("ANALYSIS PASS {}",pass);
            self.pass = pass;
            self.reset_for_pass();
            self.analyze_recursively(super::SourceType::Master,Arc::clone(&master))?;
            // clean up any residual scope (this is a must for global scopes)
            self.ctx.exit_scope(&mut self.symbols);
        }
        info!("Assembler: {}",self.symbols.assembler);
        info!("Processor: {}",self.symbols.processor);
        info!("Globals: {}",self.symbols.globals.len());
        info!("Macros: {}",self.symbols.macros.len());
        info!("Variables: {}",self.symbols.vars.len());
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
            return crate::lang::line_entry(tree_sitter_merlin6502::language(), ":");
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
        crate::lang::eprint_lines_sexpr(tree_sitter_merlin6502::language(), program,1);
    }
}

