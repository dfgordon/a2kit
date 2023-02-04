//! ## Applesoft Renumber Module

use tree_sitter;
use tree_sitter_applesoft;
use crate::lang;
use crate::lang::Visit;
use std::collections::HashMap;
use log::{trace,error};
use crate::DYNERR;

struct Primaries {
    line: String,
    nums: Vec<usize>,
}

impl Visit for Primaries {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> lang::WalkerChoice {
        if let Some(parent) = curs.node().parent() {
            let grab_it = match (parent.kind(),curs.node().kind()) {
                ("line","linenum") => true,
                _ => false
            };
            if grab_it {
                self.push_linenum(curs).expect("unexpected parsing error");
                return lang::WalkerChoice::GotoParentSibling;
            }
        }
        return lang::WalkerChoice::GotoChild;
    }
}

impl Primaries {
    fn new() -> Self {
        Self {
            line: String::new(),
            nums: Vec::new()
        }
    }
    fn push_linenum(&mut self,curs: &tree_sitter::TreeCursor) -> Result<(),lang::LanguageError> {
        let txt = lang::node_text(curs.node(), &self.line).replace(" ","");
        trace!("found primary line number {}",txt);
        if let Ok(num) = usize::from_str_radix(&txt, 10) {
            self.nums.push(num);
            return Ok(());
        }
        Err(lang::LanguageError::Syntax)
    }
    fn get_primary_nums(&mut self,source: &str, parser: &mut tree_sitter::Parser) -> Vec<usize> {
        self.nums = Vec::new();
        for line in source.clone().lines() {
            if line.len()==0 {
                continue;
            }
            self.line = line.to_string() + "\n";
            let tree = parser.parse(&self.line,None).expect("error parsing");
            self.walk(&tree);
        }
        self.nums.clone()
    }
}

pub struct Renumberer {
    line: String,
    renumbered: String,
    last_end_point: Option<tree_sitter::Point>,
    map: HashMap<usize,usize>
}

impl Visit for Renumberer {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> lang::WalkerChoice {
        match curs.node().kind() {
            "linenum" => {
                let txt = lang::node_text(curs.node(),&self.line);
                trace!("line number node {}",txt);
                if let Ok(num) = usize::from_str_radix(&txt.replace(" ",""),10) {
                    if let Some(new_num) = self.map.get(&num) {
                        let leading = txt.len() - txt.trim_start().len();
                        let trailing = txt.len() - txt.trim_end().len();
                        let mut fmt_num = String::from(" ").repeat(leading);
                        fmt_num += &new_num.to_string();
                        fmt_num += &String::from(" ").repeat(trailing);
                        self.concat(&fmt_num,&curs.node());
                    } else {
                        self.concat(&txt,&curs.node());
                    }
                }
            },
            _ => if curs.node().child_count()==0 {
                let txt = lang::node_text(curs.node(),&self.line);
                self.concat(&txt,&curs.node());
            }
        };
        lang::WalkerChoice::GotoChild
    }
}

impl Renumberer {
    pub fn new() -> Self {
        Self {
            line: String::new(),
            renumbered: String::new(),
            last_end_point: None,
            map: HashMap::new()
        }
    }
    fn concat(&mut self,item: &str,node: &tree_sitter::Node) {
        // keep spaces the parser may have thrown out
        if let Some(end) = self.last_end_point {
            if node.start_position().column>end.column {
                self.renumbered += &" ".repeat(node.start_position().column-end.column);
            }
        }
        self.renumbered += item;
        self.last_end_point = Some(node.end_position());
    }
    /// Renumber all lines with number >= beg && number < end, as [start,start+step,...].
    /// If update_refs, update line number references globally (otherwise not at all).
    /// This function assumes the existing numbering is valid.
    pub fn renumber(&mut self,source: &str, beg: usize, end: usize, first: usize, step: usize) 
    -> Result<String,DYNERR> {
        self.renumbered = String::new();
        self.map = HashMap::new();
		let mut parser = tree_sitter::Parser::new();
		parser.set_language(tree_sitter_applesoft::language()).expect("error loading applesoft grammar");
        let mut primary_finder = Primaries::new();
		let primaries = primary_finder.get_primary_nums(source, &mut parser);
		// setup the mapping from old to new line numbers
        let mut curr = first;
		for i in 0..primaries.len() {
            if primaries[i]>=beg && primaries[i]<end {
                if curr > 63999 {
                    error!("attempt to create line {}, max is 63999",curr);
                    return Err(Box::new(lang::LanguageError::LineNumber));
                }
                if i > 0 && primaries[i-1] < beg && curr <= primaries[i-1] {
                    error!("mapping {} to {} violates lower bound",primaries[i],curr);
                    return Err(Box::new(lang::LanguageError::LineNumber));
                }
                if i < primaries.len()-1 && primaries[i+1] >= end && curr >= primaries[i+1] {
                    error!("mapping {} to {} violates upper bound",primaries[i],curr);
                    return Err(Box::new(lang::LanguageError::LineNumber));
                }
                trace!("mapping {} to {}",primaries[i],curr);
			    self.map.insert(primaries[i],curr);
                curr += step;
            }
        }
        for line in source.lines() {
            if line.len()==0 {
                self.renumbered += "\n";
                continue;
            }
            self.line = String::from(line) + "\n";
            let tree = parser.parse(&self.line,None).expect("error parsing");
            self.walk(&tree);
            self.renumbered += "\n";
        }
        Ok(self.renumbered.clone())
	}
}