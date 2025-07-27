//! Applesoft diagnostics module.
//! 
//! Provides diagnostics and symbol information.
//! Used for both CLI and LSP.

use std::collections::{HashSet,HashMap};
use tree_sitter;
use regex;
use std::io;
use std::io::Read;
use std::fmt::Write;
use std::borrow::BorrowMut;
use lsp_types::{self as lsp, DiagnosticSeverity};
use crate::lang::{Document,lsp_range, node_text, node_integer, extended_range};
use crate::lang::server::basic_diag;
use super::{Variable,Line,Symbols};
use super::settings::Settings;
use crate::{DYNERR, STDRESULT};
use log::{trace,warn};

use crate::lang::{Navigate,Navigation};
use crate::lang::server::Analysis;

use super::name_range;

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
    fcollisions: HashMap<String,HashSet<String>>,
    vcollisions: HashMap<String,HashSet<String>>,
    symbols: Symbols,
    last_good_line_number: i64,
    flow: FlowState,
    depth_of_def: u32,
    dummy_var_key: String,
    end_name: regex::Regex
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
        self.flow = FlowState::new();
        self.diagnostics = Vec::new();
        self.symbols = Symbols::new();
        self.fcollisions = HashMap::new();
        self.vcollisions = HashMap::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(&tree_sitter_applesoft::LANGUAGE.into())?;
        for pass in 1..3 {
            self.last_good_line_number = -1;
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
            end_name: regex::Regex::new(r"\W").expect("regex failure")
        }
    }
    pub fn set_config(&mut self,config: Settings) {
        self.config = config;
    }
    pub fn get_symbols(&self) -> Symbols {
        self.symbols.clone()
    }
    fn create(&self,rng: tree_sitter::Range,mess: &str,severity: lsp::DiagnosticSeverity) -> lsp::Diagnostic {
        basic_diag(lsp_range(rng,self.row,self.col),mess,severity)
    }
    fn push(&mut self,rng: tree_sitter::Range,mess: &str,severity: lsp::DiagnosticSeverity) {
        self.diagnostics.push(self.create(rng,mess,severity));
    }
    fn collision(&mut self,long_key: &String,rng: tree_sitter::Range,is_func: bool) {
        if !long_key.is_ascii() {
            warn!("attempt to evaluate non-ASCII name");
            return;
        }
        let mut colliding: Vec<String> = Vec::new();
        let (word_len,trailing) = match self.end_name.find(&long_key) {
            Some(m) => (m.start(),long_key[m.start()..].to_string()),
            None => (long_key.len(),"".to_string())
        };
        if word_len >= 2 {
            // TODO: solve the clone anti-pattern
            let short_key = [long_key[0..2].to_string(),trailing].concat();
            let mut long_set = match is_func {
                true => match self.fcollisions.get(&short_key) {
                    Some(set) => set.clone(),
                    None => HashSet::new()
                },
                false => match self.vcollisions.get(&short_key) {
                    Some(set) => set.clone(),
                    None => HashSet::new()
                }
            };
            long_set.insert(long_key.to_string());
            for s in &long_set {
                colliding.push(s.to_string());
            }
            if is_func {
                self.fcollisions.insert(short_key,long_set);
            } else {
                self.vcollisions.insert(short_key,long_set);
            }
        }
        if colliding.len() > 1 {
            let mut mess = "variable name collision:\n".to_string();
            for c in colliding {
                mess += &c;
                mess += ",";
            }
            self.push(rng,&mess[0..mess.len()-1],self.config.flag.collisions.unwrap());
        }
    }
    fn linenum_range(&self,node: &tree_sitter::Node,source: &str) -> lsp_types::Range {
        let len = node_text(node,source).trim_end().len() as u32;
        let rng = lsp_range(node.range(),self.row,self.col);
        lsp_types::Range::new(rng.start,lsp_types::Position::new(rng.start.line,rng.start.character + len))
    }
	fn process_variable_def(&mut self,maybe_node: Option<tree_sitter::Node>, dim: bool, recall: bool) {
		if let Some(node) = maybe_node {
            let [keyname,cased] = super::var_to_key(node,recall,&self.line);
            let map = match keyname.ends_with(")") || recall {
                true => self.symbols.arrays.borrow_mut(),
                false => self.symbols.scalars.borrow_mut()
            };
            if !map.contains_key(&keyname) {
                map.insert(keyname.clone(),Variable::new());
            }
            if let Some(var_info) = map.get_mut(&keyname) {
                if dim {
                    log::trace!("declaring {}",&keyname);
                    var_info.decs.push(lsp_range(name_range(node),self.row,self.col));
                } else {
                    log::trace!("defining {}",&keyname);
                    var_info.defs.push(lsp_range(name_range(node),self.row,self.col));
                }
                var_info.case.insert(cased);
            }
        }
	}
    /// gathers primary symbol information
    fn visit_primaries(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.depth_of_def {
			self.depth_of_def = 0;
			self.dummy_var_key = "".to_string();
		}
		let parent = curs.node().parent();
		let rng = curs.node().range();
		if curs.node().is_error() {
			return Ok(Navigation::GotoSibling);
        }
		if curs.node().kind() == "linenum" && parent.is_some() && parent.unwrap().kind() == "line" {
			let mut next_statement = curs.node().next_named_sibling();
			let mut remark: Option<String> = None;
			while next_statement.is_some() {
				if let Some(child) = next_statement.unwrap().named_child(0) {
                    if child.kind() == "tok_rem" {
                        if let Some(txt) = child.next_named_sibling() {
                            remark = Some(node_text(&txt,&self.line));
                        }
                    }
                }
				next_statement = next_statement.unwrap().next_named_sibling();
			}
			if let Some(num) = node_integer(&curs.node(),&self.line) {
                if num < 0 || num > 63999 {
                    self.push(rng, "Out of range (0,63999)",lsp::DiagnosticSeverity::ERROR);
                } else if num <= self.last_good_line_number {
                    self.push(rng, "Line number out of order",lsp::DiagnosticSeverity::ERROR);
                } else {
                    self.symbols.lines.insert(num, Line {
                        rem: remark,
                        primary: self.linenum_range(&curs.node(),&self.line),
                        gosubs: Vec::new(),
                        gotos: Vec::new()
                    });
                    self.last_good_line_number = num;
                }
            }
			return Ok(Navigation::GotoSibling);
		}
		else if curs.node().kind() == "tok_dim" {
			return Ok(Navigation::GotoSibling); // goto dim_item
        }
		else if curs.node().kind() == "dim_item" {
			self.process_variable_def(curs.node().named_child(0), true, false);
			return Ok(Navigation::GotoSibling);
		}
		else if curs.node().kind() == "tok_def" {
			// dummy variable is not needed during this pass
            self.depth_of_def = curs.depth();
            if let Some(n1) = curs.node().next_named_sibling() { // tok_fn
                if let Some(n2) = n1.next_named_sibling() { // name_fn
                    let [keyname,cased] = super::var_to_key(n2, false, &self.line);
                    if self.symbols.functions.contains_key(&keyname) {
                        self.diagnostics.push(self.create(n2.range(),"function is redefined",lsp::DiagnosticSeverity::ERROR));
                    } else {
                        let mut var_info = Variable::new();
                        var_info.defs.push(lsp_range(n2.range(),self.row,self.col));
                        var_info.case.insert(cased);
                        self.symbols.functions.insert(keyname,var_info);
                    }
                    return Ok(Navigation::GotoParentSibling);
                }
            }
		}
		else if curs.node().kind() == "assignment" {
			if let Some(mut next) = curs.node().named_child(0) {
                while !next.kind().starts_with("var_") {
                    if let Some(n) = next.next_named_sibling() {
                        next = n;
                    } else {
                        warn!("assignment parsed wrong");
                        return Ok(Navigation::GotoParentSibling);
                    }
                }
				self.process_variable_def(Some(next), false, false);
            }
		}
		else if curs.node().kind() == "tok_recall" {
            if let Some(var_node) = curs.node().next_named_sibling() {
                if var_node.kind().starts_with("var_") {
                    self.process_variable_def(Some(var_node), false, true);
                }
            }
		}
		else if ["tok_read","tok_get","tok_input"].contains(&curs.node().kind()) {
            let mut var_node = curs.node().next_named_sibling();
            while var_node.is_some() {
                if var_node.unwrap().kind().starts_with("var_") {
                    self.process_variable_def(var_node, false, false);
                }
                var_node = var_node.unwrap().next_named_sibling();
            }
		}
		else if curs.node().kind() == "tok_for" {
            if let Some(var_node) = curs.node().next_named_sibling() {
                if var_node.kind().starts_with("var_") {
                    self.process_variable_def(Some(var_node), false, false);
                }
            }
		}
		// this determines how deep in the tree we need to go
		else if curs.depth() < 4 {
			return Ok(Navigation::GotoChild);
        }
		
		return Ok(Navigation::GotoParentSibling);
    }
	fn value_range(&mut self,node: tree_sitter::Node, low: f64, high: f64, expect_int: bool) {
        if node.kind()=="real" && expect_int {
            self.push(node.range(),"Expected an integer",lsp::DiagnosticSeverity::ERROR);
        }
		if node.kind()=="int" || node.kind()=="real" || node.kind()=="unary_aexpr" {
            match node_text(&node,&self.line).replace(" ","").parse::<f64>() {
                Ok(parsed) => if parsed < low || parsed > high {
                    let mut mess = "Out of range (".to_string();
                    match node.kind()=="real" {
                        true => write!(&mut mess,"{low:e},{high:e})").expect("could not format"),
                        false => write!(&mut mess,"{low},{high})").expect("could not format")
                    };
                    self.push(node.range(),&mess,lsp::DiagnosticSeverity::ERROR);
                },
                Err(_) => warn!("parsing numerical node failed {}",node_text(&node,&self.line))
            }
        }
	}
    fn check_case(&mut self, node: tree_sitter::Node, rng: tree_sitter::Range) {
        let txt = node_text(&node,&self.line);
        if self.config.flag.case_sensitive.is_some() && txt.to_uppercase() != txt {
            self.push(rng, "settings require upper case", self.config.flag.case_sensitive.unwrap());
        }
    }
    fn process_linenum_ref(&mut self, curs: &tree_sitter::TreeCursor, is_sub: bool) -> Result<Navigation,DYNERR> {
		let mut next = Some(curs.node());
		if next.unwrap().kind() != "linenum" { // we might start on GOSUB node
			next = next.unwrap().next_named_sibling();
        }
		while next.is_some() && next.unwrap().kind() == "linenum" {
            let node = next.unwrap();
            let rng = lsp_range(node.range(), self.row, self.col);
			let line = match node_integer(&node,&self.line) {
                Some(x) => self.symbols.lines.get_mut(&x),
                _ => None
            };
            let ip_branch = self.flow.eval_ip_branch(&node);
            if ip_branch.is_some() {
                self.diagnostics.push(ip_branch.unwrap());
            } else if line.is_some() {
                if is_sub {
                    line.unwrap().gosubs.push(rng);
                } else {
                    line.unwrap().gotos.push(rng);
                }
			} else if node.parent().is_some() && node.parent().unwrap().has_error()	{
				self.diagnostics.push(self.create(node.range(), "Maybe unanalyzed (fix line)", lsp::DiagnosticSeverity::WARNING));
				return Ok(Navigation::GotoSibling);
			} else if self.config.flag.bad_references.is_some() {
				self.diagnostics.push(self.create(node.range(), "Line does not exist",self.config.flag.bad_references.unwrap()));
            }
			next = node.next_named_sibling();
		}
		return Ok(Navigation::GotoParentSibling);
	}
	fn process_variable_ref(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>	{
        if curs.node().has_error() {
            return Ok(Navigation::GotoSibling);
        }
		let is_recall = match curs.node().prev_named_sibling() {
            Some(sib) => sib.kind()=="tok_recall",
            None => false
        };
		let [keyname,cased] = super::var_to_key(curs.node(),is_recall,&self.line);
		let name_range = name_range(curs.node());
        let not_dummy: bool = self.depth_of_def == 0 || keyname != self.dummy_var_key;
		let is_array = keyname.ends_with(")") || is_recall;
		if self.config.flag.collisions.is_some() {
			self.collision(&keyname, curs.node().range(),false);
        }
        if is_array {
            if !self.symbols.arrays.contains_key(&keyname) {
                self.symbols.arrays.insert(keyname.clone(),Variable::new());
            }
            let var_info = self.symbols.arrays.get_mut(&keyname).unwrap();
            var_info.push_ref_selectively(lsp_range(name_range, self.row, self.col));
            var_info.case.insert(cased);
            if var_info.decs.len() == 0 && self.config.flag.undeclared_arrays.is_some() {
                self.diagnostics.push(self.create(name_range, "array is never DIM'd", self.config.flag.undeclared_arrays.unwrap()));
            }
        } else {
            if !self.symbols.scalars.contains_key(&keyname) {
                self.symbols.scalars.insert(keyname.clone(),Variable::new());
            }
            let var_info = self.symbols.scalars.get_mut(&keyname).unwrap();
            var_info.push_ref_selectively(lsp_range(name_range, self.row, self.col));
            var_info.case.insert(cased);
            if var_info.defs.len() == 0 && not_dummy && self.config.flag.undefined_variables.is_some() {
                self.diagnostics.push(self.create(name_range, "variable is never assigned", self.config.flag.undefined_variables.unwrap()));
            }
        }
		if let Some(name_node) = curs.node().named_child(0) {
            self.check_case(name_node, name_node.range());
        }
        match is_array {
            true => Ok(Navigation::GotoChild),
            false => Ok(Navigation::GotoSibling)
        }
	}
    fn visit_node(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.depth_of_def {
			self.depth_of_def = 0;
			self.dummy_var_key = "".to_string();
		}
		let parent = curs.node().parent();
		let rng = curs.node().range();
        if curs.node().is_missing() {
            let mut mess = "something is missing".to_string();
            let (node,more) = match curs.node().parent() {
                Some(p) => (p,[" after ",&p.to_sexp()].concat()),
                None => (curs.node(),"".to_string())
            };
            mess += &more;
            let syn_rng = extended_range(&node, self.line.len());
            self.push(syn_rng, &mess, lsp::DiagnosticSeverity::ERROR);
            return Ok(Navigation::GotoSibling);
        }
        if curs.node().is_error() {
            self.push(curs.node().range(), &("syntax error: ".to_string() + &curs.node().to_sexp()), lsp::DiagnosticSeverity::ERROR);
            return Ok(Navigation::GotoSibling);
        }
		if curs.node().kind() == "linenum" && parent.is_some() && parent.unwrap().kind() == "line" {
            self.flow.curr_line_num = node_integer::<u16>(&curs.node(),&self.line);
			return Ok(Navigation::GotoSibling);
		}
		if curs.node().kind().starts_with("tok_") {
			self.check_case(curs.node(), rng);
        }
		if curs.node().kind() == "tok_gosub" {
			return self.process_linenum_ref(curs, true);
        } else if curs.node().kind() == "linenum" && parent.is_some() && parent.unwrap().kind() != "line" { // any ref that is not GOSUB
			return self.process_linenum_ref(curs, false);
        } else if curs.node().kind() == "tok_def" {
            let dummy_var_node = crate::lang::named_sibling(curs.node(), 3);
            if let Some(dummy) = dummy_var_node {
                if dummy.kind() == "var_real" {
                    self.dummy_var_key = super::var_to_key(dummy, false, &self.line)[0].clone();
                }
            }
			self.depth_of_def = curs.depth();
		} else if curs.node().kind().starts_with("var_") {
			return self.process_variable_ref(curs);
        } else if curs.node().kind() == "name_fn" {
			self.check_case(curs.node(), rng);
			let [keyname,cased] = super::var_to_key(curs.node(),false,&self.line);
			if self.config.flag.collisions.is_some() {
				self.collision(&keyname, rng, true);
            }
            if !self.symbols.functions.contains_key(&keyname) {
                self.symbols.functions.insert(keyname.clone(),Variable::new());
            }
			let var_info = self.symbols.functions.get_mut(&keyname).unwrap();
			var_info.push_ref_selectively(lsp_range(rng, self.row, self.col));
			var_info.case.insert(cased);
            if var_info.defs.len()==0 && parent.is_some() && parent.unwrap().kind() == "fcall" {
                self.diagnostics.push(self.create(rng, "function never defined",lsp::DiagnosticSeverity::ERROR));
            }
		} else if curs.node().kind() == "real" {
			self.check_case(curs.node(), rng);
        } else if curs.node().kind()=="line" {
			if node_text(&curs.node(), &self.line).len() > 239 {
				self.push(rng,"Maximum length of a line is 239",lsp::DiagnosticSeverity::ERROR);
            }
		} else if curs.node().kind()=="assignment" {
            let mut idx = 0;
            if let Some(first) = curs.node().named_child(0) {
                if first.kind() == "tok_let" {
                    idx = 1;
                }
            }
			let lhs = curs.node().named_child(idx);
			let rhs = curs.node().named_child(idx+2);
			if lhs.is_some() && rhs.is_some()
			{
				if lhs.unwrap().kind()=="var_real" {
					self.value_range(rhs.unwrap(),-1.7e38,1.7e38,false);
                } else if lhs.unwrap().kind()=="var_int" {
					self.value_range(rhs.unwrap(),-32767.,32767.,true);
                }
			}
		}
		else if curs.node().kind()=="tok_poke"
		{
			if let Some(addr) = curs.node().next_named_sibling() {
                self.flow.eval_ip_start(&addr);
				self.value_range(addr,-32767.,65535.,true);
				if let Some(byte) = addr.next_named_sibling() {
					self.value_range(byte,0.,255.,true);
                }
			}
		}
        else if curs.node().kind() == "tok_print" {
            if let Some(diag) = self.flow.eval_ip_run(&curs.node()) {
                self.diagnostics.push(diag);
            }
        }
        else if curs.node().kind() == "tok_call" {
            if let Some(addr) = curs.node().next_named_sibling() {
				self.value_range(addr,-32767.,65535.,true);
            }
            if let Some(parent) = curs.node().parent() {
                if parent.named_child_count() == 3 {
                    // if CHAIN pattern always accept
                    let addr = node_integer(&parent.named_child(1).unwrap(), &self.line);
                    let str = parent.named_child(2).unwrap();
                    let chain = match (addr,str.kind()) {
                        (Some(520),"str") => true,
                        _ => false
                    };
                    if chain {
                        self.push(parent.range(),"CHAIN pattern",lsp::DiagnosticSeverity::INFORMATION);
                        return Ok(Navigation::GotoSibling);
                    }
                }
                if parent.named_child_count() > 2 && self.config.flag.extended_call.is_some() {
                    let mut arg_rng = parent.named_child(2).unwrap().range();
                    arg_rng.end_byte = rng.end_byte;
                    arg_rng.end_point = rng.end_point;
                    self.push(parent.range(), "extended CALL syntax",self.config.flag.extended_call.unwrap());
                }
            }
        }
        else if ["tok_peek","tok_himem","tok_lomem"].contains(&curs.node().kind()) {
            if let Some(addr) = curs.node().next_named_sibling() {
				self.value_range(addr,-32767.,65535.,true);
            }
		}
		else if ["tok_coloreq","tok_speedeq"].contains(&curs.node().kind()) {
			if let Some(byte) = curs.node().next_named_sibling() {
                self.value_range(byte,0.,255.,true);
            }
		}
		else if curs.node().kind()=="tok_hcoloreq"
		{
			if let Some(v) = curs.node().next_named_sibling() {
                self.value_range(v,0.,7.,true);
            }
		}
		else if curs.node().kind()=="tok_wait"
		{
			if let Some(addr) = curs.node().next_named_sibling() {
				self.value_range(addr,-32767.,65535.,true);
				if let Some(mask) = addr.next_named_sibling() {
                    self.value_range(mask,0.,255.,true);
					if let Some(val) = mask.next_named_sibling() {
                        self.value_range(val,0.,255.,true);
                    }
				}
			}
		}
		else if curs.node().kind()=="data_literal" // perhaps parser should handle this
		{
			if let Some(data_st) = curs.node().parent()	{
				if let Some(sib) = data_st.next_sibling() {
                    let txt = node_text(&curs.node(),&self.line);
                    let next = node_text(&sib, &self.line);
					if next.trim()==":" && (txt.split("\"").count()+1)%2 == 1 {
						self.push(rng,"Odd quote parity in literal on multi-statement line invites trouble.",lsp::DiagnosticSeverity::WARNING);
                    }
				}
			}
		}
		else if curs.node().kind()=="str" && !node_text(&curs.node(),&self.line).ends_with("\"") && self.config.flag.terminal_string.is_some() {
			self.push(rng,"Unquote missing. This is valid if it is intended.",self.config.flag.terminal_string.unwrap());
		}
		else if curs.node().kind() == "tok_onerr" && parent.is_some() {
			let mut next_statement = parent.unwrap().next_named_sibling();
			while next_statement.is_some() {
				let is_rem = match next_statement.unwrap().named_child(0) {
                    Some(first_child) => first_child.kind() == "tok_rem",
                    None => false
                };
                if !is_rem {
					self.push(next_statement.unwrap().range(), "Statements trailing ONERR GOTO on the same line are ignored", lsp::DiagnosticSeverity::WARNING);
                    next_statement = next_statement.unwrap().next_named_sibling();
                }
			}
		}
		return Ok(Navigation::GotoChild);
    }
}