//! ## Applesoft Renumber Module

use tree_sitter;
use tree_sitter_applesoft;
use lsp_types::{Range,Position,TextEdit};
use crate::lang;
use crate::lang::Navigate;
use crate::lang::linenum::{LabelInformation,LineNumberTool};
use std::collections::BTreeMap;
use log::{error,debug};
use crate::{STDRESULT,DYNERR};

pub mod flags {
    pub const REORDER: u64 = 1;
    pub const PASS_OVER_REFS: u64 = 2;
}

pub struct Renumberer {
    flags: u64,
    parser: tree_sitter::Parser,
    row: isize,
    line: String,
    info: BTreeMap<usize,LabelInformation>,
    primaries: bool,
    secondaries: bool
}

impl Navigate for Renumberer {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<lang::Navigation,DYNERR> {
        if let Some(parent) = curs.node().parent() {
            if curs.node().kind() == "linenum" {
                let grab_it = match (parent.kind()=="line",self.primaries,self.secondaries) {
                    (true,true,false) => true, // only primaries
                    (false,false,true) => true, // only secondaries
                    (_,true,true) => true, //either
                    _ => false
                };
                if grab_it {
                    self.push_linenum(curs)?;
                    return Ok(lang::Navigation::GotoSibling);
                }
            }
        }
        return Ok(lang::Navigation::GotoChild);
    }
}

impl LineNumberTool for Renumberer {
    fn gather(&mut self,source: &str, row: isize, primaries: bool, secondaries: bool) -> Result<BTreeMap<usize,LabelInformation>,DYNERR> {
        self.primaries = primaries;
        self.secondaries = secondaries;
        self.info = BTreeMap::new();
        self.row = row;
        for line in source.lines() {
            if line.len()==0 {
                self.row += 1;
                continue;
            }
            self.line = line.to_string() + "\n";
            match self.parser.parse(&self.line,None) {
                Some(tree) => self.walk(&tree)?,
                None => return Err(Box::new(lang::Error::ParsingError))
            }
            self.row += 1;
        }
        Ok(self.info.clone())
    }
}

impl Renumberer {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_applesoft::language()).expect("could not load TS language");
        Self {
            flags: 0,
            parser,
            row: 0,
            line: String::new(),
            info: BTreeMap::new(),
            primaries: true,
            secondaries: true
        }
    }
    fn push_linenum(&mut self,curs: &tree_sitter::TreeCursor) -> STDRESULT {
        match Self::get_info(curs,&self.line,self.row) {
            Ok((num,info)) => {
                self.info.insert(num,info);
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    pub fn set_flags(&mut self,flags:u64) {
        self.flags = flags;
    }
    /// Get edits for LSP, this simply wraps the trait default
    pub fn get_edits(&mut self,all_txt: &str, ext_sel: Option<Range>, start: &str, step: &str)
    -> Result<Vec<TextEdit>,String> {
        self.renumber_edits(all_txt,ext_sel,start,step,
            self.flags & flags::PASS_OVER_REFS == 0,
            self.flags & flags::REORDER > 0, 0, 63999)
    }
    /// Renumber all lines with number >= beg && number < end, as [start,start+step,...].
    /// References are updated globally.
    /// This function assumes the existing numbering is valid.
    pub fn renumber(&mut self,source: &str, beg: usize, end: usize, first: usize, step: usize) -> Result<String,DYNERR> {
        let all_primaries = match self.gather(source,0, true, false) {
            Ok(result) => result,
            Err(_) => return Err(Box::new(lang::Error::LineNumber))
        };
        let [mut l0,mut ln] = [0x10000,0];
        for (num,label) in &all_primaries {
            if *num >= beg && l0 > label.rng.start.line {
                l0 = label.rng.start.line;
            }
            if *num < end && ln < label.rng.start.line {
                ln = label.rng.start.line;
            }
        }
        debug!("renumber rows {} to {}",l0,ln);
        let ext_sel = match l0 <= ln {
            true => Some(Range::new(Position::new(l0,0),Position::new(ln+1,0))),
            false => None
        };
        match self.renumber_edits(source,ext_sel,
            &first.to_string(),
            &step.to_string(),
            self.flags & flags::PASS_OVER_REFS == 0,
            self.flags & flags::REORDER > 0, 0, 63999) {
            Ok(edits) => {
                debug!("apply the main edits");
                match lang::apply_edits(source, &edits, 0) {
                    Ok(ans) => Ok(ans),
                    Err(e) => {
                        error!("apply edits failed");
                        Err(e)
                    }
                }
            },
            Err(s) => {
                error!("{}",s);
                Err(Box::new(lang::Error::LineNumber))
            }
        }
	}
}