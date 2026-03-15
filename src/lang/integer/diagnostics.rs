//! Integer diagnostics module.
//! 
//! Provides diagnostics and symbol information.
//! Used for both CLI and LSP.

use tree_sitter;
use std::io;
use std::io::Read;
use std::fmt::Write;
use std::collections::HashSet;
use lsp_types::{self as lsp,Diagnostic,DiagnosticSeverity};
use crate::lang::{Document,lsp_range, node_text};
use super::Workspace;
use workspace::WorkspaceScanner;
use crate::{DYNERR, STDRESULT};

use crate::lang::{Navigate,Navigation};
use crate::lang::server::Analysis;

mod workspace;
mod pass1;
mod pass2;
pub mod chain;
pub mod expression;

const CASE_CHECK: [&str;5] = ["statement_", "fcall_", "str_name", "int_name", "op_"];

pub struct Analyzer {
    config: super::settings::Settings,
    row: isize,
    col: isize,
    pass: usize,
    line: String,
    diagnostics: Vec<lsp_types::Diagnostic>,
    symbols: super::Symbols,
    last_good_line_number: i64,
    in_dim_statement: bool,
    saved_depth: u32,
    curr_backlinks: HashSet<lsp::Uri>,
    scanner: WorkspaceScanner,
    workspace_folders: Vec<lsp::Uri>,
    err_pattern: regex::Regex
}

impl Navigate for Analyzer {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
        match self.pass {
            1 => self.visit_primaries(curs),
            2 => self.visit_node(curs),
            _ => panic!("unexpected number of visit passes")
        }
    }
}

impl Analysis for Analyzer {
    fn init_workspace(&mut self,source_dirs: Vec<lsp_types::Uri>,volatile_docs: Vec<crate::lang::Document>) -> STDRESULT {
        self.workspace_folders = source_dirs;
        self.scanner.gather_docs(&self.workspace_folders, 1000)?;
        self.scanner.append_volatile_docs(volatile_docs);
        self.scanner.scan()
    }
    fn analyze(&mut self,doc: &Document) -> Result<(),DYNERR> {
        self.diagnostics = Vec::new();
        self.symbols = super::Symbols::new();
        self.scanner.update_doc(doc);
        self.scanner.get_workspace().get_all_backlinks(&mut self.curr_backlinks, &doc.uri, &doc.uri, 0, 8);
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_integerbasic::LANGUAGE.into())?;
        for pass in 1..3 {
            self.last_good_line_number = -1;
            self.saved_depth = 0;
            self.in_dim_statement = false;
            self.row = 0;
            self.col = 0;
            self.pass = pass;
            for line in doc.text.lines() {
                log::trace!("analyze row {}",self.row);
                if line.trim_start().len()==0 {
                    self.row += 1;
                    continue;
                }
                self.line = String::from(line) + "\n";
                match parser.parse(&self.line,None) {
                    Some(tree) => self.walk(&tree)?,
                    None => return Err(Box::new(crate::lang::Error::ParsingError))
                };
                self.row += 1;
            }    
        }
        Ok(())
    }
    fn update_config(&mut self,json_str: &str) -> STDRESULT {
        match super::settings::parse(json_str) {
            Ok(c) => {
                self.config = c;
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    fn read_stdin(&self) -> String {
        if atty::is(atty::Stream::Stdin) {
            return crate::lang::line_entry(tree_sitter_integerbasic::LANGUAGE.into(), ">");
        }
        let mut program = String::new();
        io::stdin().read_to_string(&mut program).expect("failed to read stdin");
        program
    }
    fn get_diags(&self,_doc: &Document) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }
    fn get_folds(&self,_doc: &crate::lang::Document) -> Vec<lsp_types::FoldingRange> {
        Vec::new()
    }
    fn err_warn_info_counts(&self) -> [usize;3] {
        let mut err = 0;
        let mut warn = 0;
        let mut info = 0;
        for item in &self.diagnostics {
            match item.severity {
                Some(DiagnosticSeverity::ERROR) => err += 1,
                Some(DiagnosticSeverity::WARNING) => warn += 1,
                Some(DiagnosticSeverity::INFORMATION) => info += 1,
                _ => {}
            }
        }
        [err,warn,info]
    }
    fn eprint_lines_sexpr(&self,program: &str) {
        crate::lang::eprint_lines_sexpr(tree_sitter_integerbasic::LANGUAGE.into(), program,1);
    }
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            config: super::settings::Settings::new(),
            row: 0,
            col: 0,
            pass: 0,
            line: String::new(),
            diagnostics: Vec::new(),
            symbols: super::Symbols::new(),
            last_good_line_number: -1,
            saved_depth: 0,
            in_dim_statement: false,
            curr_backlinks: HashSet::new(),
            scanner: WorkspaceScanner::new(),
            workspace_folders: Vec::new(),
            err_pattern: regex::Regex::new(r"^(?i) *(D *S *P|N *O *D *S *P|N *E *X *T|I *N *P *U *T) *[A-Z]").expect("regex failure")
        }
    }
    pub fn set_config(&mut self,config: super::settings::Settings) {
        self.config = config;
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
    fn create(&self,rng: tree_sitter::Range,mess: &str,severity: DiagnosticSeverity) -> Diagnostic {
        crate::lang::server::basic_diag(lsp_range(rng,self.row,self.col), mess, severity)
    }
    fn push(&mut self,rng: tree_sitter::Range,mess: &str,severity: DiagnosticSeverity) {
        self.diagnostics.push(self.create(rng,mess,severity));
    }
    fn linenum_range(&self,node: &tree_sitter::Node,source: &str) -> lsp_types::Range {
        let len = node_text(node,source).trim_end().len() as u32;
        let rng = lsp_range(node.range(),self.row,self.col);
        lsp_types::Range::new(rng.start,lsp_types::Position::new(rng.start.line,rng.start.character + len))
    }
	fn value_range(&mut self,node: tree_sitter::Node, low: i64, high: i64) {
        let Ok(val) = expression::eval_aexpr(&node, &self.line, &self.symbols) else {
            return;
        };
        if val < low || val > high {
            let mut mess = "Out of range (".to_string();
            write!(&mut mess,"{low},{high})").expect("could not format");
            self.push(node.range(),&mess,DiagnosticSeverity::ERROR);
        }
	}
}