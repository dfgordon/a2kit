//! Applesoft diagnostics module.
//! 
//! Provides diagnostics and symbol information.
//! Used for both CLI and LSP.

use std::collections::{HashSet,HashMap};
use tree_sitter;
use std::io;
use std::io::Read;
use std::fmt::Write;
use lsp_types::{self as lsp, DiagnosticSeverity};
use crate::lang::{Document, lsp_range, node_text, node_integer};
use crate::lang::server::basic_diag;
use workspace::WorkspaceScanner;
use super::{Workspace,Variable,Line,Symbols,Collisions};
use super::settings::Settings;
use crate::{DYNERR, STDRESULT};

use crate::lang::{Navigate,Navigation};
use crate::lang::server::Analysis;

use super::name_range;

mod workspace;
mod pass1;
mod pass2;
pub mod chain;
pub mod expression;

/// Update the `collisions` map from short keys to long keys.
/// The `long_key` should come from var_to_key.
/// Return a reference to the set of colliding long names if applicable, or None.
fn update_collisions<'a>(collisions: &'a mut Collisions,long_key: &String) -> Option<&'a HashSet<String>> {
    let mut word_len = 0;
    for c in long_key.chars() {
        if !c.is_ascii_alphanumeric() {
            break;
        }
        word_len += 1;
    }
    let trailing = match word_len {
        l if l == long_key.len() => String::new(),
        l if l >= 2 => long_key[l..].to_string(),
        _ => return None
    };
    if word_len >= 2 {
        let short_key = [long_key[0..2].to_string(),trailing].concat();
        if !collisions.contains_key(&short_key) {
            collisions.insert(short_key.clone(),HashSet::new());
        }
        let set = collisions.get_mut(&short_key).unwrap();
        set.insert(long_key.to_string());
        Some(set)
    } else {
        None
    }
}

struct FlowState {
    ip_branch_stack: Vec<lsp::Range>,
    curr_line_num: Option<u16>,
    row: isize,
    col: isize,
    line: String
}

impl FlowState {
    fn new() -> Self {
        Self {
            ip_branch_stack: Vec::new(),
            curr_line_num: None,
            row: 0,
            col: 0,
            line: String::new()
        }
    }
    fn new_line(&mut self,txt: &str,row: isize) {
        self.ip_branch_stack = Vec::new();
        self.curr_line_num = None;
        self.line = txt.to_string();
        self.row = row;
        self.col = 0;
    }
    /// use address node (presumed to follow POKE) to start an interprogram branch, if detected
    fn eval_ip_start(&mut self,addr: &tree_sitter::Node) {
        if let Some(val) = node_integer::<u8>(&addr,&self.line) {
            if val == 103 || val == 104 {
                self.ip_branch_stack.push(lsp_range(addr.range(), self.row, self.col));
            }
        }
    }
    /// use linenum reference to pop the interprogram branch if any, and check if line number condition
    /// is satisfied, return a diagnostic if needed.
    fn eval_ip_branch(&mut self,linenum: &tree_sitter::Node) -> Option<lsp::Diagnostic> {
        if self.ip_branch_stack.len() == 0 {
            return None;
        }
        self.ip_branch_stack = Vec::new(); // clear completely
        let end_rng = lsp_range(linenum.range(),self.row,self.col);
        if let Some(new) = node_integer::<u16>(&linenum,&self.line) {
            if let Some(curr) = self.curr_line_num {
                if curr/256 >= new/256 {
                    return Some(basic_diag(end_rng, "interprogram branch", lsp::DiagnosticSeverity::INFORMATION));
                } else {
                    return Some(basic_diag(end_rng,"interprogram branch violates search condition",lsp::DiagnosticSeverity::ERROR));
                }
            }
        }
        None
    }
    /// use print node to pop the interprogram branch in case we have a RUN command.
    fn eval_ip_run(&mut self,print_node: &tree_sitter::Node) -> Option<lsp::Diagnostic> {
        if self.ip_branch_stack.len() == 0 {
            return None;
        }
        if let Some(nxt) = print_node.next_named_sibling() {
            let txt = node_text(&nxt,&self.line).replace(" ","").to_lowercase();
            if txt.starts_with("chr$") || nxt.kind()=="var_str" {
                if let Some(nxtnxt) = nxt.next_named_sibling() {
                    if node_text(&nxtnxt,&self.line).replace(" ","").to_lowercase().starts_with("\"run") {
                        self.ip_branch_stack = Vec::new();
                    }
                }
            } else if txt.starts_with("\"\u{0004}run") || txt.starts_with("\"\\x04run") {
                self.ip_branch_stack = Vec::new();
            }
        }
        None
    }
}

pub struct Analyzer {
    row: isize,
    col: isize,
    pass: usize,
    config: Settings,
    line: String,
    diagnostics: Vec<lsp::Diagnostic>,
    fcollisions: Collisions,
    vcollisions: Collisions,
    symbols: Symbols,
    last_good_line_number: i64,
    flow: FlowState,
    depth_of_def: u32,
    dummy_var_key: String,
    curr_backlinks: HashSet<lsp::Uri>,
    scanner: WorkspaceScanner,
    workspace_folders: Vec<lsp::Uri>
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
        self.flow = FlowState::new();
        self.diagnostics = Vec::new();
        self.symbols = Symbols::new();
        self.fcollisions = HashMap::new();
        self.vcollisions = self.scanner.get_workspace().ws_collisions.clone();
        self.scanner.update_doc(doc);
        self.scanner.get_workspace().get_all_backlinks(&mut self.curr_backlinks, &doc.uri, &doc.uri, 0, 8);
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::LANGUAGE.into())?;
        for pass in 1..3 {
            self.last_good_line_number = -1;
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
                self.flow.new_line(&self.line, self.row);
                match parser.parse(&self.line,None) {
                    Some(tree) => self.walk(&tree)?,
                    None => return Err(Box::new(crate::lang::Error::ParsingError))
                };
                if self.flow.ip_branch_stack.len() > 0 {
                    let rng = self.flow.ip_branch_stack.pop().unwrap();
                    self.diagnostics.push(basic_diag(rng, "interprogram branch is not taken on this line", DiagnosticSeverity::ERROR));
                }
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
            return crate::lang::line_entry(tree_sitter_applesoft::LANGUAGE.into(), "]");
        }
        let mut program = String::new();
        io::stdin().read_to_string(&mut program).expect("failed to read stdin");
        program
    }
    fn get_diags(&self,_doc: &Document) -> Vec<lsp::Diagnostic> {
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
                Some(lsp::DiagnosticSeverity::ERROR) => err += 1,
                Some(lsp::DiagnosticSeverity::WARNING) => warn += 1,
                Some(lsp::DiagnosticSeverity::INFORMATION) => info += 1,
                _ => {}
            }
        }
        [err,warn,info]
    }
    fn eprint_lines_sexpr(&self,program: &str) {
        crate::lang::eprint_lines_sexpr(tree_sitter_applesoft::LANGUAGE.into(), program,1);
    }
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            row: 0,
            col: 0,
            pass: 0,
            config: Settings::new(),
            line: String::new(),
            diagnostics: Vec::new(),
            fcollisions: HashMap::new(),
            vcollisions: HashMap::new(),
            symbols: Symbols::new(),
            last_good_line_number: -1,
            flow: FlowState::new(),
            depth_of_def: 0,
            dummy_var_key: "".to_string(),
            curr_backlinks: HashSet::new(),
            scanner: WorkspaceScanner::new(),
            workspace_folders: Vec::new()
        }
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn get_symbols(&self) -> Symbols {
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
    fn create(&self,rng: tree_sitter::Range,mess: &str,severity: lsp::DiagnosticSeverity) -> lsp::Diagnostic {
        basic_diag(lsp_range(rng,self.row,self.col),mess,severity)
    }
    fn push(&mut self,rng: tree_sitter::Range,mess: &str,severity: lsp::DiagnosticSeverity) {
        self.diagnostics.push(self.create(rng,mess,severity));
    }
    fn linenum_range(&self,node: &tree_sitter::Node,source: &str) -> lsp_types::Range {
        let len = node_text(node,source).trim_end().len() as u32;
        let rng = lsp_range(node.range(),self.row,self.col);
        lsp_types::Range::new(rng.start,lsp_types::Position::new(rng.start.line,rng.start.character + len))
    }
	fn value_range(&mut self,node: tree_sitter::Node, low: f64, high: f64, expect_int: bool) {
        let Ok(val) = expression::eval_aexpr(&node, &self.line, &self.symbols) else {
            return;
        };
        let is_int = val.floor() == val;
        if expect_int && !is_int {
            self.push(node.range(),"Expected an integer",lsp::DiagnosticSeverity::ERROR);
        }
        if val < low || val > high {
            let mut mess = "Out of range (".to_string();
            match low >= -32767. && high <= 65535. {
                false => write!(&mut mess,"{low:e},{high:e})").expect("could not format"),
                true => write!(&mut mess,"{low},{high})").expect("could not format")
            };
            self.push(node.range(),&mess,lsp::DiagnosticSeverity::ERROR);
        }
	}
    fn check_case(&mut self, node: tree_sitter::Node, rng: tree_sitter::Range) {
        let txt = node_text(&node,&self.line);
        if self.config.flag.case_sensitive.is_some() && txt.to_uppercase() != txt {
            self.push(rng, "settings require upper case", self.config.flag.case_sensitive.unwrap());
        }
    }
}