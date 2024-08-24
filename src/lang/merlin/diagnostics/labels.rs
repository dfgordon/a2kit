use lsp_types as lsp;
use tree_sitter::TreeCursor;
use crate::lang::merlin::context::Context;
use super::super::{Symbol,Symbols,Workspace,SourceType,LabelType};
use super::super::symbol_flags as flg;
use crate::lang::merlin::{self, MerlinVersion, assembly};
use crate::lang::server::{path_in_workspace,basic_diag};
use crate::lang::{Navigation,node_text,lsp_range};
use crate::DYNERR;

const FWD_REF_AVERSE: [&str;5] = ["arg_equ","arg_if","arg_do","arg_lup","arg_var"];
const MACRO_AVERSE: [&str;6] = ["psop_ent","psop_ext","psop_exd","psop_put","psop_use","psop_sav"];
const FOLDS: [&str;11] = ["psop_do","psop_if","psop_else","psop_fin","psop_lup","psop_end_lup","psop_mac","psop_eom","psop_dum","psop_dend","psop_end"];

/// Register occurence of any top level symbol.
/// Looks around the node to create the symbol and assign a value.  Returns the calculated symbol flags.
/// This function can panic if the starting node is not "label_def", "label_ref", "macro_def", or "macro_ref".
fn register(txt: &str, loc: lsp::Location, node: &tree_sitter::Node, symbols: &mut Symbols, docstring: Option<String>, ctx: &Context, fwd: Vec<LabelType>) -> u64 {
    let mut new_val: Option<i64> = None;
    // TODO: assign a value based on PC here
    if let Some(c2) = node.next_named_sibling() {
        if c2.kind()=="psop_equ" {
            if let Some(c3) = c2.next_named_sibling() {
                new_val = match assembly::eval_expr(&c3, ctx.line(), None, symbols, ctx.curr_scope()) {
                    Ok(v) => Some(v),
                    Err(_) => None 
                };
            }
        }
    }
    let map = match node.kind() {
        "macro_def" | "macro_ref" => &mut symbols.macros,
        _ => match node.named_child(0).unwrap().kind() {
            "global_label" => &mut symbols.globals,
            "var_label" => &mut symbols.vars,
            _ => panic!("register symbol hit an unexpected node")
        }
    };
    match map.get_mut(txt) {
        Some(sym) => {
            if node.kind() == "label_def" {
                sym.defining_code = Some(ctx.line().to_string());
                if let Some(ds) = docstring {
                    sym.docstring = ds;
                }
                // new assignment takes precedence esp. for var
                if new_val.is_some() {
                    sym.value = new_val;
                }
            }
            log::trace!("add node {}",txt);
            sym.fwd_refs.insert(loc.clone(),fwd);
            sym.add_node(loc, node, ctx.line());
            sym.flags
        },
        None => {
            let mut sym = Symbol::create(loc.clone(),node,ctx.line());
            if node.kind() == "label_def" {
                sym.defining_code = Some(ctx.line().to_string());
                if let Some(ds) = docstring {
                    sym.docstring = ds;
                }
                // new assignment takes precedence esp. for var
                if new_val.is_some() {
                    sym.value = new_val;
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
    // TODO: values could be assigned to locals here.  For macro-locals we would only assign during expansion.
    let source = ctx.line().to_owned();
    if let Some(scope) = ctx.curr_scope_mut() {
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

fn eval_fold_expr(node: &tree_sitter::Node,pc: Option<usize>,symbols: &Symbols,ctx: &Context,in_macro_def: bool,diagnostics: Option<&mut Vec<lsp::Diagnostic>>) -> i64 {
    let range = lsp_range(node.range(),ctx.row(),ctx.col());
    let cannot_eval_mess = match in_macro_def {
        true => basic_diag(range,"evaluation was deferred",lsp::DiagnosticSeverity::HINT),
        false => basic_diag(range,"extension cannot evaluate, assuming true",lsp::DiagnosticSeverity::WARNING)
    };
    let (arg,diag) = match node.next_named_sibling() {
        Some(arg_node) => match node.kind() {
            "psop_if" => match assembly::eval_if(&arg_node, ctx.line()) {
                Ok(val) => (val,None),
                Err(_) => (1,Some(cannot_eval_mess))
            },
            "psop_do" => match assembly::eval_expr(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => (val,None),
                Err(_) => (1,Some(cannot_eval_mess))
            },
            _ =>  match assembly::eval_expr(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => (val,None),
                Err(_) => (1,None)
            }
        },
        None => (1,None)
    };
    if let Some(diag) = diag {
        if let Some(diagnostics) = diagnostics {
            diagnostics.push(diag);
        }
    }
    arg
}

fn visit_gather_macro_def(node: &tree_sitter::Node, loc: lsp::Location, ctx: &mut Context, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
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
            if symbols.global_defined(&txt) {
                push(rng,"macro name is used previously as a label",lsp::DiagnosticSeverity::ERROR);
            }
        } else {
            push(rng,"macro label needs to be global",lsp::DiagnosticSeverity::ERROR);
        }
        register(&txt, loc, &node, symbols, Some(ctx.running_docstring.clone()), ctx, vec![]);
        ctx.running_docstring = String::new();
        let needs_first_line = match ctx.curr_scope() {
            Some(scope) => scope.flags & flg::MAC == 0,
            None=> true
        };
        ctx.enter_scope(&txt,symbols);
        if needs_first_line {
            ctx.push_macro_line();
        }
        return Ok(Navigation::GotoSibling);
    } else if node.kind()=="psop_eom" { // also processed by folds
        ctx.exit_scope(symbols);
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind()=="label_def" {
        let ck = child.unwrap().kind();
        if ck == "global_label" {
            if symbols.global_defined(&txt) {
                push(rng, "macro local shadows global", lsp::DiagnosticSeverity::WARNING);
            }
            if symbols.child_defined(&txt,&ctx.curr_scope().unwrap()) {
                push(rng, "redefinition of a macro scoped label", lsp::DiagnosticSeverity::ERROR);
            } else {
                register_child(&txt,loc,&node,ctx,vec![]);
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
            if no_fwd && !symbols.global_defined(&txt) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::Global);
            }
            if no_fwd && !symbols.child_defined(&txt,ctx.curr_scope().unwrap()) {
                log::trace!("{}: push `{}` onto fwd list",ctx.row(),&txt);
                fwd.push(LabelType::MacroLocal);
            }
            register_child(&txt,loc,&node,ctx,fwd);
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
        let arg = eval_fold_expr(&node, None, symbols, ctx, in_macro_def, Some(diagnostics));
        folding.append(&mut ctx.folding_range(&node.kind(), rng, loc.clone(), arg, Some(diagnostics)));
    }

    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };

    let (asm,gen,is_end) = ctx.cond_asm();
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
    } else if curs.depth()>1 && node.kind() != "label_def" {
        ctx.running_docstring = String::new();
    }
    if child.is_some() && node.kind()=="label_def" && gen {
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
        } else if ck == "local_label" {
            if let Some(scope) = ctx.curr_scope() {
                if symbols.child_defined(&txt, &scope) {
                    push(rng, "redefinition of a local label", lsp::DiagnosticSeverity::ERROR);
                } else {
                    register_child(&txt,loc,&node,ctx,vec![]);
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
        }
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind() == "label_ref" && asm {
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
            register_child(&txt,loc,&node,ctx,fwd);
        }
        return Ok(Navigation::GotoSibling);
    } else if gen && (node.kind() == "psop_put" || node.kind() == "psop_use") && (src.typ==SourceType::Master || src.typ==SourceType::Module) {
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

fn visit_verify_macro_def(node: &tree_sitter::Node, loc: lsp::Location, ctx: &mut Context, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
-> Result<Navigation,DYNERR> {
    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(basic_diag(rng,mess,severity));
    };
    let child = node.named_child(0);
    let (rng,txt) = ctx.node_spec(&node);
    if node.kind()=="macro_def" {
        ctx.enter_scope(&txt,symbols);
    } else if node.kind()=="psop_eom" { // also processed by folds
        ctx.exit_scope(symbols);
        return Ok(Navigation::GotoSibling);
    } else if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_defined(&txt);
            let is_mac_loc = symbols.child_defined(&txt, ctx.curr_scope().as_ref().unwrap());
            if is_glob && is_mac_loc {
                push(rng,"macro local shadows global",lsp::DiagnosticSeverity::WARNING);
            }
            // This fixes a wrongly identified macro local originating in the first pass.
            // We have to catch the first one and switch all occurrences right then.
            if is_glob && !is_mac_loc {
                let scope = ctx.curr_scope_mut().unwrap();
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
            if is_mac_loc && symbols.child_forward(&txt,ctx.curr_scope().as_ref().unwrap(),&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            } else if is_glob && symbols.global_forward(&txt,&loc) {
                push(rng,"illegal forward reference",lsp::DiagnosticSeverity::ERROR);
            } else if !is_glob && !is_mac_loc {
                push(rng,"label is undefined",lsp::DiagnosticSeverity::ERROR);
            }
        } else if ck=="local_label" {
            push(rng, "cannot use local labels in a macro", lsp::DiagnosticSeverity::ERROR);
        } else if ck == "var_label" {
            if !symbols.var_defined(&txt) && child.unwrap().named_child(0).is_none() {
                push(rng,"variable is undefined",lsp::DiagnosticSeverity::ERROR);
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
        let arg = eval_fold_expr(&node, None, symbols, ctx, in_macro_def, None);
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
    if child.is_some() && node.kind()=="label_ref" {
        let ck = child.unwrap().kind();
        if ck=="global_label" && symbols.mac_defined(&txt) {
            push(rng,"macro cannot be used here",lsp::DiagnosticSeverity::ERROR);
        } else if ck == "global_label" {
            let is_glob = symbols.global_defined(&txt);
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
        let ck = child.unwrap().kind();
        if ck == "global_label" {
            ctx.enter_scope(&txt,symbols);
        } else if ck=="local_label" {
            if let Some(next) = node.next_named_sibling() {
                if next.kind()=="psop_mac" || next.kind()=="psop_ent" || next.kind()=="psop_ext" || next.kind()=="psop_equ" {
                    push(rng,&format!("cannot use local label for {}",node_text(&next, ctx.line())),lsp::DiagnosticSeverity::ERROR);
                }
            }
        }
        return Ok(Navigation::GotoSibling)
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
    }
    return Ok(Navigation::GotoChild);
}    
