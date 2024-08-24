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
use crate::lang::Navigation;
use crate::DYNERR;

mod syntax;
mod pseudo;
mod ops;

const FOLDS: [&str;11] = ["psop_do","psop_if","psop_else","psop_fin","psop_lup","psop_end_lup","psop_mac","psop_eom","psop_dum","psop_dend","psop_end"];

// simplified relative to version in labels.rs
fn eval_fold_expr(node: &tree_sitter::Node,pc: Option<usize>,symbols: &Symbols,ctx: &Context) -> i64 {
    let arg = match node.next_named_sibling() {
        Some(arg_node) => match node.kind() {
            "psop_if" => match assembly::eval_if(&arg_node, ctx.line()) {
                Ok(val) => val,
                Err(_) => 1
            },
            "psop_do" => match assembly::eval_expr(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => val,
                Err(_) => 1
            },
            _ =>  match assembly::eval_expr(&arg_node, ctx.line(), pc, symbols, ctx.curr_scope()) {
                Ok(val) => val,
                Err(_) => 1
            }
        },
        None => 1
    };
    arg
}

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
        curs: &TreeCursor, ctx: &mut Context, ws: &Workspace, symbols: &mut Symbols, diagnostics: &mut Vec<lsp::Diagnostic>)
        -> Result<Navigation,DYNERR> {

        let node = curs.node();
        let src = match ctx.curr_source() {
            Some(s) => s,
            None => return Ok(Navigation::Exit)
        };
        let (rng,_) = ctx.node_spec(&node);
        let loc = lsp::Location::new(src.doc.uri.clone(),rng);

        if FOLDS.contains(&node.kind()) {
            let arg = eval_fold_expr(&node, None, symbols, ctx);
            ctx.folding_range(&node.kind(), rng, loc, arg, None);
        }
        let (asm,_,_) = ctx.cond_asm();
        if !asm {
            // have to keep looking for folds
            return Ok(Navigation::GotoChild);
        }
        if (node.kind() == "psop_put" || node.kind() == "psop_use") && (src.typ==SourceType::Master || src.typ==SourceType::Module) {
            return Ok(Navigation::Descend);
        }
        syntax::visit(curs, ctx, diagnostics);
        self.op_sentry.visit(curs, ctx, diagnostics);
        self.psop_sentry.visit(curs, ctx, diagnostics);

        Ok(Navigation::GotoChild)
    }
}