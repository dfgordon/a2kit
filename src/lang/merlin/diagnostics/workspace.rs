use std::collections::{HashSet,HashMap};
use std::ffi::OsString;
use std::path::PathBuf;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use super::super::{SourceType,Symbol,Workspace};
use crate::lang::{Document,Navigate,Navigation,node_text,lsp_range};
use crate::{DYNERR,STDRESULT};

const RCH: &str = "unreachable was reached";
const MAX_DIRS: usize = 1000;
const MAX_DEPTH: usize = 10;
const IGNORE_DIRS: [&str;3] = [
    "build",
    "node_modules",
    "target"
];

impl Workspace {
    pub fn new() -> Self {
        Self {
            ws_folders: Vec::new(),
            docs: Vec::new(),
            use_map: HashMap::new(),
            put_map: HashMap::new(),
            includes: HashSet::new(),
            entries: HashMap::new(),
            linker_frac: HashMap::new(),
            rel_modules: HashSet::new()
        }
    }
    pub fn get_ws_symbols(&self) -> Vec<lsp::WorkspaceSymbol> {
        let mut ans = Vec::new();
        for (name,sym) in &self.entries {
            let mut locs = Vec::new();
            for loc in &sym.defs {
                locs.push(loc.clone());
            }
            for loc in &sym.refs {
                locs.push(loc.clone());
            }
            for loc in locs {
                ans.push(lsp::WorkspaceSymbol {
                    name: name.to_owned(),
                    kind: lsp::SymbolKind::CONSTANT,
                    tags: None,
                    container_name: None,
                    location: lsp::OneOf::Left(loc),
                    data: None
                });
            }
        }
        ans
    }
    /// Get all masters of this URI
	pub fn get_masters(&self, uri: &lsp::Url) -> HashSet<String> {
        let mut ans = HashSet::new();
        if let Some(masters) = self.put_map.get(&uri.to_string()) {
            for master in masters {
                ans.insert(master.to_owned());
            }
        }
        if let Some(masters) = self.use_map.get(&uri.to_string()) {
            for master in masters {
                ans.insert(master.to_owned());
            }
        }
		ans
	}
	/// find document's master based on what is in workspace and preference,
	/// but ignoring availability of labels and diagnostic status.
	pub fn get_master(&self, include: &Document, preferred_master: Option<String>) -> Document {
        let masters = self.get_masters(&include.uri);
        if masters.len() == 0 {
            return include.clone();
        }
        let preferred = match &preferred_master {
            Some(uri) => {
                match masters.get(uri.as_str()) {
                    Some(s) => s,
                    None => masters.iter().next().unwrap()
                }
            },
            None => masters.iter().next().unwrap()
        };
        for doc in &self.docs {
            if doc.uri.as_str() == preferred {
                return doc.clone();
            }
        }
		return include.clone();
	}
	/// Get the document URL that is the best match to the ProDOS path at the *next* cursor location.
    /// This may return an empty vector, or a vector with more than one match, where the latter
    /// means there were multiple equally good matches.
	pub fn get_include_doc(&self, node: &tree_sitter::Node, source: &str) -> Vec<lsp::Url> {
        let mut ans = Vec::new();
        let mut best_quality = 0;
		if let Some(file_node) = node.next_named_sibling() {
            log::debug!("search for include `{}`",node_text(&file_node,source));
            for doc in &self.docs {
                let quality = super::match_prodos_path(&file_node, source, &doc);
                log::trace!("match {} to {} Q={}",node_text(&file_node, source),doc.uri.as_str(),quality);
                if quality > best_quality {
                    ans = vec![doc.uri.clone()];
                    best_quality = quality;
                } else if quality > 0 && quality == best_quality {
                    if doc.uri.as_str().len() < ans.last().unwrap().as_str().len() {
                        ans = vec![doc.uri.clone()];
                    } else {
                        ans.push(doc.uri.clone());
                    }
                }
            }
        }
        log::debug!("found {} include candidates",ans.len());
        for uri in &ans {
            log::trace!("  {}",uri.as_str());
        }
		ans
	}
    pub fn source_type(&self, uri: &lsp::Url, linker_threshold: f64) -> SourceType {
        let key = uri.to_string();
        if uri.scheme() == "macro" {
            return SourceType::MacroRef;
        }
        if let Some(frac) = self.linker_frac.get(&uri.to_string()) {
            if *frac >= linker_threshold {
                return SourceType::Linker;
            }
        }
        let is_put = self.put_map.contains_key(&key);
        let is_use = self.use_map.contains_key(&key);
        let is_rel = self.rel_modules.contains(&uri.to_string());
        match (is_put,is_use,is_rel) {
            (true,true,_) => SourceType::UseAndPut,
            (true,false,_) => SourceType::Put,
            (false,true,_) => SourceType::Use,
            (false,false,true) => SourceType::Module,
            (false,false,false) => SourceType::Master
        }
    }
}

pub struct WorkspaceScanner {
    parser: tree_sitter::Parser,
    line: String,
    curr_uri: Option<lsp::Url>,
    curr_row: isize,
    curr_depth: usize,
    file_count: usize,
    dir_count: usize,
    ws: Workspace,
    running_docstring: String,
    scan_patt: regex::Regex,
    link_patt: regex::Regex
}

impl WorkspaceScanner {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_merlin6502::LANGUAGE.into()).expect(RCH);
        Self {
            parser,
            line: String::new(),
            curr_uri: None,
            curr_row: 0,
            curr_depth: 0,
            file_count: 0,
            dir_count: 0,
            ws: Workspace::new(),
            running_docstring: String::new(),
            scan_patt: regex::Regex::new(r"^\S*\s+(ENT|PUT|USE|REL|ent|put|use|rel)(\s+|$)").expect(RCH),
            link_patt: regex::Regex::new(r"^\S*\s+(LNK|LKV|ASM|lnk|lkv|asm)\s+").expect(RCH)
        }
    }
    /// Workspace scanner has its own copies that are not always up to date.
    /// This should be called when there is a document change notification.
    /// When there is a full rescan, call this with all checkpoints
    /// right after the gather.
    pub fn update_doc(&mut self,doc: &Document) {
        for old in &mut self.ws.docs {
            if old.uri == doc.uri {
                old.text = doc.text.clone();
                old.version = doc.version;
            }
        }
    }
    /// Borrow the workspace data
    pub fn get_workspace(&self) -> &Workspace {
        &self.ws
    }
    /// Set the workspace data, source is probably another analysis object
    pub fn set_workspace(&mut self,ws: Workspace) {
        self.ws = ws;
    }
    /// Add volatile documents to the workspace set, useful for piped documents
    pub fn append_volatile_docs(&mut self, docs: Vec<Document>) {
        for doc in docs {
            self.ws.docs.push(doc);
        }
    }
    /// Recursively gather files from this directory
    fn gather_from_dir(&mut self, base: &PathBuf, max_files: usize) -> STDRESULT {
        self.dir_count += 1;
        self.curr_depth += 1;
        let opt = glob::MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: false
        };
        // first scan source files
        let patt = base.join("*.s");
        if let Some(globable) = patt.as_os_str().to_str() {
            if let Ok(paths) = glob::glob_with(globable,opt) {
                for entry in paths {
                    if let Ok(path) = &entry {
                        let full_path = base.join(path);
                        if let (Ok(uri),Ok(txt)) = (lsp::Url::from_file_path(full_path),std::fs::read_to_string(path.clone())) {
                            log::trace!("{}",uri.as_str());
                            self.ws.docs.push(Document::new(uri, txt));
                        }
                    }
                    self.file_count += 1;
                    if self.file_count >= max_files {
                        log::error!("aborting due to excessive source file count of {}",self.file_count);
                        return Err(Box::new(crate::lang::Error::OutOfRange));
                    }
                }
            }
        } else {
            log::warn!("directory {} could not be globbed",base.display());
        }
        // now go into subdirectories
        if self.curr_depth < MAX_DEPTH {
            for entry in std::fs::read_dir(base)? {
                let entry = entry?;
                let ignore_dirs = IGNORE_DIRS.iter().map(|x| OsString::from(x)).collect::<Vec<OsString>>();
                if ignore_dirs.contains(&entry.file_name()) {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() && self.dir_count < MAX_DIRS {
                    self.gather_from_dir(&path,max_files)?
                }
            }
        }
        self.curr_depth -= 1;
        Ok(())
    }
    /// Buffer all documents matching `*.s` in any of `dirs`, up to maximum count `max_files`,
    /// searching at most MAX_DIRS directories, using at most MAX_DEPTH recursions, and ignoring IGNORE_DIRS.
	pub fn gather_docs(&mut self, dirs: &Vec<lsp::Url>, max_files: usize) -> STDRESULT {
        self.ws.ws_folders = Vec::new();
        self.ws.docs = Vec::new();
        self.curr_depth = 0;
        self.file_count = 0;
        self.dir_count = 0;
        // copy the workspace url's to the underlying workspace object
        for dir in dirs {
            self.ws.ws_folders.push(dir.clone());
        }
        for dir in dirs {
            log::debug!("scanning {}",dir.as_str());
            let path = match dir.to_file_path() {
                Ok(b) => b,
                Err(_) => return Err(Box::new(crate::lang::Error::BadUrl))
            };
            self.gather_from_dir(&path,max_files)?;
		}
        if self.dir_count >= MAX_DIRS {
            log::warn!("scan was aborted after {} directories",self.dir_count);
        }
        log::info!("there were {} sources in the workspace",self.file_count);
        Ok(())
	}
    /// Scan buffered documents for entries and includes.
    /// Assumes buffers are up to date.
    pub fn scan(&mut self) -> STDRESULT {
        self.ws.entries = HashMap::new();
        self.ws.use_map = HashMap::new();
        self.ws.put_map = HashMap::new();
        self.ws.includes = HashSet::new();
        self.ws.linker_frac = HashMap::new();
        self.ws.rel_modules = HashSet::new();
        for i in 0..self.ws.docs.len() {
            let doc = self.ws.docs[i].to_owned();
            self.curr_uri = Some(doc.uri.clone());
            self.curr_row = 0;
            let mut linker_count = 0.0;
            self.running_docstring = String::new();
            for line in doc.text.lines() {
                // use regex to skip most lines, this avoids the need to
                // deal with implicit macro call resolution, and may speed things up
                if !self.scan_patt.is_match(line) && !line.starts_with("*") {
                    if self.link_patt.is_match(line) {
                        linker_count += 1.0;
                    }
                    self.curr_row += 1;
                    self.running_docstring = String::new();
                    continue;
                }
                self.line = line.to_string() + "\n";
                if let Some(tree) = self.parser.parse(&self.line,None) {
                    self.walk(&tree)?;
                }
                self.curr_row += 1;
            }
            self.ws.linker_frac.insert(doc.uri.to_string(),linker_count/self.curr_row as f64);
        }
        // clean the include maps so that a master cannot also be an include.
        // it is possible to end up with no masters.
        for include in &self.ws.includes {
            for masters in self.ws.use_map.values_mut() {
                masters.remove(include);
            }
            for masters in self.ws.put_map.values_mut() {
                masters.remove(include);
            }
        }
        Ok(())
    }
}

impl Navigate for WorkspaceScanner {
    /// Visitor to build information about the overall workspace.
    /// Important that this be efficient since every file is scanned.
    /// The caller should skip over all lines but those matching `scan_patt`.
    /// For this scan we do not need or want to descend into includes (no recursive includes).
    fn visit(&mut self, curs: &TreeCursor) -> Result<Navigation,DYNERR> {
        // as an optimization, take swift action on certain high level nodes
        if curs.node().kind() == "operation" || curs.node().kind() == "macro_call" {
            self.running_docstring = String::new();
            return Ok(Navigation::Exit);
        }
        if curs.node().kind() == "source_file" {
            return Ok(Navigation::GotoChild);
        }
        if curs.node().kind() == "pseudo_operation" {
            return Ok(Navigation::GotoChild);
        }
        let curr = curs.node();
        let next = curr.next_named_sibling();
        let curr_uri = self.curr_uri.as_ref().unwrap().clone();
        let loc = lsp::Location::new(curr_uri.clone(), lsp_range(curr.range(), self.curr_row, 0));

        // Gather docstring

        if curr.kind() == "heading" {
            let temp = match curr.named_child(0) {
                Some(n) => node_text(&n, &self.line),
                None => String::new()
            };
            self.running_docstring += &temp;
            self.running_docstring += "\n";
            return Ok(Navigation::Exit);
        } else if curs.depth()>1 && curr.kind() != "label_def" {
            self.running_docstring = String::new();
        }

        // Identify REL modules

        if curr.kind() == "psop_rel" {
            self.ws.rel_modules.insert(curr_uri.to_string());
            return Ok(Navigation::Exit);
        }

        // Handle entries.

        if curr.kind() == "label_def" && next.is_some() && next.unwrap().kind() == "psop_ent" {
            let name = node_text(&curr,&self.line);
            if !self.ws.entries.contains_key(&name) {
                self.ws.entries.insert(name.clone(),Symbol::new(&name));
            }
            let sym = self.ws.entries.get_mut(&name).unwrap();
            sym.add_node(loc, &curr, &self.line);
            sym.defining_code = Some(self.line.clone());
            sym.docstring = self.running_docstring.clone();
            self.running_docstring = String::new();
            return Ok(Navigation::Exit);
        }
        if curr.kind() == "psop_ent" {
            let mut sib = match next {
                Some(n) => n.named_child(0),
                None => None
            };
            while sib.is_some() && sib.unwrap().kind() == "label_ref" {
                let name = node_text(&sib.unwrap(),&self.line);
                if !self.ws.entries.contains_key(&name) {
                    self.ws.entries.insert(name.clone(),Symbol::new(&name));
                }
                let sym = self.ws.entries.get_mut(&name).unwrap();
                sym.add_node(loc.clone(), &sib.unwrap(), &self.line);
                sib = sib.unwrap().next_named_sibling();
            }
            return Ok(Navigation::Exit);
        }

        // Now check for includes.

        if curr.kind() == "label_def" {
            self.running_docstring = String::new();
            return Ok(Navigation::GotoSibling);
        }
        if curr.kind() == "psop_use" {
            let matching_docs = self.ws.get_include_doc(&curr,&self.line);
            if matching_docs.len()==1 {
                let include_uri = matching_docs[0].to_string();
                let mut masters = match self.ws.use_map.get(&include_uri) {
                    Some(m) => m.to_owned(),
                    None => HashSet::new()
                };
                masters.insert(curr_uri.to_string());
                self.ws.use_map.insert(include_uri.clone(),masters);
                self.ws.includes.insert(include_uri);
            } else {
                log::debug!("USE resulted in no unique match ({})",matching_docs.len());
            }
        }
        if curr.kind() == "psop_put" {
            let matching_docs = self.ws.get_include_doc(&curr,&self.line);
            if matching_docs.len()==1 {
                let include_uri = matching_docs[0].to_string();
                let mut masters = match self.ws.put_map.get(&include_uri) {
                    Some(m) => m.to_owned(),
                    None => HashSet::new()
                };
                masters.insert(curr_uri.to_string());
                self.ws.put_map.insert(include_uri.clone(),masters);
                self.ws.includes.insert(include_uri);
            } else {
                log::debug!("PUT resulted in no unique match ({})",matching_docs.len());
            }
        }
        // If none of the above we can go straight to the next line
        return Ok(Navigation::Exit);
    }
}