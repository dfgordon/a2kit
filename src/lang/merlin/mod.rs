//! # Merlin analysis and transformations
//! 
//! This module is used by both the CLI and the language server.
//! The Merlin parser is provided by `tree_sitter_merlin6502`.  Every file will be parsed as Merlin 16+,
//! other Merlin versions are handled via diagnostic filters.
//! The server compiles to a separate executable, its entry point is in `src/bin/server-merlin.rs`.
//! 
//! The analyzer performs functions that begin to resemble assembly, such as resolving
//! file relationships and identifying symbols.  There is a spot assembler that is used to aid in
//! disassembly. As of this writing, however, full assembly is not supported.
//! 
//! We test against Merlin 8 and Merlin 32.
//! Test coverage is 100% in terms of instructions and addressing modes available on
//! all processor variants (barring human error in constructing the tests).
//! We rely on documentation to glean Merlin 16/16+ behavior.
//! In theory a2kit should conform to the selected Merlin version.
//! Table of known or interesting distinctions follows.
//! 
//! Operation | Merlin 8 | Merlin 16+ | Merlin 32
//! ----------|----------|-----------|------
//! DCI | invert end of dstring | M8 | invert end of whole argument
//! INV | wrong lower case | correct lower case | refuses lower case (1)
//! FLS | wrong lower case | wrong lower case | refuses lower case
//! FLS | flashes specials | flashes specials | does not flash specials
//! REV | hex ignored | unknown | hex incorporated into string (2)
//! DS | bit shift not allowed | unknown | bit shift works
//! JML | n/a | unknown | cannot produce $DC opcode
//!
//! 1. lower case is only invertible in the alternate character set
//! 2. a2kit will reject trailing hex as a syntax error, but the REV processor would reverse each dstring separately if the parser allowed it

use lsp_types as lsp;
use std::collections::{HashSet,HashMap};
use std::fmt;

use super::node_text;

mod context;
pub mod settings;
pub mod checkpoint;
pub mod hovers;
pub mod completions;
pub mod tokenizer;
pub mod formatter;
pub mod assembly;
pub mod disassembly;
pub mod diagnostics;
pub mod semantic_tokens;
pub mod handbook;

#[cfg(test)]
mod tests;

const RCH: &str = "unreachable was reached";
const COLUMN_SEPARATOR: char = '\u{0100}';
const CALL_TOK: char = '\u{0100}';

pub mod symbol_flags {
    /// entry label (exported)
    pub const ENT: u64 = 0x01;
    /// external label (imported)
    pub const EXT: u64 = 0x02;
    /// label is a subroutine
    pub const SUB: u64 = 0x04;
    /// label is a macro
    pub const MAC: u64 = 0x08;
    /// label is a macro argument such as `]1`
    pub const ARG: u64 = 0x10;
    /// label is a local
    pub const LOC: u64 = 0x20;
    /// label is a macro local
    pub const MLC: u64 = 0x40;
    /// label is a variable
    pub const VAR: u64 = 0x80;
}

/// Stackable source types, e.g., we may have a nesting such as (Master (Put (MacroRef)) (MacroRef))
#[derive(PartialEq,Clone)]
pub enum SourceType {
    Master,
    Module,
    UseAndPut,
    Use,
    Put,
    MacroRef,
    Linker
}

#[derive(Clone,PartialEq)]
pub enum MerlinVersion {
    Merlin8,
    Merlin16,
    Merlin16Plus,
    Merlin32
}

#[derive(Clone,PartialEq)]
pub enum ProcessorType {
    _6502,
    _65c02,
    _65802,
    _65c816
}

#[derive(Clone,PartialEq)]
pub enum LabelType {
    Local,
    Global,
    Macro,
    MacroLocal,
    Variable
}

#[derive(Clone)]
pub struct AddressMode {
    pub mnemonic: String,
    pub code: i64,
    pub cycles: i64,
    pub processors: Vec<ProcessorType>,
    pub m_sensitive: bool,
    pub x_sensitive: bool
}

/// Info on processor instructions for handbook
#[derive(Clone)]
pub struct Operation {
    pub alt: Vec<String>,
    pub brief: String,
    pub desc: String,
    pub modes: Vec<AddressMode>,
    pub processors: Vec<ProcessorType>,
    pub status: String,
    pub m_sensitive: bool,
    pub x_sensitive: bool,
    pub abs_suffixable: bool,
    pub absl_suffixable: bool,
    pub absl_prefixable: bool
}

/// Info on a specific instruction and mode, useful for disassembly.
#[derive(Clone)]
pub struct MachineOperation {
    pub mnemonic: String,
    /// this is a snippet, such as `(1),y`, where the digit is bytes of data, and can be replaced by a value
    pub operand_snippet: String,
    pub processors: Vec<ProcessorType>,
    pub relative: bool,
    pub m_sensitive: bool,
    pub x_sensitive: bool,
    pub abs_suffixable: bool,
    pub absl_suffixable: bool,
    pub absl_prefixable: bool
}

#[derive(Clone)]
pub struct PseudoOperation {
    pub alt: Vec<String>,
    pub brief: String,
    pub category: String,
    pub caveat: Option<String>,
    pub desc: String,
    pub eg: Vec<String>,
    pub choices: Vec<String>,
    /// if regex matches the argument is not supported in Merlin 8
    pub v8x: Option<regex::Regex>,
    /// if the regex matches the argument is not supported in Merlin 16
    pub v16x: Option<regex::Regex>,
    pub version: Vec<MerlinVersion>
}

#[derive(Clone)]
pub struct Symbol {
    name: String,
    flags: u64,
    decs: Vec<lsp::Location>,
    defs: Vec<lsp::Location>,
    refs: Vec<lsp::Location>,
    /// This should record only *bad* forward references.
    /// It is a map from a reference's location to label types that were not defined up to that point.
    /// Multiple label types can occur due to ambiguities during the first pass (e.g. global vs. macro local).
    fwd_refs: HashMap<lsp::Location,Vec<LabelType>>,
    /// Current value of a symbol.
    value: Option<i64>,
    /// Merlin children are as follows:
    /// * global labels can have local labels as children
    /// * macros can have "global labels" as children (macro locals)
    children: HashMap<String,Symbol>,
    /// heading that precedes a symbol definition is its docstring
    docstring: String,
    /// line(s) of code defining this symbol
    defining_code: Option<String>
}

#[derive(Clone)]
pub struct Symbols {
    assembler: MerlinVersion,
    processor: ProcessorType,
    master_doc_uri: String,
    display_doc_uri: String,
    display_doc_type: SourceType,
    globals: HashMap<String,Symbol>,
    vars: HashMap<String,Symbol>,
    macros: HashMap<String,Symbol>,
    /// lines in the display document that need the parser hint
    alt_parser_lines: HashSet<isize>
}

#[derive(Clone)]
pub struct Workspace {
    pub ws_folders: Vec<lsp::Url>,
	/// array of documents in this workspace
    pub docs: Vec<super::Document>,
	/// map from an include file uri to all master uri that `put` it
	pub put_map: HashMap<String, HashSet<String>>,
	/// map from an include file uri to all master uri that `use` it
	pub use_map: HashMap<String, HashSet<String>>,
	/// set of uri that are included by another file
	pub includes: HashSet<String>,
	/// all entry symbols in the workspace
    pub entries: HashMap<String,Symbol>,
    /// fraction of linker operations in a document
    pub linker_frac: HashMap<String,f64>,
    /// did REL appear in the file
    pub rel_modules: HashSet<String>
}

/// This wraps the Tree-sitter parser, making use of the symbol
/// information to distinguish implicit macro calls from operations
/// or pseudo-operations with trailing characters.  This is needed
/// to mimic the behavior of Merlin 8/16/16+.
/// Merlin 32 syntax is further checked in diagnostic passes.
pub struct MerlinParser {
    parser: tree_sitter::Parser,
    op_book: handbook::operations::OperationHandbook,
    psop_book: handbook::pseudo_ops::PseudoOperationHandbook,
    col: isize,
    adj_line: String,
    c2_regex: regex::Regex
}

impl fmt::Display for SourceType {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Master => write!(f,"master"),
            Self::Module => write!(f,"module"),
            Self::UseAndPut => write!(f,"USE/PUT include"),
            Self::Use => write!(f,"USE include"),
            Self::Put => write!(f,"PUT include"),
            Self::Linker => write!(f,"linker"),
            _ => write!(f,"unknown")
        }
    }
}

impl fmt::Display for MerlinVersion {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Merlin8 => write!(f,"Merlin 8"),
            Self::Merlin16 => write!(f,"Merlin 16"),
            Self::Merlin16Plus => write!(f,"Merlin 16+"),
            Self::Merlin32 => write!(f,"Merlin 32")
        }
    }
}

impl fmt::Display for ProcessorType {
    fn fmt(&self,f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::_6502 => write!(f,"6502"),
            Self::_65c02 => write!(f,"65C02"),
            Self::_65802 => write!(f,"65802"),
            Self::_65c816 => write!(f,"65816")
        }
    }
}

impl Symbol {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            flags: 0,
            decs: Vec::new(),
            defs: Vec::new(),
            refs: Vec::new(),
            fwd_refs: HashMap::new(),
            value: None,
            children: HashMap::new(),
            docstring: String::new(),
            defining_code: None
        }
    }
    /// create new symbol and add a node in one step
    pub fn create(loc: lsp::Location, node: &tree_sitter::Node, source: &str) -> Self {
        let mut ans = Self::new(&node_text(node, source));
        ans.add_node(loc, node, source);
        ans
    }
    /// Add a node to the symbol.
    /// The node can be `label_def`, `label_ref`, `macro_def`, `macro_ref`, or `var_mac`.
    /// The latter can occur without a wrapper in some pseudo-ops such as `ASC`.
    pub fn add_node(&mut self, loc: lsp::Location, node: &tree_sitter::Node, source: &str) {
        if node.kind() == "var_mac" {
            self.refs.push(loc);
            self.flags |= symbol_flags::ARG | symbol_flags::VAR;
            return;
        }
        if let Some(parent) = node.parent() {
            if let Some(grandparent) = parent.parent() {
                if grandparent.kind() == "arg_jsr" {
                    self.flags |= symbol_flags::SUB;
                }
            }
            if parent.kind() == "arg_ent" {
                self.flags |= symbol_flags::ENT;
            }
            if parent.kind() == "arg_ext" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
            }
            if parent.kind() == "arg_exd" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
            }
        }
        if let Some(next) = node.next_named_sibling() {
            if next.kind() == "psop_ent" {
                self.flags |= symbol_flags::ENT;
            }
            if next.kind() == "psop_ext" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
            }
            if next.kind() == "psop_exd" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
            }
        }
        match node.kind()  {
            "label_def" => self.defs.push(loc),
            "macro_def" => { self.defs.push(loc); self.flags |= symbol_flags::MAC },
            "label_ref" => self.refs.push(loc),
            "macro_ref" => { self.refs.push(loc); self.flags |= symbol_flags::MAC},
            _ => {}
        };
        if let Some(child) = node.named_child(0) {
            if child.kind() == "local_label" {
                self.flags |= symbol_flags::LOC;
            } else if child.kind() == "var_label" {
                self.flags |= symbol_flags::VAR;
            }
        }
    }
}

impl Symbols {
    pub fn new() -> Self {
        Self {
            assembler: MerlinVersion::Merlin8,
            processor: ProcessorType::_6502,
            master_doc_uri: String::new(),
            display_doc_uri: String::new(),
            display_doc_type: SourceType::Master,
            globals: HashMap::new(),
            vars: HashMap::new(),
            macros: HashMap::new(),
            alt_parser_lines: HashSet::new()
        }
    }
    /// return strings to be displayed in the client's toolbar,
    /// currently [master document , display document type]
    pub fn toolbar_info(&self) -> Vec<String> {
        let mut ans = Vec::new();
        let mut master = "unknown".to_string();
        // parse the display doc URI first so it is used if master does not parse
        if let Ok(url) = lsp::Url::parse(&self.display_doc_uri) {
            if let Ok(path) = url.to_file_path() {
                if let Some(os) = path.file_name() {
                    if let Some(s) = os.to_str() {
                        master = s.to_string();
                    }
                }
            }
        };
        if let Ok(url) = lsp::Url::parse(&self.master_doc_uri) {
            if let Ok(path) = url.to_file_path() {
                if let Some(os) = path.file_name() {
                    if let Some(s) = os.to_str() {
                        master = s.to_string();
                    }
                }
            }
        };
        ans.push(master);
        ans.push(self.display_doc_type.to_string());
        ans
    }
    pub fn mac_defined(&self,txt: &str) -> bool {
        if let Some(sym) = self.macros.get(txt) {
            sym.defs.len() > 0
        } else {
            false
        }
    }
    pub fn mac_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        if let Some(sym) = self.macros.get(txt) {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Macro),
                None => false
            }
        } else {
            false
        }
    }
    pub fn global_defined(&self,txt: &str) -> bool {
        if let Some(sym) = self.globals.get(txt) {
            sym.defs.len() > 0
        } else {
            false
        }
    }
    pub fn global_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        if let Some(sym) = self.globals.get(txt) {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Global),
                None => false
            }
        } else {
            false
        }
    }
    pub fn var_defined(&self,txt: &str) -> bool {
        if let Some(sym) = self.vars.get(txt) {
            sym.defs.len() > 0
        } else {
            false
        }
    }
    pub fn var_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        if let Some(sym) = self.vars.get(txt) {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Variable),
                None => false
            }
        } else {
            false
        }
    }
    pub fn child_defined(&self,txt: &str,scope: &Symbol) -> bool {
        if let Some(sym) = scope.children.get(txt) {
            sym.defs.len() > 0
        } else {
            false
        }
    }
    pub fn child_forward(&self,txt: &str,scope: &Symbol, loc: &lsp::Location) -> bool {
        if let Some(sym) = scope.children.get(txt) {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Local) || fwd.contains(&LabelType::MacroLocal),
                None => false
            }
        } else {
            false
        }
    }
    /// should only be used if symbols have been updated
    pub fn adjust_line(&self,row: isize,line: &str,term: &str) -> String {
        let prefix = match self.alt_parser_lines.contains(&row) {
            true => CALL_TOK.to_string(),
            false => "".to_string()
        };
        [&prefix,line,term].concat()
    }
    /// use this during analysis to save the rows that need the parser hint
    pub fn update_row_data(&mut self,doc: &super::Document,row: isize,col: isize) {
        if col<0 && doc.uri.to_string()==self.display_doc_uri {
            self.alt_parser_lines.insert(row);
        }
    }
}

impl MerlinParser {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_merlin6502::language()).expect(RCH);
        Self {
            parser,
            op_book: handbook::operations::OperationHandbook::new(),
            psop_book: handbook::pseudo_ops::PseudoOperationHandbook::new(),
            col: 0,
            adj_line: String::new(),
            c2_regex: regex::Regex::new(r"\s+\S+").expect(RCH)
        }
    }
    /// This adjusts the parsing precedence rules to emulate Merlin's behavior.
    /// The underlying parser requires a hint to do this because it depends on the document
    /// symbols, the Merlin version, and the processor target.
    /// This issue centers around the fact that Merlin 8/16/16+ will accept a "weak match"
    /// to an operation or psuedo-operation mnemonic.
    fn adjust_line(&mut self, old_line: &str, symbols: &Symbols) -> String {
        self.col = 0;
        if old_line.starts_with(CALL_TOK) {
            self.col = -(CALL_TOK.len_utf8() as isize); // LSP position encoding assumption
            return old_line.to_string();
        }
        if old_line.starts_with("*") || old_line.starts_with(";") {
            return old_line.to_string();
        }
        let prefix = match self.c2_regex.find(old_line) {
            Some(sep_c2) => {
                let c2 = sep_c2.as_str().trim();
                if c2.starts_with(";") || 
                    self.op_book.strong_match(c2,&symbols.processor) ||
                    self.psop_book.strong_match(c2,&symbols.assembler) {
                    "".to_string() // always take strong match as is
                } else if symbols.mac_defined(c2) {
                    CALL_TOK.to_string() // no strong match, so if macro is defined take it
                } else if self.op_book.weak_match(&c2.to_lowercase(),&symbols.processor) ||
                    self.psop_book.weak_match(&c2.to_lowercase(),&symbols.assembler) {
                    "".to_string() // weak match, take it
                } else {
                    // At this point the parser could still weak match a disabled mnemonic.
                    // Our decision is to interpret it as a macro call.
                    CALL_TOK.to_string()
                }
            }
            None => "".to_string()
        };
        self.col = -(prefix.len() as isize); // LSP position encoding assumption
        prefix + old_line
    }
    /// column offset to apply to squiggles or highlights, should be 0 or -2 for default position encoding
    pub fn col_offset(&self) -> isize {
        self.col
    }
    /// borrow the adjusted line, may have a parsing hint prefix, ends with LF
    pub fn line(&self) -> &str {
        &self.adj_line
    }
    /// adjust the line with possible parsing hints, then parse.
    pub fn parse(&mut self, line: &str, symbols: &Symbols) -> Result<tree_sitter::Tree,crate::DYNERR> {
        self.adj_line = self.adjust_line(line,symbols);
        if !self.adj_line.ends_with("\n") {
            self.adj_line += "\n";
        }
        if let Some(tree) = self.parser.parse(&self.adj_line,None) {
            return Ok(tree);
        }
        Err(Box::new(super::Error::Syntax))
    }
}