
use tree_sitter;
use crate::lang::{node_integer, Error};
use super::super::Symbols;
use crate::DYNERR;

/// Evaluate an Applesoft expression if possible.
/// Usually literal expressions will evaluate Ok, others will produce Err.
pub fn eval_aexpr(node: &tree_sitter::Node, source: &str, symbols: &Symbols) -> Result<f64,DYNERR> {
    match node.kind() {
        "real" | "int" => {
            match node_integer::<f64>(&node, source) {
                Some(v) => Ok(v),
                None => Err(Box::new(Error::ParsingError))
            }
        },
        "unary_aexpr" => {
            if node.named_child_count() != 2 {
                Err(Box::new(Error::Syntax))
            } else {
                let raw = eval_aexpr(&node.named_child(1).unwrap(), source, symbols)?;
                if node.named_child(0).unwrap().kind() == "tok_minus" {
                    Ok(-raw)
                } else if node.named_child(0).unwrap().kind() == "tok_plus" {
                    Ok(raw)
                } else {
                    // must be logical not
                    Ok(match raw==0. { true => 1., false => 0.})
                }
            }
        },
        "binary_aexpr" => {
            if node.named_child_count() == 3 {
                let val1 = eval_aexpr(&node.named_child(0).unwrap(), source, symbols)?;
                let val2 = eval_aexpr(&node.named_child(2).unwrap(), source, symbols)?;
                match node.named_child(1).unwrap().kind() {
                    "tok_plus" => Ok(val1 + val2),
                    "tok_minus" => Ok(val1 - val2),
                    "tok_times" => Ok(val1 * val2),
                    "tok_div" => match val2 {
                        0. => Err(Box::new(Error::OutOfRange)),
                        _ => Ok(val1 / val2)
                    },
                    "tok_pow" => Ok(val1.powf(val2)),
                    "tok_or" => Ok(match val1!=0. || val2!=0. { true => 1., false => 0. }),
                    "tok_and" => Ok(match val1!=0. && val2!=0. { true => 1., false => 0. }),
                    "tok_less" => Ok(match val1 < val2 { true => 1., false => 0. }),
                    "tok_gtr" => Ok(match val1 > val2 { true => 1., false => 0. }),
                    "tok_eq" => Ok(match val1 == val2 { true => 1., false => 0. }),
                    _ => Err(Box::new(Error::ParsingError))
                }
            } else if node.named_child_count() == 4 {
                let val1 = eval_aexpr(&node.named_child(0).unwrap(), source, symbols)?;
                let val2 = eval_aexpr(&node.named_child(3).unwrap(), source, symbols)?;
                match (node.named_child(1).unwrap().kind(),node.named_child(2).unwrap().kind()) {
                    ("tok_less","tok_eq") => Ok(match val1 <= val2 { true => 1., false => 0. }),
                    ("tok_gtr","tok_eq") => Ok(match val1 >= val2 { true => 1., false => 0. }),
                    ("tok_gtr","tok_less") => Ok(match val1 != val2 { true => 1., false => 0. }),
                    ("tok_less","tok_gtr") => Ok(match val1 != val2 { true => 1., false => 0. }),
                    _ => Err(Box::new(Error::ParsingError))
                }
            } else {
                Err(Box::new(Error::ParsingError))
            }
        },
        "fcall" => {
            if node.named_child_count() == 2 {
                let x = eval_aexpr(&node.named_child(1).unwrap(), source, symbols)?;
                match node.named_child(0).unwrap().kind() {
                    "tok_sgn" => Ok(match x==0.0 { true => 0.0, false => x.signum()}),
                    "tok_int" => Ok(x.floor()),
                    "tok_abs" => Ok(x.abs()),
                    "tok_sqr" => Ok(x.sqrt()),
                    "tok_log" => Ok(x.ln()),
                    "tok_exp" => Ok(x.exp()),
                    "tok_cos" => Ok(x.cos()),
                    "tok_sin" => Ok(x.sin()),
                    "tok_tan" => Ok(x.tan()),
                    "tok_atn" => Ok(x.atan()),
                    _ => Err(Box::new(Error::ParsingError))
                }
            } else {
                Err(Box::new(Error::ParsingError))
            }
        },
        _ => Err(Box::new(Error::ParsingError))
    }
}
