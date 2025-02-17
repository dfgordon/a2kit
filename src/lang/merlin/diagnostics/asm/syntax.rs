use lsp_types as lsp;
use tree_sitter::TreeCursor;
use lsp::{DiagnosticSeverity,Diagnostic};
use crate::lang::merlin::MerlinVersion;
use crate::lang::merlin::context::Context;
use crate::lang::{node_text,lsp_range,extended_range};

pub fn visit(curs: &TreeCursor, ctx: &Context, diagnostics: &mut Vec<Diagnostic>) {
    let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
        diagnostics.push(crate::lang::server::basic_diag(rng,mess,severity));
    };
    let (rng,txt) = ctx.node_spec(&curs.node());
    let version = ctx.merlin_version();
    let max_lab_len = match version {
        MerlinVersion::Merlin8 => 13,
        _ => 26
    };
    let max_c3_c4_len = match version {
        MerlinVersion::Merlin8 => 64,
        _ => 80
    };
    let kind = curs.node().kind();
    let dstring_psops = ["psop_asc","psop_dci","psop_inv","psop_fls","psop_rev","psop_str","psop_strl"];
    if curs.node().is_missing() {
        let mut mess = "something is missing".to_string();
        let (node,more) = match curs.node().parent() {
            Some(p) => (p,[" after ",&p.to_sexp()].concat()),
            None => (curs.node(),"".to_string())
        };
        mess += &more;
        let syn_rng = lsp_range(extended_range(&node, ctx.line().len()),ctx.row(),ctx.col());
        push(syn_rng, &mess, lsp::DiagnosticSeverity::ERROR);
    } else if curs.node().is_error() {
        push(rng, &("syntax error: ".to_string() + &curs.node().to_sexp()), lsp::DiagnosticSeverity::ERROR);
    } else if ["global_label","local_label","var_label"].contains(&kind) {
        let txt = node_text(&curs.node(), ctx.line());
        if txt.len() > max_lab_len && version!=MerlinVersion::Merlin32 {
            push(rng,&format!("label is too long (max = {})",max_lab_len),DiagnosticSeverity::ERROR);
        }
    } else if ctx.case_sensitivity().is_some() {
        if kind.starts_with("op_") || kind.starts_with("psop_") {
            if txt != txt.to_uppercase() {
                push(rng, "settings require upper case mnemonics",ctx.case_sensitivity().unwrap());
            }
        }
    }
    else if kind == "imm_prefix" && txt.contains("^") && version == MerlinVersion::Merlin8 {
        push(rng, "bank byte requires Merlin 16/16+/32", DiagnosticSeverity::ERROR);
    }
    else if kind == "addr_prefix" && version == MerlinVersion::Merlin8 {
        push(rng, "address prefix requires Merlin 16/16+/32", DiagnosticSeverity::ERROR);
    }
    else if kind == "data_prefix" && txt.contains("^") && version == MerlinVersion::Merlin8 {
        push(rng, "bank byte requires Merlin 16/16+/32", DiagnosticSeverity::ERROR);
    }
    else if kind=="num_str_prefix" && (version==MerlinVersion::Merlin8 || version==MerlinVersion::Merlin16) {
        push(rng,"numerical string prefix requires Merlin 16+/32",DiagnosticSeverity::ERROR);
    }
    else if kind=="braced_aexpr" && (version==MerlinVersion::Merlin8 || version==MerlinVersion::Merlin16) {
        push(rng,"braced expressions require Merlin 16+/32",DiagnosticSeverity::ERROR);
    }
    else if kind.starts_with("cop_") && (version==MerlinVersion::Merlin8 || version==MerlinVersion::Merlin16) {
        push(rng,"operator requires Merlin 16+/32",DiagnosticSeverity::ERROR);
    }
    else if kind=="dstring" && version==MerlinVersion::Merlin32 {
        if !txt.starts_with("\"") && !txt.starts_with("'") {
            push(rng,"Merlin 32 strings use either single or double quotes",DiagnosticSeverity::ERROR);
        }
    }
    else if (kind=="nchar" || kind=="pchar") && version==MerlinVersion::Merlin32 {
        push(rng,"syntax requires Merlin 32 v1.1",DiagnosticSeverity::INFORMATION);
    }
    else if kind=="comment" && version!=MerlinVersion::Merlin32 {
        // there is a limit on the combined length of the third and fourth columns.
        if let Some(c3) = curs.node().prev_named_sibling() {
            if c3.kind().starts_with("arg_") && node_text(&c3, ctx.line()).len() + txt.len() > max_c3_c4_len {
                push(rng,&format!("columns 3 and 4 together are too long (max = {})",max_c3_c4_len),DiagnosticSeverity::ERROR);
            }
        }
    }
    else if kind=="heading" && txt.len() > 64 && version!=MerlinVersion::Merlin32 {
        push(rng,"comment is too long (max = 64)",DiagnosticSeverity::ERROR);
    }
    else if kind=="filename" {
        if let Some(child) = curs.node().named_child(0) {
            if child.kind() == "dos33" {
                push(rng,"name is valid for DOS 3.3, but not ProDOS",DiagnosticSeverity::WARNING);
            }
        }
    }

    // these may coincide with previous node types, so must be outside else if sequence
    if kind.starts_with("arg_") && txt.len() > max_c3_c4_len && version!=MerlinVersion::Merlin32 {
        push(rng, &format!("column 3 is too long (max = {})", max_c3_c4_len), DiagnosticSeverity::ERROR);
    } else if dstring_psops.contains(&kind) && (version==MerlinVersion::Merlin8 || version==MerlinVersion::Merlin16) {
        if let Some(arg) = curs.node().next_named_sibling() {
            let mut count = 0;
            let mut new_rng: Option<tree_sitter::Range> = None;
            let mut maybe_part = arg.named_child(0);
            while let Some(curr) = maybe_part {
                let rng_now = curr.range();
                if curr.kind()=="dstring" || curr.kind()=="hex_data" {
                    if new_rng.is_some() {
                        new_rng.as_mut().unwrap().end_point = rng_now.end_point;
                        new_rng.as_mut().unwrap().end_byte = rng_now.end_byte;
                    } else {
                        new_rng = Some(rng_now);
                    } 
                    count += 1;
                }
                maybe_part = curr.next_named_sibling();
            }
            if count>2 && new_rng.is_some() {
                push(lsp_range(new_rng.unwrap(),ctx.row(),ctx.col()),"extended string operand requires Merlin 16+/32",DiagnosticSeverity::ERROR);
            }
        }
    } else if kind == "arg_literal" || kind == "literal" {
        push(rng, "uninterpreted literal", DiagnosticSeverity::INFORMATION);
    }
}
