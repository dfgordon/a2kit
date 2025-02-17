//! Test of Merlin diagnostics modules.

use super::super::{MerlinVersion,diagnostics};

#[cfg(test)]
fn test_diagnostics(prog_name: &str, vers: Option<MerlinVersion>, expected_messages: &[&str]) {
    use crate::lang::server::Analysis;
    let ws = std::env::current_dir().expect("no cwd").join("tests").join("merlin");
    let path = ws.join(prog_name);
    let ws_uri = lsp_types::Url::from_directory_path(ws).expect("could not create workspace URL");
    let doc = crate::lang::Document::from_file_path(&path).expect("failed to create doc");
    let mut analyzer = diagnostics::Analyzer::new();
    if let Some(v) = vers {
        let mut config = super::super::settings::Settings::new();
        config.version = v;
        analyzer.set_config(config);
    }
    analyzer.init_workspace(vec![ws_uri], Vec::new()).expect("could not init workspace");
    analyzer.analyze(&doc).expect("could not analyze");
    let diag_set = analyzer.get_diags(&doc);
    assert_eq!(diag_set.len(),expected_messages.len());
	for i in 0..diag_set.len() {
		assert_eq!(expected_messages[i],diag_set[i].message);
	}
}

mod processors {
    #[test]
    fn disable_65c02() {
        super::test_diagnostics("test-65c02-disabled.S", None, &[
            "macro is undefined",
            "macro is undefined",
            "macro name matches a disabled instruction",
            "macro name matches a disabled instruction"
        ]);
    }
    #[test]
    fn enable_65c02() {
        super::test_diagnostics("test-65c02-disabled.S", Some(super::MerlinVersion::Merlin32), &[
        ]);
    }
    #[test]
    fn disable_65816() {
        super::test_diagnostics("test-65816-disabled.S", None, &[
            "addressing mode disabled, use XC pseudo-op to enable"
        ]);
    }
    #[test]
    fn enable_65816() {
        super::test_diagnostics("test-65816-disabled.S", Some(super::MerlinVersion::Merlin32), &[
            "this would cause the XC count to exceed 2"
        ]);
    }
}

mod macros {
    #[test]
    fn matches_instruction() {
        super::test_diagnostics("test-matches-instruction.S", None, &[
            "macro name matches a mnemonic",
            "macro name matches a mnemonic",
            "folding range is never closed",
            "folding range is never closed",
            "macro is never referenced in current context",
            "macro is never referenced in current context",
        ]);
    }
    #[test]
    fn undefined() {
        super::test_diagnostics("test-mac-undefined.S", None, &[
            "macro is undefined",
            "macro is undefined",
            "macro is undefined"
        ]);
    }
    #[test]
    fn forward_ref() {
        super::test_diagnostics("test-mac-forward.S", None, &[
            "macro is forward referenced"
        ]);
    }
    #[test]
    fn context() {
        super::test_diagnostics("test-mac-context.S", None, &[
            "macro is never referenced in current context",
            "macro cannot be used here"
        ]);
    }
    #[test]
    fn termination() {
        super::test_diagnostics("test-mac-termination.S", None, &[
            "unmatched end of macro (EOM terminates all preceding MAC)",
            "macro is never referenced in current context",
        ]);
    }
    #[test]
    fn pseudo_ops() {
        super::test_diagnostics("test-mac-psops.S", None, &[
            "macro is never referenced in current context",
            "pseudo operation cannot be used in a macro",
            "pseudo operation cannot be used in a macro",
            "pseudo operation cannot be used in a macro"
        ]);
    }
    #[test]
    fn variables() {
        super::test_diagnostics("test-mac-vars.S", None, &[
            "macro substitution variable cannot label a line",
            "macro substitution variable cannot label a line",
            "extension cannot evaluate, assuming true",
            "macro substitution variable referenced outside macro",
            "macro is never referenced in current context",
            "macro substitution variable referenced outside macro",
            "macro substitution variable referenced outside macro",
        ]);
    }
    #[test]
    fn arguments() {
        super::test_diagnostics("test-mac-args.S", None, &[
            "uninterpreted literal"
        ]);
    }
    #[test]
    fn recursive_refs() {
        super::test_diagnostics("test-mac-recursive-refs.S", Some(super::MerlinVersion::Merlin32), &[
            "evaluation was deferred",
            "evaluation was deferred",
            "evaluation was deferred",
            "evaluation was deferred",
            "assembly disabled by DO",
            "evaluation was deferred",
            "forward reference check deferred",
            "macro is never referenced in current context",
        ]);
    }
    #[test]
    fn nested() {
        super::test_diagnostics("test-mac-nested.S", None, &[
            "label is undefined in this scope",
            "label is undefined in this scope",
        ]);
    }
}

mod declarations {
    #[test]
    fn undefined_global() {
        super::test_diagnostics("test-decs-un-glob.S", None, &[
            "global label is undefined"
        ]);
    }
    #[test]
    fn undefined_local() {
        super::test_diagnostics("test-decs-un-loc.S", None, &[
            "local label is not defined in this scope"
        ]);
    }
    #[test]
    fn forward_variable() {
        super::test_diagnostics("test-decs-fwd-var.S", None, &[
            "macro substitution variable cannot label a line",
            "variable is forward referenced"
        ]);
    }
    #[test]
    fn redefinitions() {
        super::test_diagnostics("test-decs-redefined.S", None, &[
            "redefinition of a global label",
            "redefinition of a local label"
        ]);
    }
}

mod locals {
    #[test]
    fn no_scope() {
        super::test_diagnostics("test-loc-noscope.S", None, &[
            "no global scope is defined yet"
        ]);
    }
    #[test]
    fn forbidden_pseudo_op() {
        super::test_diagnostics("test-loc-psops.S", None, &[
            "cannot use local label for EQU",
            "cannot use local label for ENT",
            "cannot use local label for EXT"
        ]);
    }
    #[test]
    fn local_in_macro() {
        super::test_diagnostics("test-loc-macro.S", None, &[
            "macro is never referenced in current context",
            "cannot use local labels in a macro"
        ]);
    }

}

mod equates {
    #[test]
    fn externals() {
        super::test_diagnostics("test-ext-equates.S", None, &[
            "pseudo-op argument is disabled for the selected Merlin version",
            "pseudo-op argument is disabled for the selected Merlin version",
            "pseudo-op argument is disabled for the selected Merlin version"
        ]);
    }
    #[test]
    fn externals32() {
        super::test_diagnostics("test-ext-equates.S", Some(super::MerlinVersion::Merlin32), &[
        ]);
    }
    #[test]
    fn entries() {
        super::test_diagnostics("test-ent-equates.S", None, &[
            "entry label declared, but never defined",
            "pseudo-op argument is disabled for the selected Merlin version",
            "pseudo-op argument is disabled for the selected Merlin version",
            "pseudo-op argument is disabled for the selected Merlin version"
        ]);
    }
    #[test]
    fn entries32() {
        super::test_diagnostics("test-ent-equates.S", Some(super::MerlinVersion::Merlin32), &[
            "entry label declared, but never defined",
        ]);
    }
}

mod forward {
    #[test]
    fn psop_fwd() {
        super::test_diagnostics("test-ill-fwd.S", None, &[
            "evaluation was deferred",
            "illegal forward reference",
            "forward reference check deferred"
        ])
    }
}