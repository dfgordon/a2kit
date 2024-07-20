//! Module to format lines of Merlin code
//! 
//! There are three tasks here:
//! * format a line of code for beauty
//! * format a line of code for pasting into Merlin emulators
//! * format as the user types into an editor
//! 
//! In some cases the tokenizer is passed as an argument, because one
//! formatting strategy is to tokenize then detokenize.

use std::sync::Arc;
use lsp_types as lsp;
use crate::lang::{self, lsp_range, range_contains_pos, translate_pos, node_text};
use crate::lang::{Navigate,Navigation,Document};
use crate::DYNERR;

const RCH: &str = "unreachable was reached";

pub enum ColumnStyle {
    Pasteable,
    Variable,
    Tabs
}

/// Format line of code using given style.
/// This relies on being able to split the line on COLUMN_SEPARATOR, as is the
/// case after detokenization or disassembly. 
pub fn format_tokens(line: &str, style: &ColumnStyle, widths: [usize;3]) -> String {
	let cols = line.split(super::COLUMN_SEPARATOR);
	let mut ans = String::new();
	let mut col_idx = 0;
    match style {
        ColumnStyle::Pasteable => {
            for col in cols {
                ans += col;
                ans += " ";
            }
        },
        ColumnStyle::Tabs => {
            for col in cols {
                ans += col;
                ans += "\t";
            }
        },
        _ => {
            for col in cols {
                let mut prepadding = 0;
                if let Some(c) = col.chars().next() {
                    if c==';' {
                        for j in col_idx..3 {
                            prepadding += widths[j];
                        }
                    }
                }
                let w = match col_idx {
                    i if i<3 => widths[i],
                    _ => 1
                };
                let padding = match w as i32 - col.len() as i32 {
                    x if x<1 => 1,
                    x => x as usize
                };
                ans += &" ".repeat(prepadding);
                ans += col;
                ans += &" ".repeat(padding);
                col_idx += 1;
            }
        }
    }
    ans.trim_end().to_string()
}

pub fn format_for_paste(program: String, tokenizer: &mut super::tokenizer::Tokenizer) -> Result<String,DYNERR> {
    tokenizer.set_style(ColumnStyle::Pasteable);
    let img = tokenizer.tokenize(program)?;
    tokenizer.detokenize(&img)
}

pub fn format_range(program: String, sel: lsp::Range, tokenizer: &mut super::tokenizer::Tokenizer) -> Result<Vec<lsp::TextEdit>,DYNERR> {
    let mut formatted_range = String::new();
    let line_count = program.lines().count() as u32;
    tokenizer.set_style(ColumnStyle::Variable);
    let mut row = 0;
    for line in program.lines() {
        if sel.start==sel.end || (row >= sel.start.line && row <= sel.end.line) {
            let img = tokenizer.tokenize(line.to_string())?;
            formatted_range += &tokenizer.detokenize(&img)?;
        }
        row += 1;
    }
    let (start,end) = match sel.start==sel.end {
        true => (lsp::Position::new(0,0), lsp::Position::new(line_count,0)),
        false => (lsp::Position::new(sel.start.line,0),lsp::Position::new(sel.end.line+1,0))
    };
    Ok(vec![lsp::TextEdit::new(lsp::Range::new(start,end), formatted_range)])
}

/// Handles format on type only, other formatting is handled by module level functions, which
/// delegate parsing to the tokenizer.
pub struct Formatter
{
	parser: super::MerlinParser,
    normalized_line: String,
    search_pos: lsp::Position,
    verbatim: bool,
	widths: [usize;3],
	symbols: Arc<super::Symbols>,
    c2_regex: regex::Regex,
    c3_regex: regex::Regex,
    c4_regex: regex::Regex,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            parser: super::MerlinParser::new(),
            normalized_line: String::new(),
            search_pos: lsp::Position::new(0,0),
            verbatim: false,
            widths: [9,6,11],
            symbols: Arc::new(super::Symbols::new()),
            c2_regex: regex::Regex::new(r"^\S*\s+$").expect(RCH),
            c3_regex: regex::Regex::new(r"^\S*\s+\S+\s*$").expect(RCH),
            c4_regex: regex::Regex::new(r"^\S*\s+\S+\s+\S+\s*$").expect(RCH)
        }
    }
	pub fn set_config(&mut self,settings: &super::settings::Settings) {
		let c1: usize = settings.columns.c1.try_into().or::<usize>(Ok(9)).unwrap();
		let c2: usize = settings.columns.c2.try_into().or::<usize>(Ok(6)).unwrap();
		let c3: usize = settings.columns.c3.try_into().or::<usize>(Ok(11)).unwrap();
		self.widths = [c1,c2,c3];
	}
	pub fn use_shared_symbols(&mut self,sym: Arc<super::Symbols>) {
        self.symbols = sym;
    }
	fn set_verbatim(&mut self, doc: &Document, position: lsp::Position)
	{
		self.search_pos = position;
        self.verbatim = false;
        if let Some(line) = doc.text.lines().nth(position.line as usize) {
            if let Ok(tree) = self.parser.parse(line,&self.symbols) {
                self.normalized_line = self.parser.line().to_string();
                if let Ok(()) = self.walk(&tree) {
                    return;
                }
            }
        }
	}
    pub fn format_typing(&mut self, doc: &Document, position: lsp::Position, ch: &str) -> Vec<lsp::TextEdit> {
        let stop1 = self.widths[0];
        let stop2 = self.widths[0] + self.widths[1];
        let stop3 = self.widths[0] + self.widths[1] + self.widths[2];
        match ch {
            ";" => self.set_verbatim(doc, translate_pos(&position,0,-1)),
            _ => self.set_verbatim(doc, position.clone())
        };
        if self.normalized_line.starts_with(super::CALL_TOK) {
            self.normalized_line = self.normalized_line[super::CALL_TOK.len_utf8()..].to_string();
        }
        if self.normalized_line.len() < position.character as usize {
            return Vec::new();
        }
        if self.verbatim {
            return Vec::new();
        }
        if ch == ";" && (position.character<2 || self.normalized_line[position.character as usize-2..position.character as usize-1]==*" ") {
            if position.character as usize <= stop3 {
                let new_text = " ".repeat(stop3 - position.character as usize + 1) + ";";
                let rng = lsp::Range::new(translate_pos(&position, 0, -1),position);
                return vec![lsp::TextEdit::new(rng, new_text)];
            }
        }
        // Now that persistent spaces are in place we can safely use regex
        if ch == " " {
            let txt = &self.normalized_line[0..position.character as usize];
            let rng = lsp::Range::new(position.clone(),position.clone());
            if self.c2_regex.is_match(txt) && (position.character as usize) < stop1 {
                return vec![lsp::TextEdit::new(rng," ".repeat(stop1 - position.character as usize))];
            }
            if self.c3_regex.is_match(txt) && (position.character as usize) < stop2 {
                return vec![lsp::TextEdit::new(rng," ".repeat(stop2 - position.character as usize))];
            }
            if self.c4_regex.is_match(txt) && (position.character as usize) < stop3 {
                return vec![lsp::TextEdit::new(rng," ".repeat(stop3 - position.character as usize))];
            }
        }
        return Vec::new();
    }    
}

impl lang::Navigate for Formatter
{
    fn visit(&mut self, curs: &tree_sitter::TreeCursor) -> Result<Navigation,DYNERR>
    {
        // go down as deep as possible and see if we are in a verbatim node,
        // and also replace persistent white space with any non white space ASCII.
        if ["dstring","txt","literal"].contains(&curs.node().kind()) {
            let rng = lsp_range(curs.node().range(), self.search_pos.line as isize, self.parser.col_offset());
            let replacement = node_text(&curs.node(),self.parser.line()).replace(" ","A").replace("\t","A");
            self.normalized_line.replace_range(rng.start.character as usize..rng.end.character as usize,&replacement);
            if range_contains_pos(&rng, &self.search_pos) {
                self.verbatim = true;
            }
        }
		return Ok(Navigation::GotoChild);
    }
}
