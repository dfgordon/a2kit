//! test of Applesoft diagnostics module

use regex::Regex;
use super::diagnostics;

#[cfg(test)]
fn test_diagnostics(prog_name: &str, expected_messages: &[&str]) {
    use crate::lang::server::Analysis;
    let path = std::env::current_dir().expect("no cwd").join("tests").join("applesoft").join(prog_name);
    let mut analyzer = diagnostics::Analyzer::new();
    let doc = crate::lang::Document::from_file_path(&path).expect("failed to create document");
    analyzer.analyze(&doc).expect("could not analyze");
    let diag_set = analyzer.get_diags(&doc);
    assert_eq!(diag_set.len(),expected_messages.len());
	for i in 0..diag_set.len()
	{
        let patt = Regex::new(expected_messages[i]).expect("bad regex");
		assert!(patt.is_match(&diag_set[i].message));
	}
}

#[test]
fn undeclared() {
    test_diagnostics("test-undeclared.abas", &[
        "array is never DIM'd",
        "array is never DIM'd",
        "array is never DIM'd",
        "array is never DIM'd",
        "array is never DIM'd"
    ]);
}

#[test]
fn unassigned() {
    test_diagnostics("test-unassigned.abas", &[
        "variable is never assigned",
        "variable is never assigned",
        "variable is never assigned",
        "variable is never assigned"
    ]);
}

#[test]
fn collisions() {
    test_diagnostics("test-collisions.abas", &[
        r"variable name collision:\s*(PIES,PI|PI,PIES)",
        r"variable name collision:\s*(MYWRD\$,MYCLR\$|MYCLR\$,MYWRD\$)",
        r"variable name collision:\s*(CUBE,CUTE|CUTE,CUBE)"
    ]);
}

#[test]
fn range_errors() {
    test_diagnostics("test-ranges.abas", &[
        r"Out of range \(0,63999\)",
        r"Out of range \(-1\.7e38,1\.7e38\)",
        r"Out of range \(-32767,32767\)",
        r"Out of range \(0,7\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(0,255\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(-32767,65535\)",
        r"Out of range \(0,255\)",
        r"Out of range \(0,255\)"
    ]);
}

#[test]
fn line_numbers() {
    test_diagnostics("test-lines.abas", &[
        "Line number out of order",
        r"\(ERROR \(tok_minus\)\)",
        "Line does not exist",
        "Line does not exist",
        "Line does not exist",
        "Line does not exist",
        "Line does not exist",
        "Line does not exist"
    ]);
}

#[test]
fn user_functions() {
    test_diagnostics("test-functions.abas", &[
        "function is redefined",
        "function never defined"
    ]);
}

#[test]
fn data() {
    test_diagnostics("test-data.abas", &[
        "Odd quote parity in literal on multi-statement line invites trouble"
    ]);
}
