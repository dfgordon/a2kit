use tree_sitter;
use lsp_types::{self as lsp};
use std::borrow::BorrowMut;
use crate::lang::lsp_range;
use super::{Line,Variable,Symbols,name_range,node_text,node_integer};
use super::super::{var_to_key,Collisions};
use crate::lang::Navigation;
use crate::DYNERR;

fn process_variable_def(symbols: &mut Symbols, ws_coll: Option<&mut Collisions>, maybe_node: Option<tree_sitter::Node>, line: &str, row: isize, col: isize, dim: bool, recall: bool) {
    if let Some(node) = maybe_node {
        let [keyname,cased] = var_to_key(node,recall,line);
        if let Some(collisions) = ws_coll {
            super::update_collisions(collisions, &keyname);
        }
        let map = match keyname.ends_with(")") || recall {
            true => symbols.arrays.borrow_mut(),
            false => symbols.scalars.borrow_mut()
        };
        if !map.contains_key(&keyname) {
            map.insert(keyname.clone(),Variable::new());
        }
        if let Some(var_info) = map.get_mut(&keyname) {
            if dim {
                log::trace!("declaring {}",&keyname);
                var_info.decs.push(lsp_range(name_range(node),row,col));
            } else {
                log::trace!("defining {}",&keyname);
                var_info.defs.push(lsp_range(name_range(node),row,col));
            }
            var_info.case.insert(cased);
        }
    }
}

/// Visitor function to gather definitions and declarations, used in both workspace and program scans.
/// Function definitions are *not* gathered here.
/// If Some is returned the caller is advised to immediately return the enclosed navigation directive.
/// If None is returned the caller can continue evaluating the cursor position.
pub fn visit_defs_and_decs(symbols: &mut Symbols, ws_coll: Option<&mut Collisions>, curs: &tree_sitter::TreeCursor, line: &str, row: isize, col: isize) -> Option<Navigation> {
    if curs.node().kind() == "tok_dim" {
        return Some(Navigation::GotoSibling); // goto dim_item
    }
    else if curs.node().kind() == "dim_item" {
        process_variable_def(symbols, ws_coll,curs.node().named_child(0),line,row,col, true, false);
        return Some(Navigation::GotoSibling);
    }
    else if curs.node().kind() == "assignment" {
        if let Some(mut next) = curs.node().named_child(0) {
            while !next.kind().starts_with("var_") {
                if let Some(n) = next.next_named_sibling() {
                    next = n;
                } else {
                    log::warn!("assignment parsed wrong");
                    return Some(Navigation::GotoParentSibling);
                }
            }
            process_variable_def(symbols, ws_coll,Some(next),line,row,col, false, false);
        }
    }
    else if curs.node().kind() == "tok_recall" {
        if let Some(var_node) = curs.node().next_named_sibling() {
            if var_node.kind().starts_with("var_") {
                process_variable_def(symbols, ws_coll,Some(var_node),line,row,col, false, true);
            }
        }
    }
    else if ["tok_read","tok_get","tok_input"].contains(&curs.node().kind()) {
        let mut var_node = curs.node().next_named_sibling();
        // this ugliness keeps the borrow checker happy about the loop, is there a better way?
        let mut dummy = std::collections::HashMap::new();
        let collisions = match ws_coll {
            Some(coll) => coll,
            None => &mut dummy
        };
        while var_node.is_some() {
            if var_node.unwrap().kind().starts_with("var_") {
                process_variable_def(symbols, Some(collisions),var_node,line,row,col, false, false);
            }
            var_node = var_node.unwrap().next_named_sibling();
        }
    }
    else if curs.node().kind() == "tok_for" {
        if let Some(var_node) = curs.node().next_named_sibling() {
            if var_node.kind().starts_with("var_") {
                process_variable_def(symbols, ws_coll,Some(var_node),line,row,col, false, false);
            }
        }
    }
    None
}

impl super::Analyzer {
    /// gathers primary symbol information
    pub fn visit_primaries(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.depth_of_def {
			self.depth_of_def = 0;
			self.dummy_var_key = "".to_string();
		}
		let rng = curs.node().range();
		if curs.node().is_error() {
            return match curs.depth() {
                x if x < 2 => Ok(Navigation::GotoChild),
                _ => Ok(Navigation::GotoSibling)
            }
        }
        if let Some(nav) = visit_defs_and_decs(&mut self.symbols, None, curs, &self.line, self.row, self.col) {
            return Ok(nav);
        }
		if curs.node().kind() == "linenum" && curs.depth() == 2 {
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
		else if curs.node().kind() == "tok_def" {
			// dummy variable is not needed during this pass
            self.depth_of_def = curs.depth();
            if let Some(n1) = curs.node().next_named_sibling() { // tok_fn
                if let Some(n2) = n1.next_named_sibling() { // name_fn
                    let [keyname,cased] = var_to_key(n2, false, &self.line);
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
		// this determines how deep in the tree we need to go
		else if curs.depth() < 4 {
			return Ok(Navigation::GotoChild);
        }
		
		return Ok(Navigation::GotoParentSibling);
    }
}