
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use crate::lang::merlin::context::Context;
use crate::lang::merlin::{MerlinVersion,ProcessorType};

pub struct OpSentry {
    program_line: usize,
    xc_count: usize,
    xc_appearances: usize
}

impl OpSentry
{
    pub fn new(ctx: &Context) -> Self {
        Self {
            program_line: 0,
            xc_count: match ctx.merlin_version() {
                MerlinVersion::Merlin8 => 0,
                _ => 2
            },
            xc_appearances: 0
        }
    }
    pub fn visit(&mut self,curs: &TreeCursor, ctx: &mut Context, diagnostics: &mut Vec<lsp::Diagnostic>) {
        let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
            diagnostics.push(crate::lang::server::basic_diag(rng,mess,severity));
        };
        let op_book = ctx.op_handbook();
		let node = curs.node();
        let (rng,txt) = ctx.node_spec(&node);
		let parent = node.parent();
		let next = node.next_named_sibling();
		if parent.is_some() && parent.unwrap().kind()=="source_file" && node.kind()!="heading" {
			self.program_line += 1;
        }
		if node.kind()=="psop_xc" {
			self.xc_appearances += 1;
			if self.xc_appearances!=self.program_line {
				push(rng,"XC pseudo-ops must appear first and be uninterrupted",lsp::DiagnosticSeverity::ERROR);
            }
			if next.is_some() && ctx.node_match(&next.unwrap(), "OFF") {
				self.xc_count = 0;
            }
			else {
				self.xc_count += 1;
            }
			if self.xc_count>2 {
				self.xc_count = 2;
				push(rng,"this would cause the XC count to exceed 2",lsp::DiagnosticSeverity::ERROR);
			}
            log::trace!("xc updated to {}",self.xc_count);
            // the XC value stored in the Context is updated in the first pass
			return;
		}
		if self.xc_count==2 { // all modes are valid so no further checks during this visit
			return;
        }
		let processor = match self.xc_count { 0 => ProcessorType::_6502 , _ => ProcessorType::_65c02 };
		if node.kind().starts_with("op_") {
            if !op_book.weak_match(&txt,&processor) {
                push(rng,"instruction is disabled, use XC pseudo-op to enable",lsp::DiagnosticSeverity::ERROR);
            }
        } else if node.kind()=="macro_ref" {
            if op_book.strong_match(&txt,&ProcessorType::_65c816) {
				push(rng,"macro name matches a disabled instruction",lsp::DiagnosticSeverity::INFORMATION);
            }
			return;
		} else if node.kind().starts_with("arg_") {
            if let Some(addr_node) = node.named_child(0) {
                log::trace!("checking {}",&node.kind());
                if let Some(op) = op_book.get(&node.kind()[4..]) {
                    for mode in &op.modes {
                        let proc_ok = mode.processors.contains(&processor);
                        let mode_ok = mode.get_reduced_mode() == addr_node.kind();
                        if proc_ok && mode_ok {
                            return;
                        }
                    }
                    if parent.is_some() && !parent.unwrap().has_error() {
                        push(rng,"addressing mode disabled, use XC pseudo-op to enable",lsp::DiagnosticSeverity::ERROR);                            
                    }
                }
            }
        }
	}
}
