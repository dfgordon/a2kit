//! # Module for handling line numbers
//! 
//! A task we want to support for any BASIC dialect is renumbering.
//! This provides tools that any dialect should be able to use.

use tree_sitter;
use lsp_types::{TextEdit,Range,Position};
use crate::lang;
use std::collections::BTreeMap;
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

pub trait LineNumberTool {
    /// Default method should usually suffice
    fn get_info(curs: &tree_sitter::TreeCursor,source: &str,row: isize) -> Result<(usize,LabelInformation),DYNERR> {
        if let Some(num) = lang::node_integer(&curs.node(), source) {
            let txt = lang::node_text(&curs.node(), source);
            trace!("found line number {}",num);
            return Ok((num, LabelInformation {
                rng: lang::lsp_range(curs.node().range(),row,0),
                leading_space: txt.len() - txt.trim_start().len(),
                trailing_space: txt.len() - txt.trim_end().len()
            }));
        }
        Err(Box::new(lang::Error::Syntax))
    }
    /// This must build a map from line labels to information about the line label.
    /// Once you have the cursor on the label, you can usually call the default `get_info` to form the map value.
    /// Each BASIC will have its own way of performing the gather, so there is no default method.
    fn gather(&mut self,source: &str, row: isize, primaries: bool, secondaries: bool) -> Result<BTreeMap<usize,LabelInformation>,DYNERR>;
    /// Build edits to renumber and possibly move a selected range of lines.
    /// This does the heavy lifting, the default should work for most BASIC dialects.
    /// If you want to apply the returned edits outside the LSP context, use `crate::lang::apply_edits`.
    /// If the result would lead to interleaving of selected and unselected lines, an error is returned.
    /// If `allow_move`, the lines will be re-ordered as necessary.
    /// If `!allow_move` and the lines need to be re-ordered, an error is returned.
    fn renumber_edits(&mut self,all_txt: &str, ext_sel: Option<Range>, start: &str, step: &str, update_refs: bool, allow_move: bool, min_num: usize, max_num: usize)
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
        let sel_primaries = match self.gather(&sel_txt,sel.start.line as isize, true, false) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };
        let sel_secondaries = match self.gather(&sel_txt,sel.start.line as isize, false, true) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };
        let all_primaries = match self.gather(all_txt,0, true, false) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };
        let all_secondaries = match self.gather(all_txt,0, false, true) {
            Ok(result) => result,
            Err(_) => return Err("unable to gather primaries".to_string())
        };

        // check for errors
        if sel_primaries.len()<1 {
            return Err("no primaries to change".to_string());
        }
        let ln = l0 + dl*(sel_primaries.len()-1);
        if ln > max_num {
            return Err(format!("upper bound of {} exceeded",max_num));
        }
        let mut insert_pos = Position::new(0,0);
        for (primary,info) in &all_primaries {
            if info.rng.start.line >= sel.start.line && info.rng.end.line <= sel.end.line {
                // moves or interleaves not possible within selection
                continue;
            }
            if *primary < l0 && info.rng.start.line >= insert_pos.line {
                insert_pos = Position::new(info.rng.start.line + 1,0);
            }
            if *primary >= l0 && *primary <= ln {
                return Err(format!("existing line {} is within proposed range",*primary));
            }
        }
        if !allow_move && insert_pos.line != sel.start.line {
            return Err("renumber parameters would require a move".to_string());
        }

        // setup the mapping from old to new line numbers
        let mut mapping = BTreeMap::new();
        let mut label_now = l0;
        for primary in sel_primaries.keys() {
            mapping.insert(*primary,label_now);
            label_now += dl;
        }

        // build edits within the selection only
        let mut sel_edits = Vec::new();
        for (primary,info) in &sel_primaries {
            if let Some(new_num) = mapping.get(primary) {
                sel_edits.push(apply_mapping(*new_num, info));
            }
        }
        if update_refs {
            for (secondary,info) in &sel_secondaries {
                if let Some(new_num) = mapping.get(secondary) {
                    sel_edits.push(apply_mapping(*new_num, info));
                }
            }
        }

        // build edits outside the selection only
        let mut unsel_edits = Vec::new();
        if update_refs {
            for (secondary,info) in &all_secondaries {
                if let Some(new_num) = mapping.get(secondary) {
                    if info.rng.start.line < sel.start.line || info.rng.end.line > sel.end.line {
                        unsel_edits.push(apply_mapping(*new_num, info));
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

