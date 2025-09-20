use lsp_types as lsp;
use tree_sitter::{TreeCursor,Node};
use crate::lang::merlin::context::Context;
use super::super::{Symbols,Workspace,SourceType};
use super::super::symbol_flags as flg;
use crate::lang::merlin::MerlinVersion;
use crate::lang::server::basic_diag;
use crate::lang::{Navigation,node_text,lsp_range};
use crate::DYNERR;

const MACRO_AVERSE: [&str;6] = ["psop_ent","psop_ext","psop_exd","psop_put","psop_use","psop_sav"];
const FOLDS: [&str;11] = ["psop_do","psop_if","psop_else","psop_fin","psop_lup","psop_end_lup","psop_mac","psop_eom","psop_dum","psop_dend","psop_end"];

/// Add diagnostics for the path appearing in PUT or USE
fn verify_include_path(curs: &TreeCursor, ctx: &mut Context, ws: &Workspace) -> Vec<lsp::Diagnostic>
{
    let mut ans = Vec::new();
    if let Some(path_node) = curs.node().next_named_sibling() {
        if let Some(src) = ctx.curr_source() {
            let rng = lsp_range(path_node.range(), src.row, src.col);
            let doc_uris = ws.get_include_doc(&curs.node(), ctx.line());
            if doc_uris.len() == 0 {
                ans.push(basic_diag(rng, "file not found in workspace", lsp::DiagnosticSeverity::ERROR));
            }
            if doc_uris.len() > 1 {
                ans.push(basic_diag(rng, &format!("multiple matches ({}) could not be resolved",doc_uris.len()), lsp::DiagnosticSeverity::ERROR));
            }
        }
    }
    ans
}

fn visit_verify_macro_def(node: &Node, loc: lsp::Location, ctx: &mut Context, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
-> Result<Navigation,DYNERR> {
    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };
    let child = node.named_child(0);
    let (rng,txt) = ctx.node_spec(&node);
    if node.kind()=="macro_def" {
        match (ctx.unused_macros_in_context_setting(),ctx.unused_macros_setting(),ctx.is_include()) {
            (Some(severity),_,true) => {
                match symbols.is_macro_referenced(&txt,15) {
                    Ok(true) => {},
                    Ok(false) => push(rng,"macro is not referenced in current context",severity),
                    Err(_) => push(rng,"nesting depth exceeds 15",lsp::DiagnosticSeverity::ERROR)
                }
            },
            (_,Some(severity),false) => {
                match symbols.is_macro_referenced(&txt,15) {
                    Ok(true) => {},
                    Ok(false) => push(rng,"macro is never referenced",severity),
                    Err(_) => push(rng,"nesting depth exceeds 15",lsp::DiagnosticSeverity::ERROR)
                }
            },
            _ => {}
        }
        let is_new_mac_scope = match ctx.curr_scope() {
            Some(scope) => scope.flags & flg::MAC == 0,
            None=> true
        };
        ctx.enter_scope(&txt,symbols);
        if is_new_mac_scope {
            ctx.trigs.push_vars = true;
            ctx.trigs.unset_vars = true;
        }
    } else if node.kind()=="macro_ref" {
        ctx.trigs.unset_vars = true;
        ctx.trigs.unset_children = true;
    } else if node.kind()=="psop_eom" { // also processed by folds
        ctx.exit_scope(symbols);
        ctx.trigs.pop_vars = true;
        return Ok(Navigation::GotoSibling);
    } else if node.kind() == "psop_mx" {
        if let Some(arg) = node.next_named_sibling() {
            super::update_var_value(&txt, &arg, symbols, ctx.line(), ctx.curr_scope());
        }
    } else if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_declared_or_defined(&txt);
            let is_mac_loc = symbols.child_defined(&txt, ctx.curr_scope().as_ref().unwrap());
            if is_glob && is_mac_loc {
                push(rng,"macro local shadows global",lsp::DiagnosticSeverity::WARNING);
            }
            if is_mac_loc && symbols.child_forward(&txt,ctx.curr_scope().as_ref().unwrap(),&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            } else if is_glob && symbols.global_forward(&txt,&loc) {
                push(rng,"forward reference check deferred",lsp::DiagnosticSeverity::HINT);
            } else if !is_glob && !is_mac_loc {
                if let Some(mac) = ctx.curr_scope() {
                    match symbols.is_macro_referenced(&mac.name, 15) {
                        Ok(true) => push(rng,"label is undefined in this scope",lsp::DiagnosticSeverity::ERROR),
                        _ => {}
                    }
                }
            }
        } else if ck=="local_label" {
            push(rng, "cannot use local labels in a macro", lsp::DiagnosticSeverity::ERROR);
        } else if ck == "var_label" {
            if !symbols.var_defined(&txt) && child.unwrap().named_child(0).is_none() {
                if let Some(mac) = ctx.curr_scope() {
                    match symbols.is_macro_referenced(&mac.name, 15) {
                        Ok(true) => push(rng,"variable is undefined",lsp::DiagnosticSeverity::ERROR),
                        _ => {}
                    }
                }
            }
        }
        return Ok(Navigation::GotoSibling)
    } else if child.is_some() && node.kind()=="label_def" {
        if child.unwrap().kind()=="local_label" {
            push(rng,"cannot use local labels in a macro",lsp::DiagnosticSeverity::ERROR);
        }
        return Ok(Navigation::GotoSibling)
    } else if MACRO_AVERSE.contains(&node.kind()) {
        push(rng,"pseudo operation cannot be used in a macro",lsp::DiagnosticSeverity::ERROR);
    }
    return Ok(Navigation::GotoChild);
}

/// Verify labels assuming gather pass has been completed
pub fn visit_verify(curs: &TreeCursor, ctx: &mut Context, ws: &Workspace, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
    -> Result<Navigation,DYNERR>
{
    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };
    let src = match ctx.curr_source() {
        Some(s) => s,
        None => return Ok(Navigation::Exit)
    };
    let node = curs.node();
    let (rng,txt) = ctx.node_spec(&node);
    let loc = lsp::Location::new(src.doc.uri.clone(),rng);
    let child = node.named_child(0);
    let (in_macro_def,in_global) = match ctx.curr_scope() {
        Some(s) => (s.flags & flg::MAC > 0, s.flags & flg::MAC == 0),
        None => (false,false)
    };
 
    if FOLDS.contains(&node.kind()) {
        let arg = super::eval_fold_expr(&node, None, symbols, ctx, in_macro_def, None);
        ctx.folding_range(&node.kind(), rng, loc.clone(), arg, None);
    }
    
    let (asm,_gen,is_end) = ctx.cond_asm();
    if is_end {
        return Ok(Navigation::Exit);
    }
    if in_macro_def || node.kind()=="macro_def" {
        return visit_verify_macro_def(&node, loc, ctx, symbols, diagnostics);
    }
    if !asm {
        // have to keep looking for folds
        return Ok(Navigation::GotoChild);
    }
    if node.kind() == "label_ref" && node.parent().unwrap().kind() == "arg_ent" {
        if !symbols.global_defined(&txt) {
            push(rng,"entry label declared, but never defined",lsp::DiagnosticSeverity::WARNING);
        }
    } else if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_declared_or_defined(&txt);
            if !is_glob {
                push(rng, "global label is undefined", lsp::DiagnosticSeverity::ERROR);
            } else if is_glob && symbols.global_forward(&txt,&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            }
        } else if ck=="local_label" {
            if in_global && !symbols.child_defined(&txt, ctx.curr_scope().as_ref().unwrap()) {
                push(rng,"local label is not defined in this scope",lsp::DiagnosticSeverity::ERROR);
            } else if in_global && symbols.child_forward(&txt, ctx.curr_scope().as_ref().unwrap(),&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            }
        } else if ck == "var_label" {
            if let Some(grandchild) = child.unwrap().named_child(0) {
                if grandchild.kind()=="var_mac" {
                    push(rng, "macro substitution variable referenced outside macro", lsp::DiagnosticSeverity::ERROR);
                } else if ctx.merlin_version()!=MerlinVersion::Merlin8 && grandchild.kind()=="var_cnt" {
                    push(rng, "argument count referenced outside macro", lsp::DiagnosticSeverity::ERROR);
                }
            } else if !symbols.var_defined(&txt) {
                push(rng,"variable is undefined",lsp::DiagnosticSeverity::ERROR);
            } else if symbols.var_forward(&txt,&loc) {
                push(rng,"variable is forward referenced",lsp::DiagnosticSeverity::WARNING);
            }
        }
        return Ok(Navigation::GotoSibling)
    }
    // handle `var_mac` occurrences that have no `var_label` parent
    else if node.kind() == "var_mac" {
        push(rng, "macro substitution variable referenced outside macro", lsp::DiagnosticSeverity::ERROR);
    } else if child.is_some() && node.kind()=="label_def" {
        match (ctx.unused_labels_in_context_setting(),ctx.unused_labels_setting(),ctx.is_include()) {
            (Some(severity),_,true) => {
                if !symbols.is_label_referenced_or_ent(&txt, ctx.curr_scope()) {
                    push(rng,"label is not referenced in current context",severity);
                }
            },
            (_,Some(severity),false) => {
                if !symbols.is_label_referenced_or_ent(&txt, ctx.curr_scope()) {
                    push(rng,"label is never referenced",severity);
                }
            },
            _ => {}
        }
        let ck = child.unwrap().kind();
        if ck == "global_label" {
            ctx.enter_scope(&txt,symbols);
        } else if ck=="local_label" {
            if let Some(next) = node.next_named_sibling() {
                if next.kind()=="psop_mac" || next.kind()=="psop_ent" || next.kind()=="psop_ext" || next.kind()=="psop_equ" {
                    push(rng,&format!("cannot use local label for {}",node_text(&next, ctx.line())),lsp::DiagnosticSeverity::ERROR);
                }
            }
        } else if ck=="var_label" {
            super::update_var_value(&txt, &node, symbols, ctx.line(), ctx.curr_scope());
        }
        return Ok(Navigation::GotoSibling)
    } else if node.kind()=="macro_ref" {
        if !symbols.mac_defined(&txt) && symbols.global_declared_or_defined(&txt) {
            push(rng,"expected macro, this is a label",lsp::DiagnosticSeverity::ERROR);
        } else if !symbols.mac_defined(&txt) {
            push(rng,"macro is undefined",lsp::DiagnosticSeverity::ERROR);
        } else if symbols.mac_forward(&txt,&loc) {
            push(rng,"macro is forward referenced",lsp::DiagnosticSeverity::ERROR);
        } else {
            super::macros::check_macro_args(&node, symbols, ctx, diagnostics);
            ctx.trigs.unset_vars = true;
        }
    } else if node.kind() == "psop_use" || node.kind() == "psop_put" {
        let psop = node.kind()[5..8].to_string().to_uppercase();
        if src.typ != SourceType::Master && src.typ != SourceType::Module {
            push(rng, &format!("recursive {} is not allowed",psop), lsp::DiagnosticSeverity::ERROR);
            diagnostics.append(&mut verify_include_path(&curs,ctx,ws));
        } else {
            diagnostics.append(&mut verify_include_path(&curs,ctx,ws));
            return Ok(Navigation::Descend);
        }
    } else if node.kind() == "psop_mx" {
        if let Some(arg) = node.next_named_sibling() {
            super::update_var_value(&txt, &arg, symbols, ctx.line(), ctx.curr_scope());
        }
    } else if asm && node.kind()=="psop_end_lup" {
        ctx.trigs.unset_vars = true;
    }
    return Ok(Navigation::GotoChild);
}    
