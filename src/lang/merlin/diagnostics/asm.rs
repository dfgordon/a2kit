//! Analyzer final pass
//! 
//! In this pass we might assemble someday, probably leveraging the Assembler object
//! in crate::lang::merlin::assembly.  At present this only evaluates symbol expressions
//! in order to calculate conditional assembly in common situations.

use lsp_types as lsp;
use tree_sitter::TreeCursor;
use crate::lang::merlin::context::Context;
use crate::lang::merlin::assembly;
use super::super::{Symbols,Workspace,SourceType};
use crate::lang::{Navigation,lsp_range};
use crate::DYNERR;

mod syntax;
mod pseudo;
mod ops;

const FOLD_STARTERS: [&str;7] = ["psop_do","psop_if","psop_else","psop_lup","psop_mac","psop_dum","psop_end"];
const FOLD_ENDERS: [&str;4] = ["psop_fin","psop_end_lup","psop_eom","psop_dend"];

pub struct Asm {
    op_sentry: ops::OpSentry,
    psop_sentry: pseudo::PseudoOpSentry,
}

impl Asm {
    pub fn new(ctx: &Context) -> Self {
        Self {
            op_sentry: ops::OpSentry::new(ctx),
            psop_sentry: pseudo::PseudoOpSentry::new()
        }
    }
    pub fn visit(&mut self,
        curs: &TreeCursor, ctx: &mut Context, ws: &Workspace, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>, folding: &mut Vec<lsp::FoldingRange>)
        -> Result<Navigation,DYNERR> {

        let node = curs.node();
        let (rng,_) = ctx.node_spec(&node);
        let src = match ctx.curr_source() {
            Some(s) => s,
            None => return Ok(Navigation::Exit)
        };
        let loc = lsp::Location::new(src.doc.uri.clone(),rng);

        let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
            diagnostics.push(crate::lang::server::basic_diag(rng,mess,severity));
        };

        if (node.kind() == "psop_put" || node.kind() == "psop_use") && (src.typ==SourceType::Master || src.typ==SourceType::Module) {
            return Ok(Navigation::Descend);
        } else if FOLD_STARTERS.contains(&node.kind()) {
            let arg = match node.next_named_sibling() {
                Some(arg_node) => match assembly::eval_expr(&arg_node, &src.line, None, symbols, None) {
                    Ok(val) => val,
                    Err(_) => {
                        if node.kind()=="psop_do" || node.kind()=="psop_if" {
                            push(lsp_range(arg_node.range(),ctx.row(),ctx.col()),"extension cannot evaluate, assuming true",lsp::DiagnosticSeverity::WARNING);
                        }
                        1
                    }
                },
                None => 1
            };
            match ctx.enter_folding_range(node.kind(), rng, loc, arg) {
                Some(d) => push(d.range,&d.message,d.severity.unwrap()),
                None => {}
            }
        } else if FOLD_ENDERS.contains(&node.kind()) {
            match ctx.exit_folding_range(&node.kind(), rng, loc) {
                Err(d) => push(d.range,&d.message,d.severity.unwrap()),
                Ok(Some(fold)) => folding.push(fold),
                Ok(None) => {}
            }
        }
        syntax::visit(curs, ctx, diagnostics);
        self.op_sentry.visit(curs, ctx, diagnostics);
        self.psop_sentry.visit(curs, ctx, diagnostics);

        Ok(Navigation::GotoChild)
    }
}