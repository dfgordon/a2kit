//! # Module for handling line numbers
//! 
//! A task we want to support for any BASIC dialect is renumbering.
//! This provides tools that any dialect should be able to use.

use tree_sitter;
use lsp_types::{TextEdit,Range,Position};
use crate::lang;
use std::collections::{HashSet,BTreeMap};
use log::{trace,debug};
use crate::DYNERR;

#[derive(Clone)]
pub struct LabelInformation {
    pub rng: Range,
    pub leading_space: usize,
    pub trailing_space: usize
}

/// Add edits to change line labels
fn apply_mapping(new_num: usize,info: &LabelInformation) -> TextEdit {
    let mut fmt_num = " ".repeat(info.leading_space);
    fmt_num += &new_num.to_string();
    fmt_num += &" ".repeat(info.trailing_space);
    TextEdit::new(info.rng,fmt_num)
}

pub trait Renumber {
    /// Default method should usually suffice
    fn get_one(curs: &tree_sitter::TreeCursor,source: &str,row: isize) -> Result<(usize,LabelInformation),DYNERR> {
        if let Some(num) = lang::node_integer(&curs.node(), source) {
            let txt = lang::node_text(&curs.node(), source);
            trace!("found line number {} at row {}",num,row);
            return Ok((num, LabelInformation {
                rng: lang::lsp_range(curs.node().range(),row,0),
                leading_space: txt.len() - txt.trim_start().len(),
                trailing_space: txt.len() - txt.trim_end().len()
            }));
        }
        Err(Box::new(lang::Error::Syntax))
    }
    /// This must build a map from line labels to information about all defining nodes (usually just one).
    /// Once you have the cursor on the label, you can usually call the default `get_one` to form the map value.
    fn gather_defs(&mut self, source: &str, row: isize) -> Result<BTreeMap<usize,Vec<LabelInformation>>,DYNERR>;
    /// This must build a map from line labels to information about all reference nodes.
    /// Once you have the cursor on the label, you can usually call the default `get_one` to form the map value.
    fn gather_refs(&mut self, source: &str, row: isize) -> Result<BTreeMap<usize,Vec<LabelInformation>>,DYNERR>;
    /// Build edits to renumber and possibly move a selected range of lines.
    /// This does the heavy lifting, the default should work for most BASIC dialects.
    /// If you want to apply the returned edits outside the LSP context, use `crate::lang::apply_edits`.
    /// If the result would lead to interleaving of selected and unselected lines, an error is returned.
    /// If `allow_move`, the lines will be re-ordered as necessary.
    /// If `!allow_move` and the lines need to be re-ordered, an error is returned.
    fn build_edits(&mut self,all_txt: &str, ext_sel: Option<Range>, start: &str, step: &str, update_refs: bool, allow_move: bool, min_num: usize, max_num: usize,
        external_refs: HashSet<usize>)
        -> Result<Vec<TextEdit>,String> {
        let line_sep = match all_txt.lines().count() == all_txt.split("\r\n").count() {
            true => "\r\n",
            false => "\n"
        };

        // parse client parameters
        let l0 = match start.parse::<usize>() { Ok(n) => n, Err(_) => return Err("invalid start".to_string()) };
        let dl = match step.parse::<usize>() { Ok(n) => n, Err(_) => return Err("invalid step".to_string()) };
        if l0 < min_num || l0 > max_num { return Err("invalid start".to_string()) }
        if dl < 1 || dl > max_num { return Err("invalid step".to_string()) }
        let lines = all_txt.lines().collect::<Vec<&str>>();
        let end_pos = Position::new(
            lines.len() as u32 - 1,
            lines[ lines.len()-1 ].chars().count() as u32
        );
        let sel = match ext_sel {
            Some(raw) => {
                let mut sel = raw; 
                if sel.end.character==0 && sel.end.line > sel.start.line {
                    sel.end.line -= 1;
                    sel.end.character = lines[sel.end.line as usize].chars().count() as u32;
                }
                sel
            },
            None => Range::new(Position::new(0,0),end_pos.clone())
        };
        let mut sel_txt = "".to_string();
        for l in sel.start.line..=sel.end.line {
            sel_txt += lines[l as usize];
            sel_txt += line_sep;
        }

        // gather line number info
        log::debug!("gather primaries in selection");
        let sel_primaries = match self.gather_defs(&sel_txt,sel.start.line as isize) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };
        log::debug!("gather secondaries in selection");
        let sel_secondaries = match self.gather_refs(&sel_txt,sel.start.line as isize) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather secondaries".to_string())
        };
        log::debug!("gather all primaries");
        let all_primaries = match self.gather_defs(all_txt,0) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };
        log::debug!("gather all secondaries");
        let all_secondaries = match self.gather_refs(all_txt,0) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather secondaries".to_string())
        };
        if sel_primaries.len()<1 {
            return Err("no primaries to change".to_string());
        }

        // build new primaries and mapping
        let mut mapping = BTreeMap::new();
        let mut new_primaries = Vec::new();
        for (primary,info) in &sel_primaries {
            if info.len() != 1 {
                return Err(format!("duplicated primary line number {}",primary));
            }
            if new_primaries.len() == 0 {
                if external_refs.contains(primary) && *primary != l0 {
                    return Err("proposed renumbering would change an external reference".to_string());
                }
                new_primaries.push(l0);
                mapping.insert(*primary,l0);
            } else {
                if external_refs.contains(primary) {
                    new_primaries.push(*primary);
                    mapping.insert(*primary,*primary);
                } else {
                    let prev = *new_primaries.last().unwrap();
                    new_primaries.push(prev + dl);
                    mapping.insert(*primary,prev+dl);
                }
            }
        }
        let ln = match new_primaries.last() {
            Some(l) if *l <= max_num => *l,
            Some(_) => return Err(format!("upper bound of {} exceeded",max_num)),
            None => return Err("renumber failed with no new primaries - this is a bug".to_string())
        };

        // find the insertion point
        let mut insert_pos = Position::new(0,0);
        for (primary,info) in &all_primaries {
            if info.len() != 1 {
                return Err(format!("duplicated primary line number {}",primary));
            }
            if info[0].rng.start.line >= sel.start.line && info[0].rng.end.line <= sel.end.line {
                // moves or interleaves not possible within selection
                continue;
            }
            if *primary < l0 && info[0].rng.start.line >= insert_pos.line {
                insert_pos = Position::new(info[0].rng.start.line + 1,0);
            }
            if *primary >= l0 && *primary <= ln {
                return Err(format!("existing line {} is within proposed range",*primary));
            }
        }

        // if there are blank lines we need to push forward the insert position past them
        for (row,line) in all_txt.lines().enumerate() {
            if insert_pos.line as usize == row && line.trim_start().len()==0 {
                insert_pos.line += 1;
            }
        }
        if !allow_move && insert_pos.line != sel.start.line {
            return Err("renumber parameters would require a move".to_string());
        }

        // build edits within the selection only
        let mut sel_edits = Vec::new();
        for (primary,info) in &sel_primaries {
            if let Some(new_num) = mapping.get(primary) {
                sel_edits.push(apply_mapping(*new_num, &info[0]));
            }
        }
        if update_refs {
            for (secondary,info) in &sel_secondaries {
                for item in info {
                    if let Some(new_num) = mapping.get(secondary) {
                        sel_edits.push(apply_mapping(*new_num, item));
                    }
                }
            }
        }

        // build edits outside the selection only
        let mut unsel_edits = Vec::new();
        if update_refs {
            for (secondary,info) in &all_secondaries {
                for item in info {
                    if let Some(new_num) = mapping.get(secondary) {
                        if item.rng.start.line < sel.start.line || item.rng.end.line > sel.end.line {
                            unsel_edits.push(apply_mapping(*new_num, item));
                        }
                    }
                }
            }    
        }

        // If we are moving, we have to apply the edits within the selection here and now, so
        // we can insert a string constant at the new position.
        // Returning overlapping edits, insertions, and deletions doesn't work.
        let mut ans: Vec<TextEdit> = Vec::new();
        if insert_pos.line != sel.start.line {
            debug!("applying pre-edits for move");
            let updated_sel = match lang::apply_edits(&sel_txt, &sel_edits, sel.start.line) {
                Ok(s) => s,
                Err(_) => return Err("apply edits failed".to_string())
            };
            ans.push(TextEdit::new(Range::new(end_pos,end_pos),line_sep.to_string()));
            ans.push(TextEdit::new(Range::new(insert_pos,insert_pos),updated_sel));
            for l in sel.start.line..=sel.end.line {
                let old_rng = Range::new(Position::new(l,0),Position::new(l+1,0));
                ans.push(TextEdit::new(old_rng,"".to_string()));
            }
            ans.append(&mut unsel_edits);
        } else
        // if we are not moving, then just return all the edits
        {
            ans.append(&mut sel_edits);
            ans.append(&mut unsel_edits);
        }
        Ok(ans)
    }
}

