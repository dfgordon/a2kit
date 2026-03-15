
use tree_sitter;
use crate::lang::{node_integer, Error};
use super::super::Symbols;
use crate::{STDRESULT,DYNERR};

/// The Integer parser is designed for tokenization and therefore has named separators including brackets.
/// What we want to do is pretend they are not there by going to the next non-separator sibling.
fn skip_bracket(node: &mut tree_sitter::Node,skip_first: bool) -> STDRESULT {
    if skip_first {
        if let Some(nxt) = node.next_named_sibling() {
            *node = nxt;
        } else {
            return Err(Box::new(Error::ParsingError));
        }
    }
    while node.kind() == "open_aexpr" || node.kind() == "close" || node.kind() == "open_fcall" {
        if let Some(nxt) = node.next_named_sibling() {
            *node = nxt;
        } else {
            return Err(Box::new(Error::ParsingError));
        }
    }
    Ok(())
}

/// Evaluate an Integer expression if possible.
/// Usually literal expressions will evaluate Ok, others will produce Err.
pub fn eval_aexpr(node0: &tree_sitter::Node, source: &str, symbols: &Symbols) -> Result<i64,DYNERR> {
    let mut node = node0.clone();
    skip_bracket(&mut node,false)?;
    match node.kind() {
        "integer" => {
            match node_integer::<i64>(&node, source) {
                Some(v) => Ok(v),
                None => Err(Box::new(Error::ParsingError))
            }
        },
        "unary_aexpr" => {
            if node.named_child_count() < 2 {
                Err(Box::new(Error::Syntax))
            } else {
                let raw = eval_aexpr(&node.named_child(1).unwrap(), source, symbols)?;
                if node.named_child(0).unwrap().kind() == "op_unary_minus" {
                    Ok(-raw)
                } else if node.named_child(0).unwrap().kind() == "op_unary_plus" {
                    Ok(raw)
                } else {
                    // must be logical not
                    Ok(match raw==0 { true => 1, false => 0})
                }
            }
        },
        "binary_aexpr" => {
            if node.named_child_count() < 3 {
                return Err(Box::new(Error::ParsingError));
            }
            let mut n = node.named_child(0).unwrap();
            skip_bracket(&mut n,false)?;
            let val1 = eval_aexpr(&n, source, symbols)?;
            skip_bracket(&mut n,true)?;
            let op = n.kind().to_string();
            skip_bracket(&mut n,true)?;
            let val2 = eval_aexpr(&n, source, symbols)?;
            match op.as_str() {
                "op_plus" => Ok(val1 + val2),
                "op_minus" => Ok(val1 - val2),
                "op_times" => Ok(val1 * val2),
                "op_div" => match val2 {
                    0 => Err(Box::new(Error::OutOfRange)),
                    _ => Ok(val1 / val2)
                },
                "op_mod" => match val2 {
                    0 => Err(Box::new(Error::OutOfRange)),
                    _ => Ok(val1 % val2)
                },
                "op_pow" => Ok(val1.pow(u32::try_from(val2)?)),
                "tok_aeq" => Ok(match val1 == val2 { true => 1, false => 0}),
                "op_aneq" | "op_neq" => Ok(match val1 == val2 { true => 0, false => 1 }),
                "op_or" => Ok(match val1!=0 || val2!=0 { true => 1, false => 0}),
                "op_and" => Ok(match val1!=0 && val2!=0 { true => 1, false => 0}),
                "op_less" => Ok(match val1 < val2 { true => 1, false => 0}),
                "op_lesseq" => Ok(match val1 <= val2 { true => 1, false => 0}),
                "op_gtr" => Ok(match val1 > val2 { true => 1, false => 0}),
                "op_gtreq" => Ok(match val1 >= val2 { true => 1, false => 0}),
                _ => Err(Box::new(Error::ParsingError))
            }
        },
        "fcall" => {
            if node.named_child_count() < 3 {
                return Err(Box::new(Error::ParsingError));
            }
            let x = eval_aexpr(&node.named_child(1).unwrap(), source, symbols)?;
            match node.named_child(0).unwrap().kind() {
                "fcall_sgn" => Ok(match x==0 { true => 0, false => x.signum()}),
                "fcall_abs" => Ok(x.abs()),
                _ => Err(Box::new(Error::ParsingError))
            }
        },
        _ => Err(Box::new(Error::ParsingError))
    }
}
