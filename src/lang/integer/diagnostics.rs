//! Integer diagnostics module.
//! 
//! Provides diagnostics and symbol information.
//! Used for both CLI and LSP.

use tree_sitter;
use std::io;
use std::io::Read;
use std::fmt::Write;
use lsp_types::{Diagnostic,DiagnosticSeverity};
use crate::lang::{Document,lsp_range, node_text, node_integer, extended_range};
use crate::{DYNERR, STDRESULT};
use log::{trace,warn};

use crate::lang::{Navigate,Navigation};
use crate::lang::server::Analysis;

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
    fn analyze(&mut self,doc: &Document) -> Result<(),DYNERR> {
        self.diagnostics = Vec::new();
        self.symbols = super::Symbols::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_integerbasic::language())?;
        for pass in 1..3 {
            self.last_good_line_number = -1;
            self.saved_depth = 0;
            self.in_dim_statement = false;
            self.row = 0;
            self.col = 0;
            self.pass = pass;
            for line in doc.text.lines() {
                trace!("analyze row {}",self.row);
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
            return crate::lang::line_entry(tree_sitter_integerbasic::language(), ">");
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
        crate::lang::eprint_lines_sexpr(tree_sitter_integerbasic::language(), program,1);
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
            err_pattern: regex::Regex::new(r"^(?i) *(D *S *P|N *O *D *S *P|N *E *X *T|I *N *P *U *T) *[A-Z]").expect("regex failure")
        }
    }
    pub fn set_config(&mut self,config: super::settings::Settings) {
        self.config = config;
    }
    pub fn get_symbols(&self) -> super::Symbols {
        self.symbols.clone()
    }
    fn create(&self,rng: tree_sitter::Range,mess: &str,severity: DiagnosticSeverity) -> Diagnostic {
        crate::lang::server::basic_diag(lsp_range(rng,self.row,self.col), mess, severity)
    }
    fn push(&mut self,rng: tree_sitter::Range,mess: &str,severity: DiagnosticSeverity) {
        self.diagnostics.push(self.create(rng,mess,severity));
    }
	fn process_variable_defs(&mut self,maybe_node: Option<tree_sitter::Node>, nmax: usize) {
		if let Some(node) = maybe_node {
            let mut maybe_next = Some(node);
            let mut num_found = 0;
            while let Some(next) = maybe_next {
                if num_found >= nmax {
                    return;
                }
                if super::LEXPR.contains(&next.kind()) {
                    num_found += 1;
                    if let Ok((keyname, cased, is_array, is_string, rng)) = super::lexpr_to_key(next,self.row,&self.line) {
                        if !self.symbols.vars.contains_key(&keyname) {
                            self.symbols.vars.insert(keyname.clone(),super::Variable::new());
                        }
                        let var_info = self.symbols.vars.get_mut(&keyname).unwrap();
                        var_info.is_array = is_array;
                        var_info.is_string = is_string;
                        if self.in_dim_statement {
                            var_info.decs.push(rng);
                        }
                        else {
                            var_info.defs.push(rng);
                        }
                        var_info.case.insert(cased);
                    }
                }
                if next.kind() == "open_dim_str" || next.kind() == "open_dim_int" {
                    maybe_next = super::pass_through_subscript(next,&self.line);
                } else {
                    maybe_next = next.next_named_sibling();
                }
            }
        }
	}
    /// gathers primary symbol information
    fn visit_primaries(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.saved_depth {
			self.saved_depth = 0;
			self.in_dim_statement = false;
		}
		let parent = curs.node().parent();
		let rng = curs.node().range();
		if curs.node().is_error() || curs.node().is_missing() {
			return Ok(Navigation::GotoSibling);
        }
		if curs.node().kind() == "linenum" && parent.is_some() && parent.unwrap().kind() == "line" {
			let mut next_statement = curs.node().next_named_sibling();
			let mut remark: Option<String> = None;
			while next_statement.is_some() {
				if let Some(child) = next_statement.unwrap().named_child(0) {
                    if child.kind() == "statement_rem" {
                        if let Some(txt) = child.next_named_sibling() {
                            remark = Some(node_text(&txt,&self.line));
                        }
                    }
                }
				next_statement = next_statement.unwrap().next_named_sibling();
			}
			if let Some(num) = node_integer(&curs.node(),&self.line) {
                if num < 0 || num > 32767 {
                    self.push(rng, "Out of range (0,32767)",DiagnosticSeverity::ERROR);
                }
                else if num as i64 <= self.last_good_line_number {
                    self.push(rng, "Line number out of order",DiagnosticSeverity::ERROR);
                }
                else {
                    self.symbols.lines.insert(num, super::Line {
                        rem: remark,
                        primary: lsp_range(rng,self.row,self.col),
                        gosubs: Vec::new(),
                        gotos: Vec::new()
                    });
                    self.last_good_line_number = num as i64;
                }
            }
			return Ok(Navigation::GotoSibling);
		}
		else if curs.node().kind().starts_with("statement_dim_") {
            self.in_dim_statement = true;
            self.saved_depth = curs.depth();
            self.process_variable_defs(Some(curs.node()), 64);
			return Ok(Navigation::GotoParentSibling);
        }
		else if curs.node().kind().starts_with("assignment_") {
            self.process_variable_defs(curs.node().named_child(0), 1);
            return Ok(Navigation::GotoParentSibling);
		}
		else if curs.node().kind().starts_with("statement_input_") {
            self.process_variable_defs(Some(curs.node()), 64);
            return Ok(Navigation::GotoParentSibling);
		}
		else if curs.node().kind() == "statement_for" {
            self.process_variable_defs(Some(curs.node()), 1);
            return Ok(Navigation::GotoParentSibling);
		}
		// this determines how deep in the tree we need to go
		else if curs.depth() < 4 {
			return Ok(Navigation::GotoChild);
        }
		
		return Ok(Navigation::GotoSibling);
    }
	fn value_range(&mut self,node: tree_sitter::Node, low: i64, high: i64) {
		if node.kind()=="integer" || node.kind()=="unary_aexpr" {
            match node_integer::<i64>(&node,&self.line) {
                Some(parsed) => if parsed < low || parsed > high {
                    let mut mess = "Out of range (".to_string();
                    write!(&mut mess,"{low},{high})").expect("could not format");
                    self.push(node.range(),&mess,DiagnosticSeverity::ERROR);
                },
                _ => warn!("parsing numerical node failed {}",node_text(&node,&self.line))
            }
        }
	}
    /// Only process the ref if it is a literal integer.
    /// Always start on GOTO, GOSUB, or THEN node (no other branching possible).
    fn process_linenum_ref(&mut self, curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if let Some(node) = curs.node().next_named_sibling() {
            if node.kind() == "integer" {
                let rng = lsp_range(node.range(), self.row, self.col);
                let line = match node_integer(&node,&self.line) {
                    Some(x) => self.symbols.lines.get_mut(&x),
                    _ => None
                };
                if line.is_some() {
                    if curs.node().kind() == "statement_gosub" {
                        line.unwrap().gosubs.push(rng);
                    } else {
                        line.unwrap().gotos.push(rng);
                    }
                } else if node.parent().is_some() && node.parent().unwrap().has_error()	{
                    self.push(node.range(), "Maybe unanalyzed (fix line)", DiagnosticSeverity::WARNING);
                    return Ok(Navigation::GotoSibling);
                } else if self.config.flag.bad_references.is_some() {
                    self.push(node.range(), "Line does not exist",self.config.flag.bad_references.unwrap());
                }
            }
        }
		return Ok(Navigation::GotoChild);
	}
    /// This is designed to take only the inner node.
    /// Child and sibling are used to identify strings and arrays.
	fn process_variable_ref(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>	{
        if curs.node().has_error() {
            return Ok(Navigation::GotoSibling);
        }
		let (keyname,cased,is_array,is_string) = super::var_to_key(curs.node(),&self.line);
        let name_range = lsp_range(curs.node().range(), self.row, self.col);
        if !self.symbols.vars.contains_key(&keyname) {
            self.symbols.vars.insert(keyname.clone(),super::Variable::new());
        }
        let var_info = self.symbols.vars.get_mut(&keyname).unwrap();
        var_info.is_array = is_array;
        var_info.is_string = is_string;
        var_info.refs.push(name_range);
        var_info.case.insert(cased);
        let is_declared = var_info.decs.len() > 0;
        let is_defined = var_info.defs.len() > 0;
        if !is_declared && self.config.flag.undeclared_arrays.is_some() {
            if is_array && !is_string {
                self.push(curs.node().range(), "array is never DIM'd", self.config.flag.undeclared_arrays.unwrap());
            } else if is_string {
                self.push(curs.node().range(), "string is never DIM'd", self.config.flag.undeclared_arrays.unwrap());
            }
        }
        if !is_defined && self.config.flag.undefined_variables.is_some() {
            self.push(curs.node().range(), "variable is never assigned", self.config.flag.undefined_variables.unwrap());
        }
        if is_declared && !is_array && !is_string {
            self.push(curs.node().range(),"unsubscripted integer array returns the first element",DiagnosticSeverity::INFORMATION);
        }
        Ok(Navigation::GotoSibling)
	}
    fn visit_node(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.saved_depth {
			self.saved_depth = 0;
			self.in_dim_statement = false;
		}
		let rng = curs.node().range();
        if self.config.flag.case_sensitive.is_some() {
            for chk in CASE_CHECK {
                let txt = node_text(&curs.node(), &self.line);
                if curs.node().kind().starts_with(chk) && txt != txt.to_uppercase() {
                    self.push(rng,"settings require upper case",self.config.flag.case_sensitive.unwrap());
                }
            }
        }
        if curs.node().is_missing() {
            let mut mess = "something is missing".to_string();
            let (node,more) = match curs.node().parent() {
                Some(p) => (p,[" after ",&p.to_sexp()].concat()),
                None => (curs.node(),"".to_string())
            };
            mess += &more;
            let syn_rng = extended_range(&node, self.line.len());
            self.push(syn_rng, &mess, DiagnosticSeverity::ERROR);
            return Ok(Navigation::GotoSibling);
        }
        if curs.node().is_error() {
            self.push(curs.node().range(), &("syntax error: ".to_string() + &curs.node().to_sexp()), DiagnosticSeverity::ERROR);
            return Ok(Navigation::GotoSibling);
        }
		if curs.node().kind() == "line" {
			if self.line.trim_end().len() > self.config.warn.length as usize {
				self.push(rng, "Line may be too long", DiagnosticSeverity::WARNING);
            }
		} else if ["statement_goto", "statement_gosub", "statement_then_line"].contains(&curs.node().kind()) {
			return self.process_linenum_ref(curs);
		} else if curs.node().kind() == "statement_poke" {
            if let Some(addr) = curs.node().next_named_sibling() {
                self.value_range(addr, -32767, 32767);
                if let Some(sep) = addr.next_named_sibling() {
                    if let Some(byte) = sep.next_named_sibling() {
                        self.value_range(byte,0,255);
                    }
                }
            }
		}
		else if curs.node().kind() == "fcall_peek" {
            if let Some(open) = curs.node().next_named_sibling() {
                if let Some(addr) = open.next_named_sibling() {
                    self.value_range(addr, -32767, 32767);
                }
            }
		}
		else if curs.node().kind() == "statement_coloreq" {
            if let Some(col) = curs.node().next_named_sibling() {
                self.value_range(col, 0, 255);
            }
		}
		else if curs.node().kind() == "statement_call" {
			if let Some(addr) = curs.node().next_named_sibling() {
				self.value_range(addr, -32767, 32767);
            }
		}
		else if curs.node().kind().starts_with("assignment") {
			if let Some(child) = curs.node().named_child(0) {
                if child.kind() != "statement_let" {
                    // check cases not handled by TS parser: DSP, NODSP, NEXT, INPUT
                    // e.g., cannot have NEXTA=1, but can have NEXT=1 or NEXT1=1
                    if self.err_pattern.is_match(&node_text(&child, &self.line)) {
                        self.push(child.range(), "illegal variable name, try LET",DiagnosticSeverity::ERROR);
                    }
                }
			}
		}
		else if super::SIMPLE_VAR_TYPES.contains(&curs.node().kind()) {
			// this will cover arrays within by looking at context
			self.process_variable_ref(curs)?;
		}
		else if curs.node().kind().starts_with("statement_dim_") {
			self.in_dim_statement = true;
			self.saved_depth = curs.depth();
		}
		else if curs.node().kind().starts_with("com_") && self.config.flag.immediate_mode.is_some() {
			self.push(rng, "Immediate mode command, Apple tokenizer will reject",self.config.flag.immediate_mode.unwrap());
		}
		return Ok(Navigation::GotoChild);
    }
}