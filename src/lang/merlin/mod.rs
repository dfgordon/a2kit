//! # Merlin analysis and transformations
//! 
//! This module is used by both the CLI and the language server.
//! The Merlin parser is provided by `tree_sitter_merlin6502`.  Every file will be parsed as Merlin 16+,
//! other Merlin versions are handled via diagnostic filters.
//! The server compiles to a separate executable, its entry point is in `src/bin/server-merlin/main.rs`.
//! 
//! The analyzer performs functions that begin to resemble assembly, such as resolving
//! file relationships and identifying symbols.  There is a spot assembler that is used to aid in
//! disassembly. As of this writing, however, full assembly is not supported.
//! 
//! ## Conditional Macro Definitions
//! 
//! The Merlin 8/16 manual explicitly recommends wrapping `MAC` in a `DO 0` fold.
//! Testing shows this is unnecessary and can lead to unexpected behaviors.  For example, something
//! other than `MAC` within this fold can end up being assembled by legacy Merlin.
//! This language server does not conform to legacy Merlin in this regard, except to produce a
//! warning if `MAC` appears inside a conditional.
//! 
//! ## Macro Locals
//! 
//! Labels that are defined within a macro definition require some discussion.
//! They are scoped to a macro and all of its dependencies, e.g., a label defined
//! in a nested macro is visible to the enclosing macro, and vice-versa.
//! Duplicate labels are legal in this scope, but this does *not* make them variables.
//! In particular, the first (last) assigned value is used everywhere in Merlin 32 (legacy Merlin).
//! Merlin 8 errors out if an equivalence is used after a reference, but Merlin 16+ does not.
//! 
//! ## Testing
//! 
//! We test against Merlin 8/16/16+/32.
//! Test versions are M8(2.58), M16(3.41), M16+(4.08), M32(1.1).
//! CI test coverage is 100% in terms of instructions and addressing modes available on
//! all processor variants (barring human error in constructing the tests).
//! Table of some under-documented distinctions follows.
//! 
//! Operation | Merlin 8 | Merlin 16 | Merlin 16+ | Merlin 32
//! ----------|----------|-----------|------------|-----------
//! DCI | invert end of dstring | M8 | M32 | invert end of whole argument
//! INV | wrong lower case | M8 | correct lower case | refuses lower case (1)
//! FLS | wrong lower case | M8 | M8 | refuses lower case (2)
//! FLS | flashes specials | M8 | M8 | does not flash specials
//! REV | hex ignored | M8 | M32 | hex incorporated into string (3)
//! STR | hex not counted | M8 | M32 | hex is counted
//! STRL | n/a | n/a | M32 | hex is counted
//! DS | bit shift not allowed | M8 | M8 | bit shift works
//! JML ($8000) | n/a | `DC 00 80` | `DC 00 80` | `6C 00 80`
//! MAC | last duplicate label takes precedence | M8 | M8 | first takes precedence
//! MAC | unreachable label accepted | M8 | M8 | unreachable label errors out (4)
//! MX | n/a | no shadowing | no shadowing | equivalence can shadow
//! LDA #'A' | `A9 41` | `A9 41` | `A9 41` | refuses until v1.1
//! DO 0 | MAC cancels | M8 | M8 | MAC does not cancel (5)
//!
//! 1. lower case is invertible in the alternate character set
//! 2. refusal is correct, flashing lower case is not possible
//! 3. a2kit will reject trailing hex as a syntax error, but the REV processor would reverse each dstring separately if the parser allowed it
//! 4. this can happen when expanding a nested macro definition
//! 5. "cancels" means code following the macro definition is assembled, even if `DO 0` is not closed

use lsp_types as lsp;
use std::collections::{HashSet,HashMap};
use std::fmt;
use std::str::FromStr;

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
    /// Current value of a symbol. When post-analyzing variables, the value at a given location
    /// can be reconstructed using `value_history`.
    value: Option<i64>,
    /// It is useful to be able to stash values for later restoration during analysis.
    value_stack: Vec<Option<i64>>,
    /// Merlin children are as follows:
    /// * global labels can have local labels as children
    /// * macros can have "global labels" as children (macro locals)
    children: HashMap<String,Symbol>,
    /// heading that precedes a symbol definition is its docstring
    docstring: String,
    /// line(s) of code defining this symbol
    defining_code: Option<String>,
    /// macro call or nested macro within another macro
    dependencies: HashSet<String>,
    /// ordered record of values assumed by the symbol, for a variable there can be many.
    /// when closing an include an entry should be added at the including location.
    /// At present LUP updates are not handled (variables are unset upon exit).
    /// The @ substitution feature is also not handled.
    checkpoints: Vec<(lsp::Location,Option<i64>)>
}

/// Extended symbol table applicable to a single module.
/// The symbols are gathered into globals, variables, and macros.
/// Locals and macro-locals appear as children of the former.
/// The symbols themselves contain global information such as all
/// the places where a symbol is referenced, defined, assigned a value, etc.
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
    mx: Symbol,
    /// lines in the display document that need the parser hint
    alt_parser_lines: HashSet<isize>
}

#[derive(Clone)]
pub struct Workspace {
    pub ws_folders: Vec<lsp::Uri>,
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
            value_stack: Vec::new(),
            children: HashMap::new(),
            docstring: String::new(),
            defining_code: None,
            dependencies: HashSet::new(),
            checkpoints: Vec::new()
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
    /// This will not create any overlaps between `refs`, `defs`, and `decs`.
    pub fn add_node(&mut self, loc: lsp::Location, node: &tree_sitter::Node, _source: &str) {
        if node.kind() == "var_mac" {
            self.refs.push(loc);
            self.flags |= symbol_flags::ARG | symbol_flags::VAR;
            return;
        }
        let mut is_dec = false;
        if let Some(parent) = node.parent() {
            if let Some(grandparent) = parent.parent() {
                if grandparent.kind() == "arg_jsr" {
                    self.flags |= symbol_flags::SUB;
                }
            }
            if parent.kind() == "arg_ent" {
                self.flags |= symbol_flags::ENT;
                self.decs.push(loc.clone());
                is_dec = true;
            }
            if parent.kind() == "arg_ext" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
                is_dec = true;
            }
            if parent.kind() == "arg_exd" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
                is_dec = true;
            }
        }
        if let Some(next) = node.next_named_sibling() {
            if next.kind() == "psop_ent" {
                self.flags |= symbol_flags::ENT;
                // in this form we really do have a definition
            }
            if next.kind() == "psop_ext" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
                is_dec = true;
            }
            if next.kind() == "psop_exd" {
                self.flags |= symbol_flags::EXT;
                self.decs.push(loc.clone());
                is_dec = true;
            }
        }
        match node.kind()  {
            "label_def" if !is_dec => self.defs.push(loc),
            "macro_def" => { self.defs.push(loc); self.flags |= symbol_flags::MAC },
            "label_ref" if !is_dec => self.refs.push(loc),
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
    fn add_dependency(&mut self,label: &str) {
        self.dependencies.insert(label.to_string());
    }
    fn dependencies(&self) -> &HashSet<String> {
        &self.dependencies
    }
    /// Set symbol to its value just prior to the given line.
    /// This can be used to rollback a variable for local analysis.
    fn localize_value(&mut self,loc: &lsp::Location) {
        let mut latest_val: Option<i64> = None;
        for (prev_loc,val) in &self.checkpoints {
            if prev_loc.uri == loc.uri {
                if prev_loc.range.start.line >= loc.range.start.line {
                    break;
                } 
                latest_val = *val;
            }
        }
        self.value = latest_val;
    }
    /// set value of child nodes to None, does not affect duplicates that may be
    /// defined in dependencies
    fn unset_children(&mut self) {
        for child in self.children.values_mut() {
            child.value = None;
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
            mx: Symbol::new("MX"),
            alt_parser_lines: HashSet::new()
        }
    }
    /// return strings to be displayed in the client's toolbar,
    /// currently [master document , display document type]
    pub fn toolbar_info(&self) -> Vec<String> {
        let mut ans = Vec::new();
        let mut master = "unknown".to_string();
        // parse the display doc URI first so it is used if master does not parse
        if let Ok(uri) = lsp::Uri::from_str(&self.display_doc_uri) {
            if let Ok(path) = super::pathbuf_from_uri(&uri) {
                if let Some(os) = path.file_name() {
                    if let Some(s) = os.to_str() {
                        master = s.to_string();
                    }
                }
            }
        };
        if let Ok(uri) = lsp::Uri::from_str(&self.master_doc_uri) {
            if let Ok(path) = super::pathbuf_from_uri(&uri) {
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
        match self.macros.get(txt) { Some(sym) => {
            sym.defs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn mac_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        match self.macros.get(txt) { Some(sym) => {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Macro),
                None => false
            }
        } _ => {
            false
        }}
    }
    pub fn global_declared_or_defined(&self,txt: &str) -> bool {
        match self.globals.get(txt) { Some(sym) => {
            sym.decs.len() + sym.defs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn global_declared(&self,txt: &str) -> bool {
        match self.globals.get(txt) { Some(sym) => {
            sym.decs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn global_defined(&self,txt: &str) -> bool {
        match self.globals.get(txt) { Some(sym) => {
            sym.defs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn global_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        match self.globals.get(txt) { Some(sym) => {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Global),
                None => false
            }
        } _ => {
            false
        }}
    }
    pub fn var_defined(&self,txt: &str) -> bool {
        match self.vars.get(txt) { Some(sym) => {
            sym.defs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn var_forward(&self,txt: &str,loc: &lsp::Location) -> bool {
        match self.vars.get(txt) { Some(sym) => {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Variable),
                None => false
            }
        } _ => {
            false
        }}
    }
    pub fn child_defined(&self,txt: &str,scope: &Symbol) -> bool {
        if scope.flags & symbol_flags::MAC > 0 {
            if let Ok(count) = self.count_macro_loc_definitions(scope, txt, 0, 15) {
                return count > 0;
            }
        }
        match scope.children.get(txt) { Some(sym) => {
            sym.defs.len() > 0
        } _ => {
            false
        }}
    }
    pub fn child_forward(&self,txt: &str,scope: &Symbol, loc: &lsp::Location) -> bool {
        match scope.children.get(txt) { Some(sym) => {
            match sym.fwd_refs.get(loc) {
                Some(fwd) => fwd.contains(&LabelType::Local) || fwd.contains(&LabelType::MacroLocal),
                None => false
            }
        } _ => {
            false
        }}
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
    /// recursively check to see if this label is a dependency of the symbol
    fn is_dependency(&self,label: &str,sym: &Symbol,curr_depth: usize,max_depth: usize) -> Result<bool,crate::DYNERR> {
        if curr_depth > max_depth {
            return Err(Box::new(assembly::Error::Nesting));
        }
        for m2 in sym.dependencies() {
            if m2 == label {
                log::debug!("indirect reference to {}",label);
                log::debug!("    from {}",&sym.name);
                return Ok(true);
            }
            if let Some(dep) = self.macros.get(m2) {
                if self.is_dependency(label, dep,curr_depth+1,max_depth)? {
                    log::debug!("    from {}",&sym.name);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    /// test whether any label is referenced or an entry
    fn is_label_referenced_or_ent(&self,label: &str,scope: Option<&Symbol>) -> bool {
        let maybe_sym =  match label.get(0..1) {
            Some(":") => match scope {
                Some(parent) => parent.children.get(label),
                None => None
            },
            Some("]") => self.vars.get(label),
            Some(_) => self.globals.get(label),
            None => None
        };
        match maybe_sym {
            Some(sym) => sym.refs.len() > 0 || sym.flags & symbol_flags::ENT > 0,
            None => false
        }
    }
    /// test whether a macro is ever referenced, even indirectly
    fn is_macro_referenced(&self,label: &str, max_depth: usize) -> Result<bool,crate::DYNERR> {
        // first see if it is directly referenced, if yes we are done
        if let Some(sym) = self.macros.get(label) {
            if sym.refs.len() > 0 {
                return Ok(true);
            }
        }
        for sym in self.macros.values() {
            if sym.refs.len() > 0 {
                if self.is_dependency(label, sym,0,max_depth)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    /// how many times is the label defined in this macro and its dependencies
    fn count_macro_loc_definitions(&self,mac: &Symbol, label: &str, curr_depth: usize, max_depth: usize) -> Result<usize,crate::DYNERR> {
        let mut count: usize = 0;
        if curr_depth > max_depth {
            return Err(Box::new(assembly::Error::Nesting));
        }
        if let Some(child) = mac.children.get(label) {
            count += child.defs.len();
        }
        for m2 in mac.dependencies() {
            if let Some(sym) = self.macros.get(m2) {
                count += self.count_macro_loc_definitions(sym,label,curr_depth+1,max_depth)?;
            }
        }
        Ok(count)
    }
    /// this can be called as a macro definition is closed to get a list of duplicates
    fn detect_all_duplicates_in_macro(&self,mac: &Symbol) -> Result<Option<String>,crate::DYNERR> {
        let mut ans = String::new();
        for label in mac.children.keys() {
            if self.count_macro_loc_definitions(mac, label, 0, 15)? > 1 {
                ans += &label;
                ans += ",";
            }
        }
        if ans.len() > 0 {
            ans.pop();
            Ok(Some(ans))
        } else {
            Ok(None)
        }
    }
    /// Set variables to value at the given location.
    /// The analyzer's first pass establishes the values.
    fn localize_all_variables(&mut self,loc: &lsp::Location) {
        for var in self.vars.values_mut() {
            var.localize_value(loc);
        }
        self.mx.localize_value(loc);
    }
    /// Clear the current value of all variables, the variables themselves remain.
    fn unset_all_variables(&mut self) {
        for var in self.vars.values_mut() {
            var.value = None;
        }
        self.mx.value = None;
    }
    /// Save the current value of all variables onto a stack.
    fn stash_all_variables(&mut self) {
        for var in self.vars.values_mut() {
            var.value_stack.push(var.value);
        }
        self.mx.value_stack.push(self.mx.value);
    }
    /// Restore the current values of all variables from a stack.
    fn restore_all_variables(&mut self) {
        for var in self.vars.values_mut() {
            if let Some(v) = var.value_stack.pop() {
                var.value = v;
            }
        }
        if let Some(v) = self.mx.value_stack.pop() {
            self.mx.value = v;
        }
    }
    /// Checkpoint variables at the given location.
    /// Locations must be visited in order, and only during the first pass.
    /// Typical use is immediately after exiting a source scope.
    fn checkpoint_all_variables(&mut self,loc: &lsp::Location) {
        for var in self.vars.values_mut() {
            var.checkpoints.push((loc.clone(),var.value));
        }
        self.mx.checkpoints.push((loc.clone(),self.mx.value));
    }
}

impl MerlinParser {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_merlin6502::LANGUAGE.into()).expect(RCH);
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