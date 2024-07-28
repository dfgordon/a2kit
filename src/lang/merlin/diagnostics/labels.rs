use std::collections::HashMap;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use super::context::Context;
use super::super::{Symbol,Symbols,Workspace,SourceType};
use super::super::symbol_flags as flg;
use crate::lang::merlin::{self, MerlinVersion};
use crate::lang::server::{path_in_workspace,basic_diag};
use crate::lang::{Navigation,node_text,lsp_range};
use crate::DYNERR;

/// Register occurence of any top level symbol allowing for good or bad forward references.
/// Looks around the node to create the symbol.  Returns the calculated symbol flags.
fn register(txt: &str, source: &str, loc: lsp::Location, in_macro: bool, node: &tree_sitter::Node, map: &mut HashMap<String,Symbol>, docstring: Option<String>, bad_fwd_ref: bool) -> u64 {
    match map.get_mut(txt) {
        Some(sym) => {
            log::trace!("add node {}",txt);
            if bad_fwd_ref {
                sym.fwd_refs.insert(loc.clone());
            }
            sym.add_node(loc, in_macro, node, source);
            sym.flags
        },
        None => {
            let mut sym = Symbol::create(loc.clone(),in_macro,node,source);
            if node.kind() == "label_def" || node.kind() == "macro_def" {
                sym.defining_code = Some(source.to_string());
                if let Some(ds) = docstring {
                    sym.docstring = ds;
                }
            }
            log::trace!("new node {}",txt);
            let returned_flags = sym.flags;
            if bad_fwd_ref {
                sym.fwd_refs.insert(loc);
            }
            map.insert(txt.to_string(),sym);
            returned_flags
        }
    }
}

/// Register occurence of any child symbol allowing for forward references.
/// This information is put on the scope stack, not the main symbol store.
/// When exiting the scope the child information is saved to the main store.
fn register_child(txt: &str, loc: lsp::Location, in_macro: bool, node: &tree_sitter::Node, ctx: &mut Context) {
    let source = ctx.line().to_owned();
    if let Some(scope) = ctx.curr_scope() {
        match scope.children.get_mut(txt) {
            Some(sym) => {
                log::trace!("add child node {}",txt);
                sym.add_node(loc,in_macro,node,&source);
            },
            None => {
                let sym = Symbol::create(loc,in_macro,node,&source);
                log::trace!("new child node {}",txt);
                scope.children.insert(txt.to_string(),sym);
            }
        }
    }
}

/// Gather symbols and check for forward references
pub fn visit_gather(curs: &TreeCursor, ctx: &mut Context, ws: &Workspace, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
    -> Result<Navigation,DYNERR> {
    let diag_count = diagnostics.len();
    let curr_proc = ctx.curr_proc();
    let vers = ctx.merlin_version();
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
    let in_macro = match ctx.curr_scope() {
        Some(scope) => scope.flags & flg::MAC > 0,
        None => false
    };
    if in_macro && node.kind() == "source_file" {
        ctx.push_macro_line();
    }
    if node.kind() == "heading" {
        let temp = match child {
            Some(n) => node_text(&n, ctx.line()),
            None => String::new()
        };
        ctx.running_docstring += &temp;
        ctx.running_docstring += "\n";
        return Ok(Navigation::Exit);
    } else if curs.depth()>1 && node.kind() != "macro_def" && node.kind() != "label_def" {
        ctx.running_docstring = String::new();
    }
    if node.kind() == "macro_def" && child.is_some() {
        if child.unwrap().kind()=="global_label" {
            if src.typ==SourceType::Put {
                push(rng,"macros are not allowed in PUT files",lsp::DiagnosticSeverity::ERROR);
            }
            if ctx.op_handbook().strong_match(&txt,&curr_proc) || ctx.psop_handbook().strong_match(&txt,&vers) {
                push(rng,"macro name matches a mnemonic", lsp::DiagnosticSeverity::WARNING);
            }
            if symbols.mac_defined(&txt) {
                push(rng,"redefinition of a macro",lsp::DiagnosticSeverity::ERROR);
            }
            if symbols.global_defined(&txt) {
                push(rng,"macro name is used previously as a label",lsp::DiagnosticSeverity::ERROR);
            }
            if diagnostics.len() == diag_count {
                register(&txt, ctx.line(), loc, in_macro, &node, &mut symbols.macros, Some(ctx.running_docstring.clone()),false);
                ctx.running_docstring = String::new();
                ctx.enter_scope(&txt,symbols);
            }
        } else {
            push(rng,"macro label needs to be global",lsp::DiagnosticSeverity::ERROR);
        }
        return Ok(Navigation::GotoSibling);
    } else if node.kind()=="psop_eom" {
        match in_macro {
            true => {
                ctx.exit_scope(symbols);
            },
            false => {
                push(rng,"unmatched end of macro (EOM terminates all preceding MAC pseudo-ops)",lsp::DiagnosticSeverity::ERROR);
            }
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind()=="label_def" {
        let ck = child.unwrap().kind();
        if ck == "global_label" && !in_macro {
            let mut err_count = 0;
            if symbols.global_defined(&txt) {
                err_count += 1;
                push(rng, "redefinition of a global label", lsp::DiagnosticSeverity::ERROR);
            }
            if symbols.mac_defined(&txt) {
                err_count += 1;
                push(rng, "label name is used previously as a macro", lsp::DiagnosticSeverity::ERROR);
            }
            if err_count == 0 {
                let f = register(&txt, ctx.line(), loc, in_macro, &node, &mut symbols.globals, Some(ctx.running_docstring.clone()),false);
                if f & merlin::symbol_flags::EXT > 0 {
                    match ws.entries.get(&txt) {
                        Some(ent) => {
                            if let Some(ext) = symbols.globals.get_mut(&txt) {
                                if ent.defs.len() > 0 {
                                    ext.docstring = format!("imported from {}",path_in_workspace(&ent.defs[0].uri, &ws.ws_folders));
                                } else if ent.refs.len() > 0 {
                                    ext.docstring = format!("referenced in {}, but definition not found",path_in_workspace(&ent.refs[0].uri, &ws.ws_folders));
                                } else {
                                    ext.docstring = format!("entry exists but there was an internal error");
                                }
                            }
                        },
                        None => push(rng,"entry was not found in workspace",lsp::DiagnosticSeverity::ERROR)
                    };
                }
                ctx.running_docstring = String::new();
                ctx.enter_scope(&txt,symbols);
            }
        } else if ck == "global_label" && in_macro {
            let scope = ctx.curr_scope().unwrap();
            if symbols.child_defined(&txt,&scope) {
                push(rng, "redefinition of a macro scoped label", lsp::DiagnosticSeverity::WARNING);
            } else if symbols.global_defined(&txt) {
                push(rng, "redefinition of a global label", lsp::DiagnosticSeverity::WARNING);
            } else {
                register_child(&txt,loc,in_macro,&node,ctx);
            }
        } else if ck == "local_label" && !in_macro {
            if let Some(scope) = ctx.curr_scope() {
                if symbols.child_defined(&txt, &scope) {
                    push(rng, "redefinition of a local label", lsp::DiagnosticSeverity::ERROR);
                } else {
                    register_child(&txt,loc,in_macro,&node,ctx);
                }
            } else {
                push(rng,"no global scope is defined yet",lsp::DiagnosticSeverity::ERROR);
            }
        } else if child.is_some() && child.unwrap().kind() == "var_label" {
            if let Some(grandchild) = child.unwrap().named_child(0) {
                if grandchild.kind()=="var_mac" {
                    push(rng, "macro substitution variable cannot label a line", lsp::DiagnosticSeverity::ERROR);
                    return Ok(Navigation::GotoSibling);
                } else if grandchild.kind()=="var_cnt" && ctx.merlin_version()!=MerlinVersion::Merlin8 {
                    push(rng, "argument count cannot label a line", lsp::DiagnosticSeverity::ERROR);
                    return Ok(Navigation::GotoSibling);
                }
            }
            if diagnostics.len() == diag_count {
                register(&txt,ctx.line(),loc,in_macro,&node,&mut symbols.vars,None,false);
            }
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "macro_ref" {
        if child.unwrap().kind()=="global_label" {
            let bad_fwd_ref = !symbols.mac_defined(&txt);
            register(&txt,ctx.line(),loc,in_macro,&node,&mut symbols.macros,None,bad_fwd_ref);
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "label_ref" {
        if child.unwrap().kind()=="var_label" {
            let bad_fwd_ref = child.unwrap().child_count()==0 && !symbols.var_defined(&txt);
            register(&txt,ctx.line(),loc,in_macro,&node,&mut symbols.vars,None,bad_fwd_ref);
        }
        return Ok(Navigation::GotoSibling);
    } else if (node.kind() == "psop_put" || node.kind() == "psop_use") && (src.typ==SourceType::Master || src.typ==SourceType::Module) {
        return Ok(Navigation::Descend);
    } else if node.kind()=="psop_xc" {
        // this only needs to update, XC diagnostics are in another pass
        let mut xc = ctx.get_xc();
        let next = node.next_named_sibling();
        if next.is_some() && ctx.node_match(&next.unwrap(), "OFF") {
            xc = 0;
        }
        else {
            xc += 1;
        }
        if xc>2 {
            xc = 2;
        }
        ctx.set_xc(xc);
        symbols.processor = ctx.curr_proc();
        return Ok(Navigation::Exit);
    }

    return Ok(Navigation::GotoChild);
}

/// Add diagnostics for the path appearing in PUT or USE
fn verify_include_path(curs: &TreeCursor, ctx: &mut Context, ws: &Workspace) -> Vec<lsp::Diagnostic>
{
    let mut ans = Vec::new();
    if let Some(path_node) = curs.node().next_named_sibling() {
        if let Some(src) = ctx.curr_source() {
            let rng = lsp_range(path_node.range(), src.row, src.col);
            let num = ws.include_candidates(curs, ctx.line());
            if num == 0 {
                ans.push(basic_diag(rng, "file not found in workspace", lsp::DiagnosticSeverity::ERROR));
            }
            if num > 1 {
                ans.push(basic_diag(rng, &format!("multiple matches ({}) exist in the workspace",num), lsp::DiagnosticSeverity::ERROR));
            }
        }
    }
    ans
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
    let scope = ctx.curr_scope();
    let (in_macro,in_global) = match &scope {
        Some(s) => (s.flags & flg::MAC > 0, s.flags & flg::MAC == 0),
        None => (false,false)
    };
    let macro_averse = ["psop_ent","psop_ext","psop_exd","psop_put","psop_use","psop_sav"];
    if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_defined(&txt);
            let is_mac_loc = in_macro && symbols.child_defined(&txt, scope.as_ref().unwrap());
            if in_macro && !is_glob && !is_mac_loc {
                push(rng,"label is undefined",lsp::DiagnosticSeverity::ERROR);
            } else if !in_macro && !is_glob {
                push(rng, "global label is undefined", lsp::DiagnosticSeverity::ERROR);
            } else if is_glob {
                register(&txt,ctx.line(),loc,in_macro,&node,&mut symbols.globals,None,false);
            } else if is_mac_loc {
                register_child(&txt,loc,in_macro,&node,ctx);
            }
        } else if ck=="local_label" && in_global && !symbols.child_defined(&txt, scope.as_ref().unwrap()) {
            push(rng,"local label is not defined in this scope",lsp::DiagnosticSeverity::ERROR);
        } else if ck=="local_label" && in_macro {
            push(rng, "cannot use local labels in a macro", lsp::DiagnosticSeverity::ERROR);
        } else if ck=="local_label" && scope.is_some() {
            register_child(&txt,loc,in_macro,&node,ctx);
        } else if ck == "var_label" {
            if let Some(grandchild) = child.unwrap().named_child(0) {
                if in_macro && grandchild.kind() == "var_mac" {
                    return Ok(Navigation::GotoSibling);
                } else if in_macro && ctx.merlin_version()!=MerlinVersion::Merlin8 && grandchild.kind() == "var_cnt" {
                    return Ok(Navigation::GotoSibling);
                } else if !in_macro && grandchild.kind()=="var_mac" {
                    push(rng, "macro substitution variable referenced outside macro", lsp::DiagnosticSeverity::ERROR);
                } else if !in_macro && ctx.merlin_version()!=MerlinVersion::Merlin8 && grandchild.kind()=="var_cnt" {
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
    else if node.kind() == "var_mac" && !in_macro {
        push(rng, "macro substitution variable referenced outside macro", lsp::DiagnosticSeverity::ERROR);
    } else if child.is_some() && node.kind()=="label_def" {
        let ck = child.unwrap().kind();
        if ck == "global_label" && !in_macro {
            ctx.enter_scope(&txt,symbols);
        } else if ck=="local_label" {
            if let Some(next) = node.next_named_sibling() {
                if next.kind()=="psop_mac" || next.kind()=="psop_ent" || next.kind()=="psop_ext" || next.kind()=="psop_equ" {
                    push(rng,&format!("cannot use local label for {}",node_text(&next, ctx.line())),lsp::DiagnosticSeverity::ERROR);
                }
            }
            if in_macro {
                push(rng,"cannot use local labels in a macro",lsp::DiagnosticSeverity::ERROR);
            }
        }
        return Ok(Navigation::GotoSibling)
    } else if node.kind()=="macro_def" {
        ctx.enter_scope(&txt,symbols);
    } else if node.kind()=="macro_ref" {
        if !symbols.mac_defined(&txt) && symbols.global_defined(&txt) {
            push(rng,"expected macro, this is a label",lsp::DiagnosticSeverity::ERROR);
        } else if !symbols.mac_defined(&txt) {
            push(rng,"macro is undefined",lsp::DiagnosticSeverity::ERROR);
        } else if symbols.mac_forward(&txt,&loc) {
            push(rng,"macro is forward referenced",lsp::DiagnosticSeverity::ERROR);
        } else {
            super::macros::check_macro_args(&node, symbols, ctx, diagnostics);
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
    } else if in_macro && macro_averse.contains(&node.kind()) {
        push(rng,"pseudo operation cannot be used in a macro",lsp::DiagnosticSeverity::ERROR);
    } else if node.kind()=="psop_eom" {
        if in_macro {
            ctx.exit_scope(symbols);
        }
        // unmatched case already handled in gather sweep
    }
    return Ok(Navigation::GotoChild);
}    
