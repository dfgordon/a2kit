//! # Generics and traits for language servers
//! 
//! These traits can be used to aid in the handling of requests
//! that are typically sent by a language client.  The `Analysis`
//! trait is also used by the CLI `verify` subcommand.

use std::io::Write;
use std::str::FromStr;
use lsp_types as lsp;
use lsp::request::Request;
use tree_sitter;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{STDRESULT,DYNERR};

pub const TOKEN_TYPES: [&str;21] = ["comment", "string", "keyword", "number", "regexp", "operator", "namespace",
"type", "struct", "class", "interface", "enum", "typeParameter", "function",
"method", "decorator", "macro", "variable", "parameter", "property", "label"];

// JSON-RPC error codes; are they defined somewhere else?
// -32768 through -32000 are reserved
mod rpc_error {
    pub const PARSE_ERROR: i32 = -32700;
    // pub const INVALID_REQUEST: i32 = -32600;
    // pub const METHOD_NOT_FOUND: i32 = -32601;
    // pub const INVALID_PARAMS: i32 = -32602;
    // pub const INTERNAL_ERROR: i32 = -32603;
}

/// Build an object around this trait to generate hovers.  Then when the client requests
/// hovers, feed that object into Checkpoint::hover_response.
pub trait Hovers {
    fn get(&mut self, line: String, row: isize, col: isize) -> Option<lsp::Hover>;
}

/// Build an object around this trait to generate completions.  Then when the client requests
/// completions, feed that object into Checkpoint::completion_response.
pub trait Completions {
	fn get(&mut self,lines: &mut std::str::Lines, ctx: &lsp::CompletionContext, pos: &lsp::Position) -> Result<Vec<lsp::CompletionItem>,String>;
}

/// Build an object around this trait to generate semantic tokens.  Then when the client requests
/// tokens, feed that object into Checkpoint::sem_tok_response.
pub trait Tokens {
	fn get(&mut self, txt: &str) -> Result<lsp::SemanticTokens,DYNERR>;
}

/// This important trait is used to provide data from a prior analysis to the LSP client.
/// The implementation defines all mechanisms for updating document and symbol information.
/// A typical pattern is to store the implementation in a map keyed by the document's URI string.
/// The default `*_response` functions provide a convenient way to respond to a client's
/// requests.  These functions are intended to mutate a default response within a match.
pub trait Checkpoint {
    /// Get a copy of the most recently checkpointed document and version.
    fn get_doc(&self) -> super::Document;
    /// Get a row from the most recently checkpointed document.
    fn get_line(&self,row: usize) -> Option<String>;
    fn get_symbols(&self) -> Vec<lsp::DocumentSymbol>;
    fn get_decs(&self,loc: &lsp::Location) -> Vec<lsp::Location>;
    fn get_defs(&self,loc: &lsp::Location) -> Vec<lsp::Location>;
    fn get_refs(&self,loc: &lsp::Location) -> Vec<lsp::Location>;
    fn get_renamables(&self,loc: &lsp::Location) -> Vec<lsp::Location>;
    fn get_folding_ranges(&self) -> Vec<lsp::FoldingRange>;
    fn symbol_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::DocumentSymbolParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document.uri);
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                *resp = match serde_json::to_value::<Vec<lsp::DocumentSymbol>>(chkpt.get_symbols()) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,Some(result)),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"symbol request failed while parsing".to_string())
                };
            }
        }
    }
    fn goto_dec_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::GotoDefinitionParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position_params.text_document.uri);
            let pos = params.text_document_position_params.position;
            let loc = lsp::Location::new(uri.clone(),lsp::Range::new(pos,pos));
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                *resp = match serde_json::to_value::<Vec<lsp::Location>>(chkpt.get_decs(&loc)) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,Some(result)),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"goto decs failed while parsing".to_string())
                };
            }
        }
    }
    fn goto_def_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::GotoDefinitionParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position_params.text_document.uri);
            let pos = params.text_document_position_params.position;
            let loc = lsp::Location::new(uri.clone(),lsp::Range::new(pos,pos));
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                *resp = match serde_json::to_value::<Vec<lsp::Location>>(chkpt.get_defs(&loc)) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,Some(result)),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"goto defs failed while parsing".to_string())
                };
            }
        }
    }
    fn goto_ref_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::ReferenceParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position.text_document.uri);
            let pos = params.text_document_position.position;
            let loc = lsp::Location::new(uri.clone(),lsp::Range::new(pos,pos));
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                *resp = match serde_json::to_value::<Vec<lsp::Location>>(chkpt.get_refs(&loc)) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,Some(result)),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"goto refs failed while parsing".to_string())
                };
            }
        }
    }
    fn rename_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::RenameParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position.text_document.uri);
            let pos = params.text_document_position.position;
            let sel_loc = lsp::Location::new(uri.clone(),lsp::Range::new(pos,pos));
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                let mut changes: HashMap<lsp::Uri,Vec<lsp::TextEdit>> = HashMap::new();
                let locs = chkpt.get_renamables(&sel_loc);
                for loc in locs {
                    let new_edit = lsp::TextEdit::new(loc.range, params.new_name.clone());
                    match changes.get_mut(&loc.uri) {
                        Some(edits) => edits.push(new_edit),
                        None => {
                            let edits = vec![new_edit];
                            changes.insert(loc.uri,edits);
                        }
                    };
                }
                *resp = match serde_json::to_value::<lsp::WorkspaceEdit>(lsp::WorkspaceEdit::new(changes)) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,result),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"rename request failed while parsing".to_string())
                };
            }
        }
    }
    fn folding_range_response(chkpts: HashMap<String,Arc<&Self>>, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::FoldingRangeParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document.uri);
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                let folding_ranges = chkpt.get_folding_ranges();
                *resp = match serde_json::to_value::<Vec<lsp::FoldingRange>>(folding_ranges) {
                    Ok(result) => lsp_server::Response::new_ok(req.id,result),
                    Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"folding range request failed while parsing".to_string())
                };
            }
        }
    }
    fn hover_response<HOV: Hovers>(chkpts: HashMap<String,Arc<&Self>>, hov: &mut HOV, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::HoverParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position_params.text_document.uri);
            let pos = params.text_document_position_params.position;
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                if let Some(line) = chkpt.get_line(pos.line as usize) {
                    *resp = match hov.get(line,pos.line as isize, pos.character as isize) {
                        Some(hover) => match serde_json::to_value::<lsp::Hover>(hover) {
                            Ok(result) => lsp_server::Response::new_ok(req.id,result),
                            Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"hover request failed while parsing".to_string())
                        },
                        None => lsp_server::Response::new_ok(req.id,serde_json::Value::Null)
                    };
                }
            }
        }
    }
    fn completion_response<CMP: Completions>(chkpts: HashMap<String,Arc<&Self>>, cmp: &mut CMP, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::CompletionParams>(req.params) {
            let uri = super::normalize_client_uri(params.text_document_position.text_document.uri);
            let pos = params.text_document_position.position;
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                if let Some(ctx) = params.context {
                    *resp = match cmp.get(&mut chkpt.get_doc().text.lines(),&ctx,&pos) {
                        Ok(lst) => {
                            match serde_json::to_value::<lsp::CompletionResponse>(lsp::CompletionResponse::Array(lst)) {
                                Ok(result) => lsp_server::Response::new_ok(req.id,result),
                                Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"completion request failed while parsing".to_string())
                            }
                        },
                        Err(s) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,s)
                    };
                }
            }
        }
    }
    fn sem_tok_response<TOK: Tokens>(chkpts: HashMap<String,Arc<&Self>>, tok: &mut TOK, req: lsp_server::Request, resp: &mut lsp_server::Response) {
        if let Ok(params) = serde_json::from_value::<lsp::SemanticTokensParams>(req.params) {
            let uri: lsp::Uri =super::normalize_client_uri(params.text_document.uri);
            if let Some(chkpt) = chkpts.get(&uri.to_string()) {
                let doc = chkpt.get_doc();
                if let Ok(tok) = tok.get(&doc.text) {
                    *resp = match serde_json::to_value::<lsp::SemanticTokensResult>(lsp::SemanticTokensResult::Tokens(tok)) {
                        Ok(result) => lsp_server::Response::new_ok(req.id,Some(result)),
                        Err(_) => lsp_server::Response::new_err(req.id,rpc_error::PARSE_ERROR,"semantic tokens failed while parsing".to_string())
                    };
                }
            }
        }
    }
}

/// This trait object can serve either an ordinary LSP client,
/// or the `verify` subcommand, whether it is run from the
/// console or in a subprocess.  For the LSP wrap this in Arc<Mutex<>>
/// so the analysis can run in a parallel thread.
pub trait Analysis {
    /// Analyze source directories and volatile documents that define the workspace.
    /// This should gather workspace level symbols and define any relationships
    /// that may exist between files.
    fn init_workspace(&mut self,_source_dirs: Vec<lsp::Uri>,_volatile_docs: Vec<super::Document>) -> STDRESULT {
        Ok(())
    }
    /// Analyze a master document to produce diagnostic and symbol information.
    fn analyze(&mut self,doc: &super::Document) -> STDRESULT;
    /// Parse the JSON to update the configuration.
    /// Unknown keys or unexpected values can be handled as the anlayzer chooses.
    /// This tends to be used for the CLI rather than the language server.
    fn update_config(&mut self,json_str: &str) -> STDRESULT;
    /// Get a clone of the diagnostics for the given document.
    /// The available documents are the master that was analyzed, or
    /// any of its includes.
    fn get_diags(&self,doc: &super::Document) -> Vec<lsp::Diagnostic>;
    fn get_folds(&self,doc: &super::Document) -> Vec<lsp::FoldingRange>;
    fn err_warn_info_counts(&self) -> [usize;3];
    fn eprint_lines_sexpr(&self,doc: &str);
    /// If console start interactive entry, otherwise empty input pipe into string.
    fn read_stdin(&self) -> String;
}

pub struct SemanticTokensBuilder {
    last_pos: lsp::Position,
    tok_map: HashMap<String,u32>,
    tokens: Vec<lsp::SemanticToken>,
    hex_re: regex::Regex
}

impl SemanticTokensBuilder {
    pub fn new() -> Self {
        let mut tok_map = HashMap::new();
        let types = Self::get_token_types();
        for i in 0..types.len() {
            tok_map.insert(types[i].clone(),i as u32);
        }
        Self {
            last_pos: lsp::Position::new(0,0),
            tok_map,
            tokens: Vec::new(),
            hex_re: regex::Regex::new(r"\\x[0-9a-fA-F][0-9a-fA-F]").expect("bad regex")
        }
    }
    pub fn get_token_types() ->Vec<String> {
        TOKEN_TYPES.iter().map(|x| x.to_string()).collect()
    }
    pub fn reset(&mut self) {
        self.tokens = Vec::new();
        self.last_pos = lsp::Position::new(0,0);
    }
    pub fn clone_result(&self) -> Result<lsp::SemanticTokens,DYNERR> {
        Ok(lsp::SemanticTokens {
			result_id: None,
			data: self.tokens.clone()
		})
    }
    pub fn process_escapes(&mut self,curs: &tree_sitter::TreeCursor,line: &str,rng: lsp::Range,typ: &str) {
        let pos0 = rng.start.character;
        let mut pos = rng.start.character;
        let txt = super::node_text(&curs.node(), line);
        let re_clone = self.hex_re.clone();
        for mtch in re_clone.find_iter(&txt) {
            let esc_start =  pos0 + mtch.start() as u32;
            let esc_end = pos0 + mtch.end() as u32;
            let outer = lsp::Range::new(
                lsp::Position::new(rng.start.line,pos),
                lsp::Position::new(rng.start.line, esc_start)
            );
            self.push(outer,typ);
            let emb = lsp::Range::new(
                lsp::Position::new(rng.start.line, esc_start),
                lsp::Position::new(rng.start.line, esc_end)
            );
            self.push(emb, "regexp");
            pos = esc_end;
        }
        let outer = lsp::Range::new(
            lsp::Position::new(rng.start.line, pos),
            rng.end
        );
        self.push(outer, typ);
    }
    pub fn push(&mut self,rng: lsp::Range, typ: &str) {
        if let Some(code) = self.tok_map.get(typ) {
            if rng.start.line >= self.last_pos.line {
                if rng.start.line == self.last_pos.line && rng.start.character < self.last_pos.character {
                    return;
                }
                self.tokens.push(lsp::SemanticToken {
                    delta_line: rng.start.line - self.last_pos.line,
                    delta_start: match rng.start.line == self.last_pos.line {
                        true => rng.start.character - self.last_pos.character,
                        false => rng.start.character
                    },
                    length: rng.end.character - rng.start.character,
                    token_type: *code,
                    token_modifiers_bitset: 0
                });
                self.last_pos.line = rng.start.line;
                self.last_pos.character = rng.start.character;
            }
        }
    }
}

pub fn send_edit_req(connection: &lsp_server::Connection, doc: &lsp::TextDocumentItem, edits: Vec<lsp::TextEdit>) -> Result<(),String> {
    let mut edit_list = Vec::new();
    edit_list.push(lsp::TextDocumentEdit {
        text_document: lsp::OptionalVersionedTextDocumentIdentifier::new(doc.uri.clone(), doc.version),
        edits: edits.iter().map(|x| lsp::OneOf::Left(x.clone())).collect()
    });
    let ws_edit = lsp::WorkspaceEdit {
        changes: None,
        document_changes: Some(lsp::DocumentChanges::Edits(edit_list)),
        change_annotations: None
    };
    // send the edit request
    if let Ok(params) = serde_json::to_value(lsp::ApplyWorkspaceEditParams {label: None,edit: ws_edit}) {
        let req = lsp_server::Request {
            id: lsp_server::RequestId::from("renumber".to_string()),
            method: lsp::request::ApplyWorkspaceEdit::METHOD.to_string(),
            params
        };
        match connection.sender.send(lsp_server::Message::Request(req)) {
            Ok(()) => Ok(()),
            Err(_) => Err("could not send".to_string())
        }
    } else {
        Err("could not parse".to_string())
    }
}

pub fn basic_diag(range: lsp::Range,mess: &str,severity: lsp::DiagnosticSeverity) -> lsp::Diagnostic {
    lsp::Diagnostic {
        range,
        severity: Some(severity),
        code: None,
        code_description: None,
        source: None,
        message: mess.to_string(),
        related_information: None,
        tags: None,
        data: None
    }
}

/// Get a path relative to the workspace path for display purposes.
/// Only checks the first workspace folder.
/// If there is any failure we keep the whole URI string.
pub fn path_in_workspace(full: &lsp::Uri, ws_folder: &Vec<lsp::Uri>) -> String {
    if ws_folder.len() == 0 {
        return full.to_string();
    }
    let full_path = match super::pathbuf_from_uri(full) {
        Ok(ans) => ans,
        Err(_) => return full.to_string()
    };
    let ws_path = match super::pathbuf_from_uri(&ws_folder[0]) {
        Ok(ans) => ans,
        Err(_) => return full.to_string()
    };
    let e_full_canon = full_path.canonicalize();
    let e_ws_canon = ws_path.canonicalize();
    match (e_full_canon,e_ws_canon) {
        (Ok(full_canon),Ok(ws_canon)) => {
            let mut full_iter = full_canon.iter();
            let mut ws_iter = ws_canon.iter();
            while let Some(ws_node) = ws_iter.next() {
                if let Some(node) = full_iter.next() {
                    if node != ws_node {
                        return full.to_string();
                    }
                } else {
                    return full.to_string();
                }
            }
            let mut ans = String::new();
            while let Some(node) = full_iter.next() {
                ans += &node.to_string_lossy();
                ans += "/";
            }
            if ans.len() < 2 {
                return full.to_string();
            }
            ans.pop();
            ans
        },
        _ => full.to_string()
    }
}

fn setup_env_logger(filt: log::LevelFilter, path: &str) {
    if filt==log::LevelFilter::Off {
        return;
    }
    let a2kit_logging_file = Box::new(std::fs::File::create(path).expect("failed to create log file"));
    env_logger::Builder::new().format(|buf,record| {
        writeln!(buf,"{}:{} [{}] - {}",record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.level(),
            record.args()
        )
    })
    .filter(Some("a2kit::lang"),filt)
    .target(env_logger::Target::Pipe(a2kit_logging_file))
    .init();
}

/// Parse the language server's command line arguments.
/// Sets up logging based on the arguments, panics if log level or log file are invalid.
/// As of this writing it returns only the `--suppress-tokens` status in `parse_args().0[0]`.
pub fn parse_args() -> (Vec<bool>,Vec<String>) {
    let mut log_level = log::LevelFilter::Off;
    let mut log_file = "a2kit_log.txt".to_string();
    let mut suppress_tokens = false;
    
    // process arguments
    let mut args = std::env::args().into_iter();
    args.next();
    while let Some(val) = args.next() {
        if &val == "--log-level" {
            if let Some(val) = args.next() {
                log_level = log::LevelFilter::from_str(&val).expect("invalid logging filter");
            }
        } else if &val == "--log-file" {
            if let Some(val) = args.next() {
                log_file = val;
            }
        } else if &val == "--suppress-tokens" {
            // tokens will only be sent to client upon request
            suppress_tokens = true;
        }
    }
    setup_env_logger(log_level, &log_file);
    (vec![suppress_tokens],vec![])
}
