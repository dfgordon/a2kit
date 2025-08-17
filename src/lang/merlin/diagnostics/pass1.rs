use lsp_types as lsp;
use tree_sitter::{TreeCursor,Node};
use crate::lang::merlin::context::Context;
use super::get_value;
use super::super::{Symbol,Symbols,Workspace,SourceType,LabelType};
use super::super::symbol_flags as flg;
use crate::lang::merlin::{self, MerlinVersion};
use crate::lang::server::basic_diag;
use crate::lang::{Navigation,node_text};
use crate::DYNERR;

const FWD_REF_AVERSE: [&str;5] = ["arg_equ","arg_if","arg_do","arg_lup","arg_var"];
const FOLDS: [&str;11] = ["psop_do","psop_if","psop_else","psop_fin","psop_lup","psop_end_lup","psop_mac","psop_eom","psop_dum","psop_dend","psop_end"];

/// Register occurence of any top level symbol. Also handles MX.  Should only be called during gather step.
/// Looks around the node to create or update the symbol-.  Returns the calculated symbol flags.
/// This function can panic if the starting node is not "arg_mx", "label_def", "label_ref", "macro_def", or "macro_ref".
fn register(txt: &str, loc: lsp::Location, node: &Node, symbols: &mut Symbols, docstring: Option<String>, ctx: &Context, fwd: Vec<LabelType>) -> u64 {
    let new_val = get_value(node, symbols, ctx.line(), ctx.curr_scope());
    if node.kind() == "arg_mx" {
        symbols.mx.value = new_val;
        symbols.mx.checkpoints.push((loc.clone(),new_val));
        return symbols.mx.flags;
    }
    let map = match node.kind() {
        "macro_def" | "macro_ref" => &mut symbols.macros,
        _ => match node.named_child(0).unwrap().kind() {
            "global_label" => &mut symbols.globals,
            "var_label" => &mut symbols.vars,
            _ => panic!("register symbol hit an unexpected node")
        }
    };
    if !map.contains_key(txt) {
        log::trace!("new symbol {}",txt);
        map.insert(txt.to_string(),Symbol::new(txt));
    }
    log::trace!("add node {}",txt);
    let sym = map.get_mut(txt).unwrap();
    if node.kind() == "label_def" {
        sym.defining_code = Some(ctx.line().to_string());
        if let Some(ds) = docstring {
            sym.docstring = ds;
        }
        // new assignment takes precedence esp. for var
        if new_val.is_some() {
            sym.value = new_val;
            sym.checkpoints.push((loc.clone(),new_val));
        }
    } else if node.kind() == "macro_def" {
        if let Some(ds) = docstring {
            sym.docstring = ds;
        }
    }
    sym.fwd_refs.insert(loc.clone(),fwd);
    sym.add_node(loc, node, ctx.line());
    sym.flags
}

/// Register occurence of any child symbol.
/// This information is put on the scope stack, not the main symbol store.
/// When exiting the scope the child information is saved to the main store.
fn register_child(txt: &str, loc: lsp::Location, node: &Node, symbols: &Symbols, ctx: &mut Context, fwd: Vec<LabelType>) {
    let new_val = get_value(node, symbols, ctx.line(), ctx.curr_scope());
    let source = ctx.line().to_owned();
    let defining_code = ctx.line().to_string();
    let vers = ctx.merlin_version();
    if let Some(scope) = ctx.curr_scope_mut() {
        if !scope.children.contains_key(txt) {
            log::trace!("new symbol {}",txt);
            scope.children.insert(txt.to_string(),Symbol::new(txt));
        }
        log::trace!("add child node {}",txt);
        let sym = scope.children.get_mut(txt).unwrap();
        if node.kind() == "label_def" {
            sym.defining_code = Some(defining_code);
            // The following handles duplicate child labels.
            // We flag these as errors, but Merlin accepts them in macros.
            // Merlin 32 takes the first one, all others take the last one.
            if vers == MerlinVersion::Merlin32 {
                if sym.value.is_none() && new_val.is_some() {
                    sym.value = new_val;
                    sym.checkpoints.push((loc.clone(),new_val));
                }
            } else {
                if new_val.is_some() {
                    sym.value = new_val;
                    sym.checkpoints.push((loc.clone(),new_val));
                }
            }
        }
        sym.fwd_refs.insert(loc.clone(),fwd);
        sym.add_node(loc,node,&source);
    }
}

/// Called by the main gather function if we are inside a macro definition, in which case the process is different.
fn visit_gather_macro_def(node: &Node, loc: lsp::Location, ctx: &mut Context, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
-> Result<Navigation,DYNERR> {
    let diag_count = diagnostics.len();
    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };
    let child = node.named_child(0);
    let (rng,txt) = ctx.node_spec(&node);
    if node.kind() == "source_file" {
        ctx.push_macro_line();
    }
    if node.kind() == "macro_def" && child.is_some() {
        // it seems a macro is unconditionally processed
        let curr_proc = ctx.curr_proc();
        let vers = ctx.merlin_version();
        let src = match ctx.curr_source() {
            Some(s) => s,
            None => return Ok(Navigation::Exit)
        };
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
            if symbols.global_declared_or_defined(&txt) {
                push(rng,"macro name is used previously as a label",lsp::DiagnosticSeverity::ERROR);
            }
        } else {
            push(rng,"macro label needs to be global",lsp::DiagnosticSeverity::ERROR);
        }
        register(&txt, loc, &node, symbols, Some(ctx.running_docstring.clone()), ctx, vec![]);
        ctx.running_docstring = String::new();
        let is_new_mac_scope = match ctx.curr_scope() {
            Some(scope) => scope.flags & flg::MAC == 0,
            None=> true
        };
        if is_new_mac_scope {
            ctx.enter_scope(&txt,symbols);
            ctx.push_macro_line();
            ctx.trigs.push_vars = true;
            ctx.trigs.unset_vars = true;
        } else {
            ctx.push_dependency(&txt);
            ctx.enter_scope(&txt,symbols);
        }
        return Ok(Navigation::GotoSibling); // need to visit psop_mac in the fold handler
    } else if node.kind()=="macro_ref" {
        ctx.push_dependency(&txt);
        ctx.trigs.unset_vars = true;
        ctx.trigs.unset_children = true;
    } else if node.kind()=="psop_eom" { // also processed by folds
        if let Some(duplicate_mess) = ctx.exit_scope(symbols) {
            if let Some(severity) = ctx.dup_mac_locs() {
                push(rng,&duplicate_mess,severity);
            }
        }
        ctx.trigs.pop_vars = true;
        ctx.trigs.checkpoint_vars = true;
        return Ok(Navigation::GotoSibling);
    } else if node.kind()=="arg_mx" {
        register("MX",loc,&node,symbols,None,ctx,vec![]);
    } else if child.is_some() && node.kind()=="label_def" {
        let ck = child.unwrap().kind();
        if ck == "global_label" {
            if symbols.global_declared_or_defined(&txt) {
                push(rng, "macro local shadows global", lsp::DiagnosticSeverity::WARNING);
            }
            register_child(&txt,loc,&node,symbols,ctx,vec![]);
        } else if ck == "var_label" {
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
                register(&txt,loc,&node,symbols,None,ctx,vec![]);
            }
        }
    } else if child.is_some() && node.kind()=="label_ref" {
        let mut fwd = Vec::new();
        let no_fwd = match super::find_arg_node(&node) {
            Some(psop) => FWD_REF_AVERSE.contains(&psop.as_str()),
            None => false
        };
        if child.unwrap().kind()=="var_label" {
            if !symbols.var_defined(&txt) {
                fwd.push(LabelType::Variable);
            };
            register(&txt,loc,&node,symbols,None,ctx,fwd);
        } else if child.unwrap().kind()=="global_label" {
            // Tentatively register as a child (macro local), will be resolved on second pass
            if no_fwd && !symbols.global_declared_or_defined(&txt) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::Global);
            }
            if no_fwd && !symbols.child_defined(&txt,ctx.curr_scope().unwrap()) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::MacroLocal);
            }
            register_child(&txt,loc,&node,symbols,ctx,fwd);
        }
    }
    return Ok(Navigation::GotoChild);
}

/// Gather symbols and check for forward references
pub fn visit_gather(curs: &TreeCursor, ctx: &mut Context, ws: &Workspace, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>, folding: &mut Vec<lsp::FoldingRange>)
    -> Result<Navigation,DYNERR> {
    let diag_count = diagnostics.len();
    let src = match ctx.curr_source() {
        Some(s) => s,
        None => return Ok(Navigation::Exit)
    };
    let node = curs.node();
    let (rng,txt) = ctx.node_spec(&node);
    let loc = lsp::Location::new(src.doc.uri.clone(),rng);
    let child = node.named_child(0);
    let in_macro_def = match ctx.curr_scope() {
        Some(scope) => scope.flags & flg::MAC > 0,
        None => false
    };

    if FOLDS.contains(&node.kind()) {
        let arg = super::eval_fold_expr(&node, None, symbols, ctx, in_macro_def, Some(diagnostics));
        folding.append(&mut ctx.folding_range(&node.kind(), rng, loc.clone(), arg, Some(diagnostics)));
    }

    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };

    let (asm,r#gen,is_end) = ctx.cond_asm();
    if is_end {
        return Ok(Navigation::Exit);
    }
    if in_macro_def || node.kind()=="macro_def" {
        return visit_gather_macro_def(&node, loc, ctx, symbols, diagnostics);
    }

    if node.kind() == "heading" {
        let temp = match child {
            Some(n) => node_text(&n, ctx.line()),
            None => String::new()
        };
        ctx.running_docstring += &temp;
        ctx.running_docstring += "\n";
        return Ok(Navigation::Exit);
    } else if curs.depth()>1 && node.kind() != "label_def" && node.is_named() {
        ctx.running_docstring = String::new();
    }
    if r#gen && child.is_some() && node.kind()=="label_def" {
        let ck = child.unwrap().kind();
        if ck == "global_label" {
            if symbols.global_defined(&txt) {
                push(rng, "redefinition of a global label", lsp::DiagnosticSeverity::ERROR);
            }
            if symbols.mac_defined(&txt) {
                push(rng, "label name is used previously as a macro", lsp::DiagnosticSeverity::ERROR);
            }
            let f = register(&txt, loc, &node, symbols, Some(ctx.running_docstring.clone()),ctx, vec![]);
            if f & merlin::symbol_flags::EXT > 0 {
                match ws.entries.get(&txt) {
                    Some(ent) => {
                        if let Some(ext) = symbols.globals.get_mut(&txt) {
                            if ent.defs.len() > 0 {
                                ext.defs = ent.defs.clone();
                                ext.defining_code = ent.defining_code.clone();
                                ext.docstring = ent.docstring.clone();
                            } else if ent.decs.len() > 0 {
                                ext.defining_code = None;
                                ext.docstring = format!("entry exists in operand form");
                            } else {
                                ext.defining_code = None;
                                ext.docstring = format!("entry exists but there was an internal error");
                            }
                        }
                    },
                    None => {
                        if let Some(severity) = ctx.missing_entries() {
                            push(rng,"entry was not found in workspace",severity)
                        }
                    }
                };
            }
            if f & merlin::symbol_flags::EXT > 0 || f & merlin::symbol_flags::ENT > 0 {
                if let Some(declarable) = symbols.globals.get(&txt) {
                    if declarable.decs.len() > 1 {
                        push(rng,"redeclaration of a workspace symbol",lsp::DiagnosticSeverity::ERROR);
                    }
                }
            }
            if f & merlin::symbol_flags::EXT > 0 && f & merlin::symbol_flags::ENT > 0 {
                push(rng,"label is both EXT and ENT in the same module",lsp::DiagnosticSeverity::ERROR);
            }
            ctx.running_docstring = String::new();
            ctx.enter_scope(&txt,symbols);
        } else if ck == "local_label" {
            if let Some(scope) = ctx.curr_scope() {
                if symbols.child_defined(&txt, &scope) {
                    push(rng, "redefinition of a local label", lsp::DiagnosticSeverity::ERROR);
                } else {
                    register_child(&txt,loc,&node,symbols,ctx,vec![]);
                }
            } else {
                push(rng,"no global scope is defined yet",lsp::DiagnosticSeverity::ERROR);
            }
        } else if ck == "var_label" {
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
                register(&txt,loc,&node,symbols,None,ctx,vec![]);
            }
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "macro_ref" && asm {
        if child.unwrap().kind()=="global_label" {
            let fwd = match symbols.mac_defined(&txt) {
                true => Vec::new(),
                false => vec![LabelType::Macro]
            };
            register(&txt,loc,&node,symbols,None,ctx,fwd);
            ctx.trigs.unset_vars = true;
        }
        return Ok(Navigation::GotoSibling);
    } else if r#gen && node.kind() == "label_ref" && node.parent().unwrap().kind() == "arg_ent" {
        let f = register(&txt,loc,&node,symbols,None,ctx,vec![]);
        if f & merlin::symbol_flags::EXT > 0 && f & merlin::symbol_flags::ENT > 0 {
            push(rng,"label is both EXT and ENT in the same module",lsp::DiagnosticSeverity::ERROR);
        }
    } else if asm && child.is_some() && node.kind() == "label_ref" {
        let mut fwd = Vec::new();
        let no_fwd = match super::find_arg_node(&node) {
            Some(psop) => FWD_REF_AVERSE.contains(&psop.as_str()),
            None => false
        };
        if child.unwrap().kind()=="var_label" {
            if !symbols.var_defined(&txt) {
                fwd.push(LabelType::Variable);
            };
            register(&txt,loc,&node,symbols,None,ctx,fwd);
        } else if child.unwrap().kind()=="global_label" {
            if no_fwd && !symbols.global_defined(&txt) {
                fwd.push(LabelType::Global);
            }
            register(&txt,loc,&node,symbols,None,ctx,fwd);
        }  else if child.unwrap().kind()=="local_label" {
            if no_fwd && !symbols.child_defined(&txt,ctx.curr_scope().unwrap()) {
                fwd.push(LabelType::Local);
            }
            register_child(&txt,loc,&node,symbols,ctx,fwd);
        }
        return Ok(Navigation::GotoSibling);
    } else if r#gen && (node.kind() == "psop_put" || node.kind() == "psop_use") && (src.typ==SourceType::Master || src.typ==SourceType::Module) {
        return Ok(Navigation::Descend);
    } else if asm && node.kind()=="psop_xc" {
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
    } else if asm && node.kind()=="arg_mx" {
        register("MX",loc,&node,symbols,None,ctx,vec![]);
    } else if asm && node.kind()=="psop_end_lup" {
        ctx.trigs.unset_vars = true;
    }

    return Ok(Navigation::GotoChild);
}
