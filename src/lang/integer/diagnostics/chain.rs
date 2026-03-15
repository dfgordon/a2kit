
use crate::lang::{node_text};

/// Parse a literal string that may be a DOS CHAIN command, if yes return Some(name),
/// where name is the name of the program being chained to.
fn parse_chain_literal(s: &str, skip_chain: bool) -> Option<String> {
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

    // extract program name
    let mut end = s.len();
    if s.ends_with("\"") {
        end -= 1;
    }
    Some(s[col..end].trim_start().to_owned())
}

/// From a statement_print_str node see if this is a CHAIN pattern.
/// Return None or Some(name) where name is the program being chained to.
/// It would require extensive flow analysis to capture every possible CHAIN.
/// We assume Integer BASIC is always running from DOS (no paths).
/// The hook trigger can be a string variable or a literal ASCII 4
pub fn test_chain(node: &tree_sitter::Node,source: &str) -> Option<String> {
    if node.kind() == "statement_print_str" {
        let mut stage = 0;
        let mut next = node.clone();
        while let Some(n) = next.next_named_sibling() {
            next = n;
            if next.kind().starts_with("sep_") {
                next = match next.next_named_sibling() {
                    Some(n) => n,
                    None => {return None;}
                }
            }
            match stage {
                0 => {
                    // Find the trigger
                    if next.kind() == "str_name" {
                        stage = 1;
                    } else if next.kind() == "string" {
                        let txt = node_text(&next,source);
                        if txt.trim_start().starts_with("\"\u{0004}") {
                            let Some(slice) = txt.trim_start().get(2..) else { return None; };
                            return parse_chain_literal(slice,false);
                        }
                    } else {
                        return None;
                    }
                },
                1 => {
                    // Find the CHAIN literal and maybe program name
                    if next.kind() == "string" {
                        let txt = node_text(&next,source);
                        return parse_chain_literal(txt.trim_start(),false);
                    }
                },
                _ => break
            }
        } 
        None
    } else {
        None
    }
}
