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
use log::error;

pub enum WalkerChoice {
    GotoChild,
    GotoSibling,
    GotoParentSibling,
    Exit
}

#[derive(Error,Debug)]
pub enum LanguageError {
    #[error("Syntax error")]
    Syntax,
    #[error("Invalid Line Number")]
    LineNumber,
}

/// Get text of the node, source should be a single line.
/// Panics if the source text does not include the node's range.
pub fn node_text(node: tree_sitter::Node,source: &str) -> String {
    let rng = node.range();
    return String::from(&source[rng.start_point.column..rng.end_point.column]);
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
    pub err_count: usize
}

impl SyntaxCheckVisitor {
    fn new(prog: String) -> Self {
        Self { code: prog, err_count: 0 }
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
            let mut depth = 0;
            let mut indent = String::from("");
            while c.goto_parent() {
                depth += 1;
                indent += "  ";
            }
            let mess = match depth {
                2 => String::from("ERROR on line"),
                3 => String::from("ERROR in statement"),
                _ => String::from("ERROR within statement")
            };
            eprintln!("{}{} {} {}",indent,mess.red(),self.code.get(b1..b2).expect("none").yellow().bold(),curs.node().to_sexp());
        }
        return WalkerChoice::GotoChild;
    }
}

/// Simple verify, returns an error if any issues
pub fn verify_str(lang: tree_sitter::Language,code: &str) -> Result<(),LanguageError> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang).expect("Error loading grammar");
    let mut visitor = SyntaxCheckVisitor::new(String::new());
    let mut iter = code.lines();
    while let Some(line) = iter.next()
    {
        let tree = parser.parse(String::from(line) + "\n",None).expect("Error parsing file");
        visitor.code = String::from(line);
        if line.len()>0 {
            // if stdout is the console, format some, and include the s-expression per line
            visitor.walk(&tree);
        }
    }
    if visitor.err_count > 0 {
        return Err(LanguageError::Syntax);
    }
    Ok(())
}

/// detect syntax errors in any language.  Returns tuple with long and short result messages, or an error.
/// N.b. there is extra behavior in the event either stdin or stdout are the console.
pub fn verify_stdin(lang: tree_sitter::Language,prompt: &str) -> Result<(String,String),LanguageError>
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
            code += &(line + "\n");
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
    }
    if visitor.err_count==0 {
        writeln!(bottom_line,"\u{2713} {}","Syntax OK".to_string().green()).expect("formatting error");
        return Ok((res,bottom_line));
    } else {
        // following message is not used, perhaps pack it into the error type?
        writeln!(res,"\u{2717} {} ({})","Syntax Errors".to_string().red(),visitor.err_count).expect("formatting error");
        return Err(LanguageError::Syntax);
    }
}