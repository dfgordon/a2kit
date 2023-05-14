//! # Language Module
//! 
//! Objects that want to walk a syntax tree can provide the `Visit` trait.
//! Such objects can take some action depending on the status of `TreeCursor`.
//! Language specific operations such as tokenization are in the submodules.

pub mod applesoft;
pub mod integer;
pub mod merlin;

use tree_sitter;
use colored::*;
use thiserror::Error;
use std::fmt::Write as FormattedWriter;
use std::io;
use std::io::Read;
use std::io::Write;
use atty;
use log::{debug,error};

use crate::{STDRESULT,DYNERR};

pub enum WalkerChoice {
    GotoChild,
    GotoSibling,
    GotoParentSibling,
    Exit
}

#[derive(Error,Debug)]
pub enum Error {
    #[error("Syntax error")]
    Syntax,
    #[error("Invalid Line Number")]
    LineNumber,
    #[error("Tokenization error")]
    Tokenization,
    #[error("Detokenization error")]
    Detokenization
}

/// Get text of the node, source should be a single line.
/// Panics if the source text does not include the node's range.
pub fn node_text(node: tree_sitter::Node,source: &str) -> String {
    let rng = node.range();
    return String::from(&source[rng.start_point.column..rng.end_point.column]);
}

/// Starting in some stringlike context defined by `ctx`, where the trigger byte
/// has already been consumed, escape the remaining bytes within that context.
/// If there is a literal hex escape it is put as `\x5CxHH` where H is the hex.
/// Return the escaped string and the index to the terminator.
/// The terminator is not part of the returned string.
pub fn bytes_to_escaped_string(bytes: &[u8], offset: usize, terminator: &[u8], ctx: &str) -> (String,usize)
{
    assert!(ctx=="str" || ctx=="tok_data" || ctx=="tok_rem");
    const QUOTE: u8 = 34;
    const BACKSLASH: u8 = 92;
	let escaping_ascii = [10, 13];
	let mut ans = String::new();
	let mut idx = offset;
	let mut quotes = 0;
    if ctx=="str" {
        quotes += 1;
    }
	while idx < bytes.len() {
		if ctx == "tok_data" && bytes[idx] == 0 {
			break;
        }
		if ctx == "tok_data" && quotes % 2 == 0 && terminator.contains(&bytes[idx]) {
			break;
        }
		if ctx != "tok_data" && terminator.contains(&bytes[idx]) {
			break;
        }
		if bytes[idx] == QUOTE {
			quotes += 1;
        }
		if bytes[idx] == BACKSLASH && idx + 3 < bytes.len() {
            let is_hex = |x: u8| -> bool {
                x>=48 && x<=57 || x>=65 && x<=70 || x>=97 && x<=102
            };
            if bytes[idx+1]==120 && is_hex(bytes[idx+2]) && is_hex(bytes[idx+3]) {
                ans += "\\x5c";
            } else {
                ans += "\\";
            }
        } else if escaping_ascii.contains(&bytes[idx]) || bytes[idx] > 126 {
            let mut temp = String::new();
            write!(&mut temp,"\\x{:02x}",bytes[idx]).expect("unreachable");
            ans += &temp;
        } else {
			ans += std::str::from_utf8(&[bytes[idx]]).expect("unreachable");
        }
		idx += 1;
	}
	return (ans,idx);
}

pub trait Visit {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> WalkerChoice;
    fn walk(&mut self,tree: &tree_sitter::Tree)
    {
        let mut curs = tree.walk();
        let mut choice = WalkerChoice::GotoChild;
        while ! matches!(choice,WalkerChoice::Exit)
        {
            if matches!(choice,WalkerChoice::GotoChild) && curs.goto_first_child() {
                choice = self.visit(&curs);
            } else if matches!(choice,WalkerChoice::GotoParentSibling) && curs.goto_parent() && curs.goto_next_sibling() {
                choice = self.visit(&curs);
            } else if matches!(choice,WalkerChoice::GotoSibling) && curs.goto_next_sibling() {
                choice = self.visit(&curs);
            } else if curs.goto_next_sibling() {
                choice = self.visit(&curs);
            } else if curs.goto_parent() {
                choice = WalkerChoice::GotoSibling;
            } else {
                choice = WalkerChoice::Exit;
            }
        }
    }
}

pub struct SyntaxCheckVisitor {
    pub code: String,
    pub err_count: usize,
    pub curr_line: usize
}

impl SyntaxCheckVisitor {
    fn new(prog: String) -> Self {
        Self { code: prog, err_count: 0, curr_line: 0 }
    }
}

impl Visit for SyntaxCheckVisitor {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> WalkerChoice
    {
        if curs.node().is_error()
        {
            self.err_count += 1;
            let mut c = curs.clone();
            let b1 = c.node().start_byte();
            let b2 = c.node().end_byte();
            let mut l1 = b1;
            let mut l2 = b2;
            while c.goto_parent() {
                if c.node().kind()=="line" {
                    l1 = c.node().start_byte();
                    l2 = c.node().end_byte() - 1;
                }
            }
            eprintln!("{} row {} col {}","ERROR".red(),self.curr_line,b1);
            debug!("error bounds {} {} {} {}",l1,b1,b2,l2);
            debug!("line length {}",self.code.len());
            eprintln!("    {}{}{}",
                match self.code.get(l1..b1) { None => "???", Some(s) => s },
                match self.code.get(b1..b2) { None => "???".normal(), Some(s) => s.red().bold() },
                match self.code.get(b2..l2) { None => "???", Some(s) => s });
        }
        return WalkerChoice::GotoChild;
    }
}

/// Simple verify, returns an error if any issues
pub fn verify_str(lang: tree_sitter::Language,code: &str) -> STDRESULT {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang).expect("Error loading grammar");
    let mut visitor = SyntaxCheckVisitor::new(String::new());
    let mut iter = code.lines();
    while let Some(line) = iter.next()
    {
        let tree = parser.parse(String::from(line) + "\n",None).expect("Error parsing file");
        visitor.code = String::from(String::from(line) + "\n");
        if line.len()>0 {
            // if stdout is the console, format some, and include the s-expression per line
            visitor.walk(&tree);
        }
    }
    if visitor.err_count > 0 {
        return Err(Box::new(Error::Syntax));
    }
    Ok(())
}

/// detect syntax errors in any language.  Returns tuple with long and short result messages, or an error.
/// N.b. there is extra behavior in the event either stdin or stdout are the console.
pub fn verify_stdin(lang: tree_sitter::Language,prompt: &str) -> Result<(String,String),DYNERR>
{
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang).expect("Error loading grammar");
    let mut visitor = SyntaxCheckVisitor::new(String::new());
    let mut code = String::new();
    let mut res = String::new();
    let mut bottom_line = String::new();
    // if stdin is the console, collect line entry specially
    if atty::is(atty::Stream::Stdin)
    {
        eprintln!("Line entry interface.");
        eprintln!("This is a blind accumulation of lines.");
        eprintln!("Verify occurs when entry is terminated.");
        eprintln!("Accumulated lines can be piped.");
        eprintln!("`bye` terminates.");
        loop {
            eprint!("{} ",prompt);
            let mut line = String::new();
            io::stderr().flush().expect("could not flush stderr");
            io::stdin().read_line(&mut line).expect("could not read stdin");
            if line=="bye\n" || line=="bye\r\n" {
                break;
            }
            code += &line;
        }
    }
    else
    {
        io::stdin().read_to_string(&mut code).expect("could not read stdin");
    }
    let mut iter = code.lines();
    while let Some(line) = iter.next()
    {
        let tree = parser.parse(String::from(line) + "\n",None).expect("Error parsing file");
        visitor.code = String::from(line);
        if line.len()>0 {
            // if stdout is the console, format some, and include the s-expression per line
            if atty::is(atty::Stream::Stdout) {
                if res.len()==0 {
                    res += "\n";
                }
                res += &(line.to_string() + "\n" + &tree.root_node().to_sexp() + "\n");
            } else {
                res += &(line.to_string() + "\n");
            }
            visitor.walk(&tree);
        }
        visitor.curr_line += 1;
    }
    if visitor.err_count==0 {
        writeln!(bottom_line,"\u{2713} {}","Syntax OK".to_string().green()).expect("formatting error");
        return Ok((res,bottom_line));
    } else {
        // following message is not used, perhaps pack it into the error type?
        writeln!(res,"\u{2717} {} ({})","Syntax Errors".to_string().red(),visitor.err_count).expect("formatting error");
        return Err(Box::new(Error::Syntax));
    }
}