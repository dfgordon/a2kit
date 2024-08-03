use std::collections::HashMap;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use super::context::Context;
use super::super::{Symbol,Symbols,Workspace,SourceType,LabelType};
use super::super::symbol_flags as flg;
use crate::lang::merlin::{self, MerlinVersion};
use crate::lang::server::{path_in_workspace,basic_diag};
use crate::lang::{Navigation,node_text,lsp_range};
use crate::DYNERR;

const FWD_REF_AVERSE: [&str;5] = ["arg_equ","arg_if","arg_do","arg_lup","arg_var"];
const MACRO_AVERSE: [&str;6] = ["psop_ent","psop_ext","psop_exd","psop_put","psop_use","psop_sav"];

/// Register occurence of any top level symbol.
/// Looks around the node to create the symbol.  Returns the calculated symbol flags.
fn register(txt: &str, source: &str, loc: lsp::Location, node: &tree_sitter::Node, map: &mut HashMap<String,Symbol>, docstring: Option<String>, fwd: Vec<LabelType>) -> u64 {
    match map.get_mut(txt) {
        Some(sym) => {
            if node.kind() == "label_def" || node.kind() == "macro_def" {
                sym.defining_code = Some(source.to_string());
                if let Some(ds) = docstring {
                    sym.docstring = ds;
                }
            }
            log::trace!("add node {}",txt);
            sym.fwd_refs.insert(loc.clone(),fwd);
            sym.add_node(loc, node, source);
            sym.flags
        },
        None => {
            let mut sym = Symbol::create(loc.clone(),node,source);
            if node.kind() == "label_def" || node.kind() == "macro_def" {
                sym.defining_code = Some(source.to_string());
                if let Some(ds) = docstring {
                    sym.docstring = ds;
                }
            }
            log::trace!("new node {}",txt);
            let returned_flags = sym.flags;
            sym.fwd_refs.insert(loc.clone(),fwd);
            map.insert(txt.to_string(),sym);
            returned_flags
        }
    }
}

/// Register occurence of any child symbol.
/// This information is put on the scope stack, not the main symbol store.
/// When exiting the scope the child information is saved to the main store.
fn register_child(txt: &str, loc: lsp::Location, node: &tree_sitter::Node, ctx: &mut Context, fwd: Vec<LabelType>) {
    let source = ctx.line().to_owned();
    if let Some(scope) = ctx.curr_scope() {
        match scope.children.get_mut(txt) {
            Some(sym) => {
                log::trace!("add child node {}",txt);
                sym.fwd_refs.insert(loc.clone(),fwd);
                sym.add_node(loc,node,&source);
            },
            None => {
                let mut sym = Symbol::create(loc.clone(),node,&source);
                log::trace!("new child node {}",txt);
                sym.fwd_refs.insert(loc.clone(),fwd);
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
        } else {
            push(rng,"macro label needs to be global",lsp::DiagnosticSeverity::ERROR);
        }
        register(&txt, ctx.line(), loc, &node, &mut symbols.macros, Some(ctx.running_docstring.clone()),vec![]);
        ctx.running_docstring = String::new();
        ctx.enter_scope(&txt,symbols);
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
            if symbols.global_defined(&txt) {
                push(rng, "redefinition of a global label", lsp::DiagnosticSeverity::ERROR);
            }
            if symbols.mac_defined(&txt) {
                push(rng, "label name is used previously as a macro", lsp::DiagnosticSeverity::ERROR);
            }
            let f = register(&txt, ctx.line(), loc, &node, &mut symbols.globals, Some(ctx.running_docstring.clone()),vec![]);
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
        } else if ck == "global_label" && in_macro {
            if symbols.global_defined(&txt) {
                push(rng, "macro local shadows global", lsp::DiagnosticSeverity::WARNING);
            }
            if symbols.child_defined(&txt,&ctx.curr_scope().unwrap()) {
                push(rng, "redefinition of a macro scoped label", lsp::DiagnosticSeverity::ERROR);
            } else {
                register_child(&txt,loc,&node,ctx,vec![]);
            }
        } else if ck == "local_label" && !in_macro {
            if let Some(scope) = ctx.curr_scope() {
                if symbols.child_defined(&txt, &scope) {
                    push(rng, "redefinition of a local label", lsp::DiagnosticSeverity::ERROR);
                } else {
                    register_child(&txt,loc,&node,ctx,vec![]);
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
                register(&txt,ctx.line(),loc,&node,&mut symbols.vars,None,vec![]);
            }
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "macro_ref" {
        if child.unwrap().kind()=="global_label" {
            let fwd = match symbols.mac_defined(&txt) {
                true => Vec::new(),
                false => vec![LabelType::Macro]
            };
            register(&txt,ctx.line(),loc,&node,&mut symbols.macros,None,fwd);
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "label_ref" {
        let mut fwd = Vec::new();
        let no_fwd = match super::find_arg_node(&node) {
            Some(psop) => FWD_REF_AVERSE.contains(&psop.as_str()),
            None => false
        };
        if child.unwrap().kind()=="var_label" {
            if !symbols.var_defined(&txt) {
                fwd.push(LabelType::Variable);
            };
            register(&txt,ctx.line(),loc,&node,&mut symbols.vars,None,fwd);
        } else if child.unwrap().kind()=="global_label" && !in_macro {
            if no_fwd && !symbols.global_defined(&txt) {
                fwd.push(LabelType::Global);
            }
            register(&txt,ctx.line(),loc,&node,&mut symbols.globals,None,fwd);
        } else if child.unwrap().kind()=="global_label" && in_macro {
            // Tentatively register as a child (macro local), will be resolved on second pass
            if no_fwd && !symbols.global_defined(&txt) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::Global);
            }
            if no_fwd && !symbols.child_defined(&txt,ctx.curr_scope().unwrap()) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::MacroLocal);
            }
            register_child(&txt,loc,&node,ctx,fwd);
        }  else if child.unwrap().kind()=="local_label" && !in_macro {
            if no_fwd && !symbols.child_defined(&txt,ctx.curr_scope().unwrap()) {
                fwd.push(LabelType::Local);
            }
            register_child(&txt,loc,&node,ctx,fwd);
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
    let (in_macro,in_global) = match &ctx.curr_scope() {
        Some(s) => (s.flags & flg::MAC > 0, s.flags & flg::MAC == 0),
        None => (false,false)
    };
    if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_defined(&txt);
            let is_mac_loc = in_macro && symbols.child_defined(&txt, ctx.curr_scope().as_ref().unwrap());
            if is_glob && is_mac_loc {
                push(rng,"macro local shadows global",lsp::DiagnosticSeverity::WARNING);
            }
            // This fixes a wrongly identified macro local originating in the first pass.
            // We have to catch the first one and switch all occurrences right then.
            if in_macro && is_glob && !is_mac_loc {
                let scope = ctx.curr_scope().unwrap();
                if let Some(child) = scope.children.get(&txt) {
                    if let Some(glob) = symbols.globals.get_mut(&txt) {
                        for (l,v) in &child.fwd_refs {
                            glob.fwd_refs.insert(l.clone(),v.clone());
                        }
                        for l in &child.refs {
                            glob.refs.push(l.clone());
                        }
                    }
                    if let Some(main) = symbols.macros.get_mut(&scope.name) {
                        main.children.remove(&txt); // clean the main store
                    }
                    scope.children.remove(&txt); // clean the scope stack
                }
            }
            if in_macro && !is_glob && !is_mac_loc {
                push(rng,"label is undefined",lsp::DiagnosticSeverity::ERROR);
            } else if !in_macro && !is_glob {
                push(rng, "global label is undefined", lsp::DiagnosticSeverity::ERROR);
            } else if is_glob && symbols.global_forward(&txt,&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            } else if is_mac_loc && symbols.child_forward(&txt,ctx.curr_scope().as_ref().unwrap(),&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            }
        } else if ck=="local_label" {
            if in_macro {
                push(rng, "cannot use local labels in a macro", lsp::DiagnosticSeverity::ERROR);
            } else if in_global && !symbols.child_defined(&txt, ctx.curr_scope().as_ref().unwrap()) {
                push(rng,"local label is not defined in this scope",lsp::DiagnosticSeverity::ERROR);
            } else if in_global && symbols.child_forward(&txt, ctx.curr_scope().as_ref().unwrap(),&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            }
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
    } else if in_macro && MACRO_AVERSE.contains(&node.kind()) {
        push(rng,"pseudo operation cannot be used in a macro",lsp::DiagnosticSeverity::ERROR);
    } else if node.kind()=="psop_eom" {
        if in_macro {
            ctx.exit_scope(symbols);
        }
        // unmatched case already handled in gather sweep
    }
    return Ok(Navigation::GotoChild);
}    
