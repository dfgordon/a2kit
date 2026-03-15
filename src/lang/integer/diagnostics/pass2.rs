use tree_sitter;
use lsp_types::DiagnosticSeverity;
use crate::lang::{lsp_range,extended_range,node_text,node_integer};
use super::super::{Variable,var_to_key};
use crate::lang::Navigation;
use crate::DYNERR;

impl super::Analyzer {
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
		let (keyname,cased,is_array,is_string) = var_to_key(curs.node(),&self.line);
        let mut ws_decs = 0;
        let mut ws_defs = 0;
        for backlink in &self.curr_backlinks {
            if let Some(ws_syms) = self.get_workspace().ws_symbols.get(backlink) {
                if let Some(var) = ws_syms.vars.get(&keyname) {
                    ws_decs += var.decs.len();
                    ws_defs += var.defs.len();
                }
            }
        }
        let name_range = lsp_range(curs.node().range(), self.row, self.col);
        if !self.symbols.vars.contains_key(&keyname) {
            self.symbols.vars.insert(keyname.clone(),Variable::new());
        }
        let var_info = self.symbols.vars.get_mut(&keyname).unwrap();
        var_info.is_array = is_array;
        var_info.is_string = is_string;
        var_info.push_ref_selectively(name_range);
        var_info.case.insert(cased);
        let is_declared = var_info.decs.len() + ws_decs > 0;
        let is_defined = var_info.defs.len() + ws_defs > 0;
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
    pub fn visit_node(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.saved_depth {
			self.saved_depth = 0;
			self.in_dim_statement = false;
		}
		let rng = curs.node().range();
        if self.config.flag.case_sensitive.is_some() {
            for chk in super::CASE_CHECK {
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
        else if curs.node().kind() == "statement_print_str" {
            if let Some(prog) = super::chain::test_chain(&curs.node(),&self.line) {
                self.push(rng,&format!("DOS CHAIN detected: {}",prog),DiagnosticSeverity::INFORMATION);
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
		else if super::super::SIMPLE_VAR_TYPES.contains(&curs.node().kind()) {
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