use tree_sitter;
use lsp_types::DiagnosticSeverity;
use crate::lang::{node_text,node_integer};
use super::super::{Line,Variable,Symbols,lexpr_to_key};
use crate::lang::Navigation;
use crate::DYNERR;

fn process_variable_defs(symbols: &mut Symbols,maybe_node: Option<tree_sitter::Node>, nmax: usize, line: &str, row: isize, in_dim: bool) {
    if let Some(node) = maybe_node {
        let mut maybe_next = Some(node);
        let mut num_found = 0;
        while let Some(next) = maybe_next {
            if num_found >= nmax {
                return;
            }
            if super::super::LEXPR.contains(&next.kind()) {
                num_found += 1;
                if let Ok((keyname, cased, is_array, is_string, rng)) = lexpr_to_key(next,row,line) {
                    if !symbols.vars.contains_key(&keyname) {
                        symbols.vars.insert(keyname.clone(),Variable::new());
                    }
                    let var_info = symbols.vars.get_mut(&keyname).unwrap();
                    var_info.is_array = is_array;
                    var_info.is_string = is_string;
                    if in_dim {
                        var_info.decs.push(rng);
                    }
                    else {
                        var_info.defs.push(rng);
                    }
                    var_info.case.insert(cased);
                }
            }
            if next.kind() == "open_dim_str" || next.kind() == "open_dim_int" {
                maybe_next = super::super::pass_through_subscript(next,line);
            } else {
                maybe_next = next.next_named_sibling();
            }
        }
    }
}

/// Visitor function to gather definitions and declarations, used in both workspace and program scans.
/// If Some is returned the caller is advised to immediately return the enclosed navigation directive.
/// If None is returned the caller can continue evaluating the cursor position.
pub fn visit_defs_and_decs(symbols: &mut Symbols, curs: &tree_sitter::TreeCursor, line: &str, row: isize, in_dim: bool) -> Option<Navigation> {
    if curs.node().kind().starts_with("statement_dim_") {
        process_variable_defs(symbols,Some(curs.node()), 64, line, row, in_dim);
        return Some(Navigation::GotoParentSibling);
    }
    else if curs.node().kind().starts_with("assignment_") {
        process_variable_defs(symbols,curs.node().named_child(0), 1, line, row, in_dim);
        return Some(Navigation::GotoParentSibling);
    }
    else if curs.node().kind().starts_with("statement_input_") {
        process_variable_defs(symbols,Some(curs.node()), 64, line, row, in_dim);
        return Some(Navigation::GotoParentSibling);
    }
    else if curs.node().kind() == "statement_for" {
        process_variable_defs(symbols,Some(curs.node()), 1, line, row, in_dim);
        return Some(Navigation::GotoParentSibling);
    }
    None
}

impl super::Analyzer {
    /// gathers primary symbol information
    pub fn visit_primaries(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
		if curs.depth() < self.saved_depth {
			self.saved_depth = 0;
			self.in_dim_statement = false;
		}
		let rng = curs.node().range();
		if curs.node().is_error() || curs.node().is_missing() {
            return match curs.depth() {
                x if x < 2 => Ok(Navigation::GotoChild),
                _ => Ok(Navigation::GotoSibling)
            }
        }
        if curs.node().kind().starts_with("statement_dim_") {
            self.in_dim_statement = true;
            self.saved_depth = curs.depth();
        }
        if let Some(nav) = visit_defs_and_decs(&mut self.symbols, curs, &self.line, self.row, self.in_dim_statement) {
            return Ok(nav);
        }
		if curs.node().kind() == "linenum" && curs.depth() == 2 {
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
                    self.symbols.lines.insert(num, Line {
                        rem: remark,
                        primary: self.linenum_range(&curs.node(),&self.line),
                        gosubs: Vec::new(),
                        gotos: Vec::new()
                    });
                    self.last_good_line_number = num as i64;
                }
            }
			return Ok(Navigation::GotoSibling);
		}
		// this determines how deep in the tree we need to go
		else if curs.depth() < 4 {
			return Ok(Navigation::GotoChild);
        }
		
		return Ok(Navigation::GotoSibling);
    }
}