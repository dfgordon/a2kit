//! ## Integer BASIC Renumber Module

use tree_sitter;
use tree_sitter_integerbasic;
use lsp_types::{Range,Position,TextEdit};
use crate::lang;
use crate::lang::Navigate;
use crate::lang::linenum::{LabelInformation,Renumber};
use std::collections::{HashSet,BTreeMap};
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
    info: BTreeMap<usize,Vec<LabelInformation>>,
    primaries: bool,
    secondaries: bool,
    external_refs: HashSet<usize>
}

impl Navigate for Renumberer {
    fn visit(&mut self,curs: &tree_sitter::TreeCursor) -> Result<lang::Navigation,DYNERR> {
        let mut grab_it = false;
        if let Some(parent) = curs.node().parent() {
            if curs.node().kind() == "linenum" {
                grab_it |= match (parent.kind()=="line",self.primaries,self.secondaries) {
                    (true,true,false) => true, // only primaries
                    (false,false,true) => true, // only secondaries
                    (_,true,true) => true, //either
                    _ => false
                };
            }
        }
        if let Some(prev) = curs.node().prev_named_sibling() {
            if self.secondaries && curs.node().kind() == "integer" {
                if ["statement_goto","statement_gosub","statement_then_line"].contains(&prev.kind()) {
                    grab_it |= true;
                }
            }
        }
        if grab_it {
            self.push_linenum(curs)?;
            return Ok(lang::Navigation::GotoSibling);
        }
        return Ok(lang::Navigation::GotoChild);
    }
}

impl Renumber for Renumberer {
    fn gather_defs(&mut self,source: &str, row: isize) -> Result<BTreeMap<usize,Vec<LabelInformation>>,DYNERR> {
        self.primaries = true;
        self.secondaries = false;
        self.info = BTreeMap::new();
        self.row = row;
        for line in source.lines() {
            if line.trim().len()==0 {
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
    fn gather_refs(&mut self,source: &str, row: isize) -> Result<BTreeMap<usize,Vec<LabelInformation>>,DYNERR> {
        self.primaries = false;
        self.secondaries = true;
        self.info = BTreeMap::new();
        self.row = row;
        for line in source.lines() {
            if line.trim().len()==0 {
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
        parser.set_language(&tree_sitter_integerbasic::LANGUAGE.into()).expect("could not load TS language");
        Self {
            flags: 0,
            parser,
            row: 0,
            line: String::new(),
            info: BTreeMap::new(),
            primaries: true,
            secondaries: true,
            external_refs: HashSet::new()
        }
    }
    fn push_linenum(&mut self,curs: &tree_sitter::TreeCursor) -> STDRESULT {
        match Self::get_one(curs,&self.line,self.row) {
            Ok((num,new_info)) => {
                if !self.info.contains_key(&num) {
                    self.info.insert(num,vec![new_info]);
                } else {
                    if let Some(val) = self.info.get_mut(&num) {
                        val.push(new_info);
                    }
                }
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    pub fn set_flags(&mut self,flags:u64) {
        self.flags = flags;
    }
	pub fn set_external_refs(&mut self,externals: Vec<usize>) {
        self.external_refs = HashSet::new();
		for linnum in externals {
			self.external_refs.insert(linnum);
		}
	}
    /// Get edits for LSP, this simply wraps the trait default
    pub fn get_edits(&mut self,all_txt: &str, ext_sel: Option<Range>, start: &str, step: &str)
    -> Result<Vec<TextEdit>,String> {
        self.build_edits(all_txt,ext_sel,start,step,
            self.flags & flags::PASS_OVER_REFS == 0,
            self.flags & flags::REORDER > 0, 0 , 32767, self.external_refs.clone())
    }
    /// Renumber all lines with number >= beg && number < end, as [start,start+step,...].
    /// References are updated globally.
    /// This function assumes the existing numbering is valid.
    pub fn renumber(&mut self,source: &str, beg: usize, end: usize, first: usize, step: usize) -> Result<String,DYNERR> {
        let all_primaries = match self.gather_defs(source,0) {
            Ok(result) => result,
            Err(_) => return Err(Box::new(lang::Error::LineNumber))
        };
        let [mut l0,mut ln] = [0x10000,0];
        for (num,label) in &all_primaries {
            if label.len() != 1 {
                log::error!("duplicated primary line number");
                return Err(Box::new(crate::lang::Error::LineNumber));
            }
            if *num >= beg && l0 > label[0].rng.start.line {
                l0 = label[0].rng.start.line;
            }
            if *num < end && ln < label[0].rng.start.line {
                ln = label[0].rng.start.line;
            }
        }
        debug!("renumber rows {} to {}",l0,ln);
        let ext_sel = match l0 <= ln {
            true => Some(Range::new(Position::new(l0,0),Position::new(ln+1,0))),
            false => None
        };
        match self.build_edits(source,ext_sel,
            &first.to_string(),
            &step.to_string(),
            self.flags & flags::PASS_OVER_REFS == 0,
            self.flags & flags::REORDER > 0, 0, 32767, self.external_refs.clone()) {
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