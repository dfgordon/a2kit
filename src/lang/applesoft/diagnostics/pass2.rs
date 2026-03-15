
use tree_sitter;
use lsp_types::{self as lsp};
use crate::lang::{lsp_range,extended_range};
use super::{Variable,name_range,node_text,node_integer};
use super::super::var_to_key;
use crate::lang::Navigation;
use crate::DYNERR;

impl super::Analyzer {
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
            return Ok(Navigation::GotoChild);
        }
		let is_recall = match curs.node().prev_named_sibling() {
            Some(sib) => sib.kind()=="tok_recall",
            None => false
        };
		let [keyname,cased] = var_to_key(curs.node(),is_recall,&self.line);
        let mut ws_decs = 0;
        let mut ws_defs = 0;
        for backlink in &self.curr_backlinks {
            if let Some(ws_syms) = self.get_workspace().ws_symbols.get(backlink) {
                if let Some(var) = ws_syms.arrays.get(&keyname) {
                    ws_decs += var.decs.len();
                    ws_defs += var.defs.len();
                }
                if let Some(var) = ws_syms.scalars.get(&keyname) {
                    ws_decs += var.decs.len();
                    ws_defs += var.defs.len();
                }
            }
        }
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
            if var_info.decs.len() + ws_decs == 0 && self.config.flag.undeclared_arrays.is_some() {
                self.diagnostics.push(self.create(name_range, "array is never DIM'd", self.config.flag.undeclared_arrays.unwrap()));
            }
        } else {
            if !self.symbols.scalars.contains_key(&keyname) {
                self.symbols.scalars.insert(keyname.clone(),Variable::new());
            }
            let var_info = self.symbols.scalars.get_mut(&keyname).unwrap();
            var_info.push_ref_selectively(lsp_range(name_range, self.row, self.col));
            var_info.case.insert(cased);
            if var_info.defs.len() + ws_defs == 0 && not_dummy && self.config.flag.undefined_variables.is_some() {
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
    fn collision(&mut self,long_key: &String,rng: tree_sitter::Range,is_func: bool) {
        let collisions = match is_func {
            true => &mut self.fcollisions,
            false => &mut self.vcollisions
        };
        let Some(colliding) = super::update_collisions(collisions,long_key) else {
            return;
        };
        if colliding.len() > 1 {
            let mut mess = "variable name collision:\n".to_string();
            for c in colliding {
                mess += &c;
                mess += ",";
            }
            self.push(rng,&mess[0..mess.len()-1],self.config.flag.collisions.unwrap());
        }
    }
    pub fn visit_node(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
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
                    self.dummy_var_key = var_to_key(dummy, false, &self.line)[0].clone();
                }
            }
			self.depth_of_def = curs.depth();
		} else if curs.node().kind().starts_with("var_") {
			return self.process_variable_ref(curs);
        } else if curs.node().kind() == "name_fn" {
			self.check_case(curs.node(), rng);
			let [keyname,cased] = var_to_key(curs.node(),false,&self.line);
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
            if let Some((prog,num)) = super::chain::test_chain(&curs.node(),&self.line) {
                self.push(rng,&format!("ProDOS CHAIN detected: {},{}",prog,num),lsp::DiagnosticSeverity::INFORMATION);
            }
        }
        else if curs.node().kind() == "tok_call" {
            if let Some(addr) = curs.node().next_named_sibling() {
				self.value_range(addr,-32767.,65535.,true);
            }
            if let Some(parent) = curs.node().parent() {
                if parent.named_child_count() == 3 {
                    if let Some((prog,_)) = super::chain::test_chain(&curs.node(),&self.line) {
                        self.push(rng,&format!("DOS CHAIN detected: {}",prog),lsp::DiagnosticSeverity::INFORMATION);
                        return Ok(Navigation::GotoSibling);
                    }
                }
                if parent.named_child_count() > 2 && self.config.flag.extended_call.is_some() {
                    let mut arg_rng = parent.named_child(2).unwrap().range();
                    arg_rng.end_byte = parent.range().end_byte;
                    arg_rng.end_point = parent.range().end_point;
                    self.push(arg_rng, "extended CALL syntax",self.config.flag.extended_call.unwrap());
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
                }
                next_statement = next_statement.unwrap().next_named_sibling();
			}
		}
		return Ok(Navigation::GotoChild);
    }
}
