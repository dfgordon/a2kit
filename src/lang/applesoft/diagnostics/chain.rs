
use crate::lang::{node_text, node_integer};

/// From an sfcall node see if this is `CHR$(4)`
fn detect_chr4(node: &tree_sitter::Node,source: &str) -> bool {
    let Some(n1) = node.named_child(0) else { return false; };
    let Some(n2) = node.named_child(1) else { return false; };
    let Some(ascii) = node_integer::<isize>(&n2,source) else { return false; };
    n1.kind() == "tok_chr" && ascii == 4
}

/// Parse a literal string that may be a ProDOS CHAIN command, if we find a path and line
/// return Some((program_name,line)). Empty program name means check subsequent expressions.
fn parse_chain_literal(s: &str, skip_chain: bool) -> Option<(String,isize)> {
    let mut col = 0;
    if s.starts_with("\"") {
        col += 1;
    }
    let mut getchr = || -> &str {
        while col < s.len() {
            col += 1;
            if &s[col-1..col] != " " {
                return &s[col-1..col];
            }
        }
        return "";
    };

    // rather than build a regex just parse the CHAIN directly
    if !skip_chain {
        if getchr().to_ascii_uppercase() != "C" {
            return None;
        }
        if getchr().to_ascii_uppercase() != "H" {
            return None;
        }
        if getchr().to_ascii_uppercase() != "A" {
            return None;
        }
        if getchr().to_ascii_uppercase() != "I" {
            return None;
        }
        if getchr().to_ascii_uppercase() != "N" {
            return None;
        }
    }

    // extract program path and maybe starting line
    let mut end = s.len();
    if s.ends_with("\"") {
        end -= 1;
    }
    let path_and_line = s[col..end].trim_start().to_owned();
    if path_and_line.starts_with(",") {
        return None;
    }
    if path_and_line.contains(",") {
        let mut iter = path_and_line.split(",");
        let prog = iter.next().unwrap();
        let line = iter.next().unwrap().trim_start();
        if line.starts_with("@") {
            if let Ok(num) = isize::from_str_radix(&line[1..],10) {
                return Some((prog.to_owned(),num));
            }
        }
        Some((prog.to_owned(),-1))
    } else {
        Some((path_and_line.to_owned(),-1))
    }
}

/// From a tok_print or tok_call node, see if this is a CHAIN pattern.
/// Return None or Some((name,line)) where name is the program (or path) being chained to,
/// and line is the line number if one was specified, or a negative number if not.
/// It would require extensive flow analysis to capture every possible CHAIN.
/// For DOS 3.x, the only pattern accepted is `CALL 520"<PROG>"`.
/// For ProDOS, we parse the expressions following PRINT with the following allowances:
/// * The hook trigger can be a string variable, `CHR$(4)`, or a literal ASCII 4
/// * There can be a string variable between CHAIN and the final literal part of the path
/// * The line number can be an expression, but then it will not be evaluated
pub fn test_chain(node: &tree_sitter::Node,source: &str) -> Option<(String,isize)> {
    let Some(parent) = node.parent() else { return None; };
    if node.kind() == "tok_call" && parent.named_child_count() == 3 {
        let Some(addr) = node_integer::<isize>(&parent.named_child(1).unwrap(), source) else { return None; };
        let str = parent.named_child(2).unwrap();
        if addr == 520 && str.kind() == "str" {
            Some((node_text(&str,source).trim_matches('"').to_string(),-1))
        } else {
            None
        }
    } else if node.kind() == "tok_print" {
        let mut stage = 0;
        let mut next = node.clone();
        let mut col = 0;
        while let Some(n) = next.next_named_sibling() {
            next = n;
            match stage {
                0 => {
                    // Find the trigger
                    if next.kind() == "sfcall" && detect_chr4(&next, source) {
                        stage = 1;
                    } else if next.kind() == "var_str" {
                        stage = 1;
                    } else if next.kind() == "str" {
                        let txt = node_text(&next,source);
                        if txt.trim_start().starts_with("\"\u{0004}") {
                            next = next.prev_named_sibling().unwrap();
                            stage = 1;
                            col = 2;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                },
                1 => {
                    // Find the CHAIN literal and maybe program name
                    if next.kind() == "str" {
                        let txt = node_text(&next,source);
                        let Some(slice) = txt.trim_start().get(col..) else { return None; };
                        match parse_chain_literal(slice,false) {
                            Some(result) if result.0.len() > 0 => return Some(result),
                            Some(_) => {
                                stage = 2;
                            },
                            None => return None
                        }
                    } else {
                        return None;
                    }
                },
                2 => {
                    // Accept a string variable as an unknown path fragment
                    if next.kind() == "var_str" {
                        stage = 3;
                    } else {
                        return None;
                    }
                },
                3 => {
                    // Now only the literal path will do
                    let txt = node_text(&next,source);
                    return parse_chain_literal(txt.trim_start(),true);
                },
                _ => break
            }
        } 
        None
    } else {
        None
    }
}
