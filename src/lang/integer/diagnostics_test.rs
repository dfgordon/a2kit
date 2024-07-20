//! test of Integer diagnostics module

use regex::Regex;
use super::diagnostics;

#[cfg(test)]
fn test_diagnostics(prog_name: &str, expected_messages: &[&str]) {
    use crate::lang::server::Analysis;
    let path = std::env::current_dir().expect("no cwd").join("tests").join("integerbasic").join(prog_name);
    let mut analyzer = diagnostics::Analyzer::new();
    let doc = crate::lang::Document::from_file_path(&path).expect("failed to create doc");
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
fn long_line() {
    test_diagnostics("breakout.ibas", &[
        "Line may be too long"
    ]);
}

#[test]
fn collisions() {
    test_diagnostics("test-collisions.ibas", &[
        "illegal variable name",
        "illegal variable name",
        "illegal variable name",
        "illegal variable name",
        "string is never DIM'd",
        "illegal variable name",
        "string is never DIM'd",
        "illegal variable name",
        "something is missing"
    ]);
}

#[test]
fn functions() {
    test_diagnostics("test-functions.ibas", &[
        r"\(ERROR",
        r"\(ERROR",
        r"\(ERROR",
        r"\(ERROR",
        r"\(ERROR",
        r"\(ERROR"
    ]);
}

#[test]
fn lines() {
    test_diagnostics("test-lines.ibas", &[
        // first pass
        "Line number out of order",
        // second pass
        r"\(ERROR",
        r"Line does not exist",
        r"Line does not exist",
        r"Line does not exist"
    ]);
}

#[test]
fn ranges() {
    test_diagnostics("test-ranges.ibas", &[
        "Out of range",
        "Out of range",
        "Out of range",
        "Out of range",
        "Out of range"
    ]);
}

#[test]
fn unassigned() {
    test_diagnostics("test-unassigned.ibas", &[
        "variable is never assigned",
        "variable is never assigned",
        "variable is never assigned",
        "variable is never assigned",
        "variable is never assigned"
    ]);
}

#[test]
fn undeclared() {
    test_diagnostics("test-undeclared.ibas", &[
        "array is never DIM'd",
        "variable is never assigned",
        "array is never DIM'd",
        "variable is never assigned",
        "unsubscripted integer array",
        "variable is never assigned",
        "string is never DIM'd",
        "array is never DIM'd",
        "array is never DIM'd",
        "variable is never assigned",
        "unsubscripted integer array"
    ]);
}
