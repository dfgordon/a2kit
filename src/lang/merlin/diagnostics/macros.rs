use std::collections::HashSet;
use lsp_types as lsp;
use crate::lang::{Navigate,Navigation};
use crate::lang::server::basic_diag;
use super::super::Symbols;
use crate::lang::{node_text,lsp_range};
use super::context::Context;
use crate::DYNERR;

struct Substitutor {
    line: String,
    delta: usize,
    build: String,
    search: Vec<String>,
    replace: Vec<String>,
    types: Vec<String>,
    matched_args: HashSet<usize>
}

impl Substitutor {
    fn new(line: &str,search: Vec<String>,replace: Vec<String>,types: Vec<String>) -> Self {
        Self {
            line: line.to_owned(),
            delta: 0,
            build: String::new(),
            search,
            replace,
            types,
            matched_args: HashSet::new()
        }
    }
    fn result(&self) -> (String,HashSet<usize>) {
        (self.build.clone(),self.matched_args.clone())
    }
}

impl Navigate for Substitutor {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR> {
        let txt = node_text(&curs.node(),&self.line);
        let curr_len = curs.node().start_position().column + self.delta;
        if curr_len > self.build.len() {
            self.build += &" ".repeat(curr_len - self.build.len());
        }
        for i in 0..self.search.len() {
            if curs.node().kind() == self.types[i] &&  txt == self.search[i] {
                self.matched_args.insert(i);
                self.build += &self.replace[i];
                self.delta += self.replace[i].len() - self.search[i].len();
                return Ok(Navigation::GotoSibling);
            }
        }
        // append terminal nodes
        if curs.node().named_child_count() == 0 {
            self.build += &txt;
            return Ok(Navigation::GotoSibling);
        }
        Ok(Navigation::GotoChild)
    }
}

/// Make substitutions in a line.
/// The line will not be adjusted.  Line terminator is added if necessary.
/// Returns (updated line, set that was actually replaced)
fn substitute(parser: &mut tree_sitter::Parser, line: &str, search: &Vec<String>, replace: &Vec<String>, types: &Vec<String>)
-> Result<(String,HashSet<usize>),DYNERR> {
    let mut subs = Substitutor::new(line,search.clone(),replace.clone(),types.clone());
    if let Some(tree) = parser.parse(&line,None) {
        subs.walk(&tree)?;
    }
    let (build,matched_args) = subs.result();
    Ok((build,matched_args))
}

/// Substitute macro variables with arguments
/// * txt: text of the macro, parsing hints should already be present
/// * nodes: list of macro argument nodes
/// * call_source: text of the line where the macro is called
/// returns (expanded macro, set of variables that were actually used)
fn substitute_vars(txt: &str, nodes: &Vec<tree_sitter::Node>, call_source: &str) -> Result<(String,HashSet<usize>),DYNERR> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_merlin6502::language())?;
    let mut ans = String::new();
    let mut matches = HashSet::new();
    let mut search = Vec::new();
    let mut replace = Vec::new();
    let mut types = Vec::new();
    for i in 0..nodes.len() {
        search.push(format!("]{}",i + 1));
        replace.push(node_text(&nodes[i], call_source));
        types.push("var_mac".to_string());
    }
    // Search also for things that were not provided, but could be required,
    // if found all that happens is it goes into the returned matches.
    for i in nodes.len()..8 {
        search.push(format!("]{}",i + 1));
        replace.push(format!("]{}",i + 1));
        types.push("var_mac".to_string());
    }
    for line in txt.lines() {
        let terminated = line.to_string() + "\n";
        let (ln,partial) = substitute(&mut parser, &terminated, &search, &replace, &types)?;
        ans += &ln;
        ans += "\n";
        for mtch in partial {
            matches.insert(mtch);
        }
    }
    Ok((ans, matches))
}

/// Expand a macro reference assuming arguments have already been checked.
/// * node: must be a macro_ref node
/// * call_source: text of the line where the macro is called
/// * symbols: document symbols
/// * max_recursion: currently must be 1 otherwise panic
/// returns the expanded macro, or None if something went wrong
pub fn expand_macro(node: &tree_sitter::Node, call_source: &str, symbols: &Symbols, max_recursion: usize) -> Option<String> {
    if node.kind() != "macro_ref" {
        log::debug!("expand: wrong node type");
        return None;
    }
    if max_recursion != 1 {
        panic!("expand: max_recursion must be 1");
    }
    let label = node_text(node,call_source);
    if let Some(sym) = symbols.macros.get(&label) {
        if sym.defining_code.is_none() {
            log::debug!("expand: no macro text found");
            return None;
        }
        let next = node.next_named_sibling();
        let mut nodes = Vec::new();
        if next.is_some() && next.unwrap().kind() == "arg_macro" {
            let arg_count = next.unwrap().named_child_count();
            for i in 0..arg_count {
                nodes.push(next.unwrap().named_child(i).unwrap());
            }
        }
        if let Ok((expanded,_)) = substitute_vars(sym.defining_code.as_ref().unwrap(), &nodes, call_source) {
            return Some(expanded);
        }
    }
    log::debug!("expand: symbol not found");
    None
}

/// This expands a macro's arguments for diagnostic purposes.
/// For this check there is no need to recursively expand.
/// The expanded macro is not saved anywhere.
pub fn check_macro_args(node: &tree_sitter::Node, symbols: &mut Symbols, ctx: &mut Context, diag: &mut Vec<lsp::Diagnostic>) {
    if node.kind() != "macro_ref" {
        log::debug!("expand: wrong node type");
        return;
    }
    let (rng,txt) = ctx.node_spec(node);
    if let Some(sym) = symbols.macros.get(&txt) {
        if sym.defining_code.is_none() {
            log::debug!("expand: no macro text found");
            return;
        }
        let next = node.next_named_sibling();
        let mut arg_count = 0;
        let mut nodes = Vec::new();
        if next.is_some() && next.unwrap().kind() == "arg_macro" {
            arg_count = next.unwrap().named_child_count();
            for i in 0..arg_count {
                nodes.push(next.unwrap().named_child(i).unwrap());
            }
        }
        if let Ok((_,arg_matches)) = substitute_vars(sym.defining_code.as_ref().unwrap(), &nodes, ctx.line()) {
            for i in arg_count..8 {
                if arg_matches.contains(&i) {
                    diag.push(basic_diag(rng, &format!("argument missing: `]{}`",i+1),lsp::DiagnosticSeverity::ERROR));
                }
            }
            for i in 0..nodes.len() {
                if !arg_matches.contains(&i) {
                    let rng = lsp_range(nodes[i].range(), ctx.row(), ctx.col());
                    diag.push(basic_diag(rng, "argument not used",lsp::DiagnosticSeverity::WARNING));
                }
            }
        };
    }
}