use std::collections::{HashSet,HashMap};
use std::ffi::OsString;
use std::path::PathBuf;
use crate::lang;
use lsp_types as lsp;
use tree_sitter::TreeCursor;
use super::super::{Symbols,Collisions,Workspace};
use crate::lang::{Document,Navigate,Navigation};
use crate::{DYNERR,STDRESULT};

const RCH: &str = "unreachable was reached";
const MAX_DIRS: usize = 1000;
const MAX_DEPTH: usize = 10;
const IGNORE_DIRS: [&str;4] = [
    "build",
    "node_modules",
    "target",
    ".git"
];

impl Workspace {
    pub fn new() -> Self {
        Self {
            ws_folders: Vec::new(),
            docs: Vec::new(),
            backlink_map: HashMap::new(),
            chain_destinations: HashSet::new(),
            ws_symbols: HashMap::new(),
            ws_collisions: HashMap::new()
        }
    }
    pub fn borrow_ws_symbols(&self) -> &HashMap<lsp::Uri,Symbols> {
        &self.ws_symbols
    }
    pub fn get_ws_symbols_for_client(&self) -> Vec<lsp::WorkspaceSymbol> {
        let mut ans = Vec::new();
        let mut instances: Vec<(lsp::Location,String)> = Vec::new();
        for (uri,syms) in &self.ws_symbols {
            for (name,var) in &syms.scalars {
                for rng in &var.defs {
                    instances.push((lsp::Location::new(uri.clone(),rng.clone()),name.to_owned()));
                }
            }
            for (name,var) in &syms.arrays {
                for rng in &var.decs {
                    instances.push((lsp::Location::new(uri.clone(),rng.clone()),name.to_owned()));
                }
                for rng in &var.defs {
                    instances.push((lsp::Location::new(uri.clone(),rng.clone()),name.to_owned()));
                }
            }
        }
        for (loc,name) in instances {
            ans.push(lsp::WorkspaceSymbol {
                name: name.to_owned(),
                kind: lsp::SymbolKind::VARIABLE,
                tags: None,
                container_name: None,
                location: lsp::OneOf::Left(loc),
                data: None
            });
        }
        ans
    }
    /// Get all URI that chain to this URI directly
	pub fn get_direct_backlinks(&self, uri: &lsp::Uri) -> HashSet<lsp::Uri> {
        let mut ans = HashSet::new();
        if let Some(links) = self.backlink_map.get(uri) {
            for link in links {
                ans.insert(link.to_owned());
            }
        }
		ans
	}
	/// find document's backlinks recursively and insert them into ans, the direct invocation should have exclude=uri and depth=0
	pub fn get_all_backlinks(&self, ans: &mut HashSet<lsp::Uri>, uri: &lsp::Uri, exclude: &lsp::Uri, depth: usize, max_depth: usize) {
        if depth > max_depth {
            return;
        }
        let direct = self.get_direct_backlinks(uri);
        for link in &direct {
            if exclude == link {
                continue;
            }
            if !ans.contains(link) {
                ans.insert(link.clone());
                self.get_all_backlinks(ans, link, exclude, depth+1, max_depth);
            }
        }
        if depth == 0 {
            log::info!("This document has {} direct and {} total backlinks",direct.len(),ans.len());
            for link in ans.iter() {
                log::debug!("  {}",link.as_str());
            }
        }
	}
}

pub struct WorkspaceScanner {
    parser: tree_sitter::Parser,
    line: String,
    curr_uri: Option<lsp::Uri>,
    curr_row: isize,
    curr_depth: usize,
    file_count: usize,
    dir_count: usize,
    ws: Workspace,
    working_symbols: Symbols,
    working_collisions: Collisions,
    running_docstring: String
}

impl WorkspaceScanner {
    pub fn new() -> Self {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_applesoft::LANGUAGE.into()).expect(RCH);
        Self {
            parser,
            line: String::new(),
            curr_uri: None,
            curr_row: 0,
            curr_depth: 0,
            file_count: 0,
            dir_count: 0,
            ws: Workspace::new(),
            working_symbols: Symbols::new(),
            working_collisions: Collisions::new(),
            running_docstring: String::new()
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
        log::debug!("gather from {}",base.display());
        self.dir_count += 1;
        self.curr_depth += 1;
        let opt = glob::MatchOptions {
            case_sensitive: false,
            require_literal_leading_dot: false,
            require_literal_separator: false
        };
        // first scan source files
        let patt1 = base.join("*.bas");
        let patt2 = base.join("*.abas");
        match (patt1.as_os_str().to_str(),patt2.as_os_str().to_str()) {
            (Some(g1),Some(g2)) => match (glob::glob_with(g1,opt),glob::glob_with(g2,opt)) {
                (Ok(p1),Ok(p2)) => {
                    let mut paths = Vec::new();
                    for p in p1 {
                        paths.push(p);
                    }
                    for p in p2 {
                        paths.push(p);
                    }
                    for entry in paths {
                        match &entry { Ok(path) => {
                            let full_path = base.join(path);
                            if let Some(path_str) = full_path.as_os_str().to_str() {
                                if let (Ok(uri),Ok(txt)) = (lang::uri_from_path_str(path_str),std::fs::read_to_string(path.clone())) {
                                    log::trace!("{}",uri.as_str());
                                    self.ws.docs.push(Document::new(uri, txt));
                                }
                            }
                        } Err(e) => log::warn!("glob error {}",e) }
                        self.file_count += 1;
                        if self.file_count >= max_files {
                            log::error!("aborting due to excessive source file count of {}",self.file_count);
                            return Err(Box::new(crate::lang::Error::OutOfRange));
                        }
                    }
                },
                _ => log::warn!("directory {} could not be globbed",base.display())
            },
            _ => log::warn!("directory {} has unknown encoding",base.display())
        }
        // now go into subdirectories
        if self.curr_depth < MAX_DEPTH {
            log::trace!("depth is {}",self.curr_depth);
            for entry in std::fs::read_dir(base)? {
                let entry = entry?;
                log::trace!("check {} for descent",entry.file_name().as_os_str().to_string_lossy());
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
    /// Buffer all documents matching `*.bas` or `*.abas` in any of `dirs`, up to maximum count `max_files`,
    /// searching at most MAX_DIRS directories, using at most MAX_DEPTH recursions, and ignoring IGNORE_DIRS.
	pub fn gather_docs(&mut self, dirs: &Vec<lsp::Uri>, max_files: usize) -> STDRESULT {
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
            let path = lang::pathbuf_from_uri(&dir)?;
            log::debug!("scanning {}",path.as_os_str().to_string_lossy());
            self.gather_from_dir(&path,max_files)?;
		}
        if self.dir_count >= MAX_DIRS {
            log::warn!("scan was aborted after {} directories",self.dir_count);
        }
        log::info!("there were {} sources in the workspace",self.file_count);
        Ok(())
	}
    /// Scan buffered documents for variable definitions or declarations.
    /// Create chain structure.
    /// Assumes buffers are up to date.
    pub fn scan(&mut self) -> STDRESULT {
        self.ws.ws_symbols = HashMap::new();
        self.ws.ws_collisions = HashMap::new();
        self.ws.backlink_map = HashMap::new();
        self.ws.chain_destinations = HashSet::new();
        for i in 0..self.ws.docs.len() {
            self.working_symbols = Symbols::new();
            self.working_collisions = Collisions::new();
            let doc = self.ws.docs[i].to_owned();
            log::debug!("Scanning {}...",doc.uri.as_str());
            self.curr_uri = Some(doc.uri.clone());
            self.curr_row = 0;
            self.running_docstring = String::new();
            for line in doc.text.lines() {
                self.line = line.to_string() + "\n";
                if let Some(tree) = self.parser.parse(&self.line,None) {
                    self.walk(&tree)?;
                }
                self.curr_row += 1;
            }
            self.ws.ws_symbols.insert(doc.uri.clone(),self.working_symbols.clone());
            self.ws.ws_collisions.insert(doc.uri.clone(),self.working_collisions.clone());
        }
        log::info!("Found {} chain destinations",self.ws.chain_destinations.len());
        if log::max_level() >= log::Level::Debug {
            for chain in &self.ws.chain_destinations {
                log::debug!("  dest {}",crate::lang::server::path_in_workspace(chain, &self.ws.ws_folders));
                if let Some(uri) = self.ws.backlink_map.get(chain) {
                    for backlink in uri {
                        log::debug!("    backlink {}",crate::lang::server::path_in_workspace(backlink, &self.ws.ws_folders));
                    }
                }
            }
        }
        // TODO: do we need any cleaning to remove circular chains?
        Ok(())
    }
}

impl Navigate for WorkspaceScanner {
    /// Visitor to build information about the overall workspace.
    /// Important that this be efficient since every file is scanned.
    fn visit(&mut self, curs: &TreeCursor) -> Result<Navigation,DYNERR> {
        let curr = curs.node();
        if let Some(nav) = super::pass1::visit_defs_and_decs(&mut self.working_symbols,Some(&mut self.working_collisions), curs, &self.line, self.curr_row, 0) {
            return Ok(nav);
        }
        // find the CHAIN patterns
        if curr.kind() == "tok_call" || curr.kind() == "tok_print" {
            if let Some((prog,_)) = super::chain::test_chain(&curr,&self.line) {
                let emu_path = [&prog,".bas"].concat();
                let set1 = lang::get_emulation_match(&self.ws.docs, &emu_path, "/");
                let emu_path = [&prog,".abas"].concat();
                let set2 = lang::get_emulation_match(&self.ws.docs, &emu_path, "/");
                let matching_docs = match (set1.len(),set2.len()) {
                    (1,0) => set1,
                    (0,1) => set2,
                    _ => {
                        log::debug!("CHAIN resulted in no unique match ({})",set1.len()+set2.len());
                        return Ok(Navigation::GotoSibling);
                    }
                };
                let mut masters = match self.ws.backlink_map.get(&matching_docs[0]) {
                    Some(m) => m.to_owned(),
                    None => HashSet::new()
                };
                if let Some(curr_uri) = &self.curr_uri {
                    masters.insert(curr_uri.clone());
                }
                self.ws.backlink_map.insert(matching_docs[0].clone(),masters);
                self.ws.chain_destinations.insert(matching_docs[0].clone());
            }
            return Ok(Navigation::GotoSibling);
        }
		// this determines how deep in the tree we need to go
		if curs.depth() < 4 {
			return Ok(Navigation::GotoChild);
        }
		
		return Ok(Navigation::GotoParentSibling);
    }
}