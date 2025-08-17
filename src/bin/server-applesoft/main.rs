
//! This is the Applesoft language server.
//! Cargo will compile this to a standalone executable.
//! 
//! The a2kit library crate provides most of the analysis.
//! The server activity is all in this file.

use lsp_types as lsp;
use lsp::{notification::Notification, request::Request};
use lsp_server;
use serde_json;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc,Mutex};
use a2kit::lang::server::TOKEN_TYPES; // used if we register tokens on server side
use a2kit::lang::server::Analysis;
use a2kit::lang::applesoft;
use a2kit::lang::applesoft::diagnostics::Analyzer;
use a2kit::lang::disk_server::DiskServer;

mod notification;
mod request;
mod response;

// JSON-RPC error codes; are they defined somewhere else?
// -32768 through -32000 are reserved
mod rpc_error {
    pub const PARSE_ERROR: i32 = -32700;
    // pub const INVALID_REQUEST: i32 = -32600;
    // pub const METHOD_NOT_FOUND: i32 = -32601;
    // pub const INVALID_PARAMS: i32 = -32602;
    // pub const INTERNAL_ERROR: i32 = -32603;
}

#[derive(thiserror::Error,Debug)]
enum ServerError {
    #[error("Parsing")]
    Parsing
}

struct AnalysisResult {
    uri: lsp::Uri,
    version: Option<i32>,
    diagnostics: Vec<lsp::Diagnostic>,
    symbols: applesoft::Symbols
}

/// Send log messages to the client.
/// TODO: implement as log crate trait object (we would have to take ownership of connection, solve lifetimes)
fn logger(connection: &lsp_server::Connection, message: &str) {
    let note = lsp_server::Notification::new(
        lsp::notification::LogMessage::METHOD.to_string(),
        lsp::LogMessageParams {
            typ: lsp::MessageType::LOG,
            message: message.to_string()
        }
    );
    match connection.sender.send(lsp_server::Message::Notification(note)) {
        Err(_) => {}, // nowhere to send log, what can we do about it?
        Ok(()) => {}
    }
}

/// request the root configuration item
fn request_configuration(connection: &lsp_server::Connection) -> Result<(),Box<dyn Error>> {
    let req = lsp_server::Request::new(
        lsp_server::RequestId::from("applesoft-pull-config".to_string()),
        lsp::request::WorkspaceConfiguration::METHOD.to_string(),
        lsp::ConfigurationParams { items: vec![
            lsp::ConfigurationItem {
                scope_uri: None,
                section: Some("applesoft".to_string())
            }
        ]}
    );
    match connection.sender.send(req.into()) {
        Ok(()) => Ok(()),
        Err(e) => Err(Box::new(e))
    }
}

/// parse the response to the configuration request
fn parse_configuration(resp: lsp_server::Response) -> Result<applesoft::settings::Settings,Box<dyn Error>> {
    if let Some(result) = resp.result {
        if let Some(ary) = result.as_array() {
            // This loop always exits in the first iteration, since we only requested 1 item
            for item in ary {
                let json_config = item.to_string();
                match applesoft::settings::parse(&json_config) {
                    Ok(config) => return Ok(config),
                    Err(e) => return Err(e)
                }
            }    
        }
    }
    Err(Box::new(ServerError::Parsing))
}

fn launch_analysis_thread(analyzer: Arc<Mutex<Analyzer>>, doc: a2kit::lang::Document) -> std::thread::JoinHandle<Option<AnalysisResult>> {
    std::thread::spawn( move || {
        match analyzer.lock() {
            Ok(mut analyzer) => {
                match analyzer.analyze(&doc) {
                    Ok(()) => Some(AnalysisResult {
                        uri: doc.uri.clone(),
                        version: doc.version,
                        diagnostics: analyzer.get_diags(&doc),
                        symbols: analyzer.get_symbols()
                    }),
                    Err(_) => None
                }
            }
            Err(_) => None
        }    
    })
}

/// Diagnostics are never requested by the client.
/// This server pushes them up after analysis pass, which in turn is triggered by document changes.
pub fn push_diagnostics(connection: &lsp_server::Connection,uri: lsp::Uri, version: Option<i32>, diagnostics: Vec<lsp::Diagnostic>) {
    let note = lsp_server::Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        lsp::PublishDiagnosticsParams {
            uri,
            diagnostics,
            version
        }
    );
    match connection.sender.send(lsp_server::Message::Notification(note)) {
        Err(_) => logger(connection,"could not push diagnostics"),
        Ok(()) => {}
    }
}

struct Tools {
    config: applesoft::settings::Settings,
    thread_handles: VecDeque<std::thread::JoinHandle<Option<AnalysisResult>>>,
    doc_chkpts: HashMap<String,applesoft::checkpoint::CheckpointManager>,
    analyzer: Arc<Mutex<applesoft::diagnostics::Analyzer>>,
    hover_provider: applesoft::hovers::HoverProvider,
    completion_provider: applesoft::completions::CompletionProvider,
    highlighter: applesoft::semantic_tokens::SemanticTokensProvider,
    minifier: applesoft::minifier::Minifier,
    tokenizer: applesoft::tokenizer::Tokenizer,
    disk: DiskServer
}

impl Tools {
    pub fn new() -> Self {
        Self {
            config: applesoft::settings::Settings::new(),
            thread_handles: VecDeque::new(),
            doc_chkpts: HashMap::new(),
            analyzer: Arc::new(Mutex::new(Analyzer::new())),
            hover_provider: applesoft::hovers::HoverProvider::new(),
            completion_provider: applesoft::completions::CompletionProvider::new(),
            highlighter: applesoft::semantic_tokens::SemanticTokensProvider::new(),
            minifier: applesoft::minifier::Minifier::new(),
            tokenizer: applesoft::tokenizer::Tokenizer::new(),
            disk: DiskServer::new()
        }
    }
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (bools,_) = a2kit::lang::server::parse_args();
    let suppress_tokens = bools[0];

    let mut tools = Tools::new();
    let (connection, io_threads) = lsp_server::Connection::stdio();

    logger(&connection,"start initializing connection");
    let (id,params) = connection.initialize_start()?;
    let params: lsp::InitializeParams = serde_json::from_value(params)?;
    let result = lsp::InitializeResult {
        capabilities: lsp::ServerCapabilities {
            text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
                lsp::TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(lsp::TextDocumentSyncKind::FULL), 
                    will_save: None,
                    will_save_wait_until: None,
                    save: None
                }
            )),
            definition_provider: Some(lsp::OneOf::Left(true)),
            declaration_provider: Some(lsp::DeclarationCapability::Simple(true)),
            references_provider: Some(lsp::OneOf::Left(true)),
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec!["\n".to_string()," ".to_string()]),
                ..lsp::CompletionOptions::default()
            }),
            document_symbol_provider: Some(lsp::OneOf::Left(true)),
            rename_provider: Some(lsp::OneOf::Left(true)),
            semantic_tokens_provider: match suppress_tokens {
                true => None,
                false => Some(lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(lsp::SemanticTokensOptions {
                    work_done_progress_options: lsp::WorkDoneProgressOptions {
                        work_done_progress: None
                    },
                    legend: lsp::SemanticTokensLegend {
                        token_types: TOKEN_TYPES.iter().map(|x| lsp::SemanticTokenType::new(x)).collect(),
                        token_modifiers: vec![]
                    },
                    range: None,
                    full: Some(lsp::SemanticTokensFullOptions::Bool(true))
                }))
            },
            execute_command_provider: Some(lsp::ExecuteCommandOptions {
                commands: [
                    "applesoft.semantic.tokens",
                    "applesoft.minify",
                    "applesoft.tokenize",
                    "applesoft.detokenize",
                    "applesoft.renumber",
                    "applesoft.move",
                    "applesoft.disk.mount",
                    "applesoft.disk.pick",
                    "applesoft.disk.put",
                    "applesoft.disk.delete",
                ].iter().map(|x| x.to_string()).collect::<Vec<String>>(),
                work_done_progress_options: lsp::WorkDoneProgressOptions {
                    work_done_progress: None
                }
            }),
            ..lsp::ServerCapabilities::default()
        },
        server_info: Some(lsp::ServerInfo {
            name: "applesoft".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string())
        })
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;
    logger(&connection,"connection initialized");

    // registrations
    let mut registrations: Vec<lsp::Registration> = Vec::new();
    if let Some(workspace) = params.capabilities.workspace {
        if let Some(config) = workspace.configuration {
            if config {
                registrations.push(lsp::Registration {
                    id: "pull-config".to_string(),
                    method: lsp::notification::DidChangeConfiguration::METHOD.to_string(),
                    register_options: None
                });
            }
        }
    }
    let req = lsp_server::Request::new(
        lsp_server::RequestId::from("applesoft-reg-config".to_string()),
        lsp::request::RegisterCapability::METHOD.to_string(),
        lsp::RegistrationParams { registrations });
    if let Err(_) = connection.sender.send(req.into()) {
        logger(&connection,"Could not register change configuration capability");
    }

    // Starting configuration
    match request_configuration(&connection) {
        Ok(()) => {},
        Err(_) => logger(&connection,"could not request starting configuration")
    }

    // Main loop
    loop {

        // Gather data from analysis threads
        if let Some(oldest) = tools.thread_handles.front() {
            if oldest.is_finished() {
                let done = tools.thread_handles.pop_front().unwrap();
                if let Ok(Some(result)) = done.join() {
                    if let Some(chkpt) = tools.doc_chkpts.get_mut(&result.uri.to_string()) {
                        chkpt.update_symbols(result.symbols);
                        tools.hover_provider.use_shared_symbols(chkpt.shared_symbols());
                        tools.completion_provider.use_shared_symbols(chkpt.shared_symbols());
                    }
                    push_diagnostics(&connection, result.uri, result.version, result.diagnostics);
                }
            }
        }

        // Handle messages from the client
        if let Ok(msg) = connection.receiver.recv_timeout(std::time::Duration::from_millis(100)) {
            match msg {
                lsp_server::Message::Notification(note) => {
                    notification::handle_notification(&connection,note,&mut tools);
                }
                lsp_server::Message::Request(req) => {
                    if request::handle_request(&connection, req, &mut tools) {
                        break;
                    }
                },
                lsp_server::Message::Response(resp) => {
                    response::handle_response(&connection, resp, &mut tools);
                }
            }
        }
    }

    io_threads.join()?;
    Ok(())
}