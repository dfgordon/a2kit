use lsp_types as lsp;
use tree_sitter::TreeCursor;
use crate::lang::merlin::ProcessorType;

use crate::lang::merlin::context::Context;
use crate::lang::merlin::MerlinVersion;


pub struct PseudoOpSentry {
    def_found: bool,
    org_found: bool,
    rel_found: bool,
    op_found: bool
}

impl PseudoOpSentry {
    pub fn new() -> Self {
        Self {
            def_found: false,
            org_found: false,
            rel_found: false,
            op_found: false
        }
    }
	pub fn visit(&mut self,curs: &TreeCursor, ctx: &mut Context, diagnostics: &mut Vec<lsp::Diagnostic>)
	{
        let mut push = |rng: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity| {
            diagnostics.push(crate::lang::server::basic_diag(rng,mess,severity));
        };
		let node = curs.node();
        let (rng,txt) = ctx.node_spec(&node);
        let prev = node.prev_named_sibling();

		// ordering of conditionals is supposed to promote efficiency

		if node.kind() == "label_def" {
			self.def_found = true;
        } else if node.kind()=="operation" {
			self.op_found = true;
        } else if node.kind().starts_with("psop_") {
			if node.kind().ends_with("equ") && prev.is_none() {
				push(rng, "must provide label", lsp::DiagnosticSeverity::ERROR);
			} else if node.kind().ends_with("org") {
                if self.rel_found {
				    push(rng,"REL and ORG should not appear in the same file",lsp::DiagnosticSeverity::ERROR);
                }
				self.org_found = true;
			} else if node.kind().ends_with("rel") {
				if self.org_found {
					push(rng,"REL and ORG should not appear in the same file",lsp::DiagnosticSeverity::ERROR);
                }
                if self.def_found {
					push(rng,"REL appears after one or more label definitions",lsp::DiagnosticSeverity::ERROR);			
                }
                self.rel_found = true;
			} else if node.kind().ends_with("obj") && self.op_found {
				push(rng, "OBJ should not appear after start of code",lsp::DiagnosticSeverity::ERROR);
			} else if node.kind().ends_with("ext") || node.kind().ends_with("exd") || node.kind().ends_with("ent") {
				let operand = node.next_named_sibling().is_some() && node.next_named_sibling().unwrap().kind()!="comment";
				if prev.is_some() && operand {
					push(rng,"use either column 1 or 3 for the label(s), not both",lsp::DiagnosticSeverity::ERROR);
                }
				if prev.is_none() && !operand {
					push(rng,"must provide label(s) in either column 1 or 3",lsp::DiagnosticSeverity::ERROR);
                }
			} else if node.kind().ends_with("mx") && ctx.curr_proc()!=ProcessorType::_65c816 && ctx.curr_proc()!=ProcessorType::_65802 {
                push(rng,"MX should not be used with 8 bit processor", lsp::DiagnosticSeverity::ERROR);
            }
            if !ctx.psop_handbook().weak_match(&txt,&ctx.merlin_version()) {
                push(rng,"pseudo-op is disabled for the selected Merlin version",lsp::DiagnosticSeverity::ERROR);
            }
            if let Some(psop_info) = ctx.psop_handbook().get(&txt) {
                log::trace!("check {}",&node.kind());
                let maybe_re = match ctx.merlin_version() {
                    MerlinVersion::Merlin8 => psop_info.v8x,
                    MerlinVersion::Merlin16 => psop_info.v16x,
                    _ => None
                };
                if let Some(re) = maybe_re {
                    if let Some(arg) = node.next_named_sibling() {
                        if arg.kind().starts_with("arg_") {
                            let mut next = arg.named_child(0);
                            while next.is_some() {
                                let (arg_rng,arg_txt) = ctx.node_spec(&next.unwrap());
                                if re.find(&arg_txt).is_some() {
                                    push(arg_rng, "pseudo-op argument is disabled for the selected Merlin version", lsp::DiagnosticSeverity::ERROR);
                                }
                                next = next.unwrap().next_named_sibling();
                            }
                        }
                    }
                }
            }
        } else if node.kind()=="macro_ref" {
			if ctx.psop_handbook().strong_match(&txt,&MerlinVersion::Merlin16Plus) {
				push(rng,"macro name matches a disabled pseudo-op",lsp::DiagnosticSeverity::INFORMATION);
			}
		}
    }
}
