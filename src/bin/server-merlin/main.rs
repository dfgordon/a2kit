
//! This is the merlin language server.
//! Cargo will compile this to a standalone executable.
//! 
//! The a2kit library crate provides most of the analysis.
//! The server activity is all in this file.

use lsp_types as lsp;
use std::io::Write;
use std::str::FromStr;
use lsp::{notification::Notification, request::Request};
use lsp_server;
use serde_json;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::{Arc,Mutex};
use a2kit::lang::server::Analysis;
use a2kit::lang::merlin;
use a2kit::lang::merlin::diagnostics::Analyzer;
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

enum WorkspaceScanMethod {
    None,
    UseCheckpoints,
    FullUpdate,
}

#[derive(thiserror::Error,Debug)]
enum ServerError {
    #[error("Parsing")]
    Parsing
}

struct AnalysisResult {
    uri: lsp::Url,
    version: Option<i32>,
    diagnostics: Vec<lsp::Diagnostic>,
    folding: Vec<lsp::FoldingRange>,
    symbols: merlin::Symbols,
    workspace: merlin::Workspace
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

fn update_client_toolbar(connection: &lsp_server::Connection, symbols: &merlin::Symbols) -> Result<(),Box<dyn Error>> {
    let info = symbols.toolbar_info();
    let not = lsp_server::Notification::new(
        "merlin6502.context".to_string(),
        info[0].clone()
    );
    if let Err(e) = connection.sender.send(not.into()) {
        return Err(Box::new(e))
    }
    let not = lsp_server::Notification::new(
        "merlin6502.interpretation".to_string(),
        info[1].clone()
    );
    if let Err(e) = connection.sender.send(not.into()) {
        return Err(Box::new(e))
    }
    Ok(())
}

/// request the root configuration item
fn request_configuration(connection: &lsp_server::Connection) -> Result<(),Box<dyn Error>> {
    let req = lsp_server::Request::new(
        lsp_server::RequestId::from("merlin6502-pull-config".to_string()),
        lsp::request::WorkspaceConfiguration::METHOD.to_string(),
        lsp::ConfigurationParams { items: vec![
            lsp::ConfigurationItem {
                scope_uri: None,
                section: Some("merlin6502".to_string())
            }
        ]}
    );
    match connection.sender.send(req.into()) {
        Ok(()) => Ok(()),
        Err(e) => Err(Box::new(e))
    }
}

/// parse the response to the configuration request
fn parse_configuration(resp: lsp_server::Response) -> Result<merlin::settings::Settings,Box<dyn Error>> {
    if let Some(result) = resp.result {
        if let Some(ary) = result.as_array() {
            // This loop always exits in the first iteration, since we only requested 1 item
            for item in ary {
                let json_config = item.to_string();
                match merlin::settings::parse(&json_config) {
                    Ok(config) => return Ok(config),
                    Err(e) => return Err(e)
                }
            }    
        }
    }
    Err(Box::new(ServerError::Parsing))
}

fn launch_analysis_thread(analyzer: Arc<Mutex<Analyzer>>, doc: a2kit::lang::Document, ws_scan: WorkspaceScanMethod) -> std::thread::JoinHandle<Option<AnalysisResult>> {
    std::thread::spawn( move || {
        match analyzer.lock() {
            Ok(mut analyzer) => {
                let maybe_gather = match ws_scan {
                    WorkspaceScanMethod::UseCheckpoints => Some(false),
                    WorkspaceScanMethod::FullUpdate => Some(true),
                    _ => None
                };
                if let Some(gather) = maybe_gather {
                    match analyzer.rescan_workspace(gather) {
                        Ok(()) => {},
                        Err(_) => {}
                    }
                }
                match analyzer.analyze(&doc) {
                    Ok(()) => Some(AnalysisResult {
                        uri: doc.uri.clone(),
                        version: doc.version,
                        diagnostics: analyzer.get_diags(&doc),
                        folding: analyzer.get_folds(&doc),
                        symbols: analyzer.get_symbols(),
                        workspace: analyzer.get_workspace().clone()
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
pub fn push_diagnostics(connection: &lsp_server::Connection,uri: lsp::Url, version: Option<i32>, diagnostics: Vec<lsp::Diagnostic>) {
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
    config: merlin::settings::Settings,
    workspace: merlin::Workspace,
    thread_handles: VecDeque<std::thread::JoinHandle<Option<AnalysisResult>>>,
    doc_chkpts: HashMap<String,merlin::checkpoint::CheckpointManager>,
    analyzer: Arc<Mutex<Analyzer>>,
    hover_provider: merlin::hovers::HoverProvider,
    completion_provider: merlin::completions::CompletionProvider,
    highlighter: merlin::semantic_tokens::SemanticTokensProvider,
    tokenizer: merlin::tokenizer::Tokenizer,
    formatter: merlin::formatter::Formatter,
    assembler: merlin::assembly::Assembler,
    disassembler: merlin::disassembly::Disassembler,
    disk: DiskServer
}

impl Tools {
    pub fn new() -> Self {
        Self {
            config: merlin::settings::Settings::new(),
            workspace: merlin::Workspace::new(),
            thread_handles: VecDeque::new(),
            doc_chkpts: HashMap::new(),
            analyzer: Arc::new(Mutex::new(Analyzer::new())),
            hover_provider: merlin::hovers::HoverProvider::new(),
            completion_provider: merlin::completions::CompletionProvider::new(),
            highlighter: merlin::semantic_tokens::SemanticTokensProvider::new(),
            tokenizer: merlin::tokenizer::Tokenizer::new(),
            formatter: merlin::formatter::Formatter::new(),
            assembler: merlin::assembly::Assembler::new(),
            disassembler: merlin::disassembly::Disassembler::new(),
            disk: DiskServer::new()
        }
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

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {

    let mut log_level = log::LevelFilter::Off;
    let mut log_file = "a2kit_log.txt".to_string();

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
        }
    }
    setup_env_logger(log_level, &log_file);

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
                trigger_characters: Some(["$",":","]","(","[",","].iter().map(|trig| trig.to_string()).collect()),
                ..lsp::CompletionOptions::default()
            }),
            document_symbol_provider: Some(lsp::OneOf::Left(true)),
            rename_provider: Some(lsp::OneOf::Left(true)),
            document_range_formatting_provider: Some(lsp::OneOf::Left(true)),
            document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                first_trigger_character: " ".to_string(),
                more_trigger_character: Some(vec![";".to_string()])
            }),
            folding_range_provider: Some(lsp::FoldingRangeProviderCapability::Simple(true)),
            ..lsp::ServerCapabilities::default()
        },
        server_info: Some(lsp::ServerInfo {
            name: "merlin6502".to_string(),
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
        lsp_server::RequestId::from("merlin6502-reg-config".to_string()),
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

    // Initial workspace scan
    if let Some(folders) = params.workspace_folders {
        let source_dirs = folders.iter().map(|f| f.uri.clone()).collect::<Vec<lsp::Url>>();
        tools.hover_provider.set_workspace_folder(source_dirs.clone());
        if let Ok(mut mutex) = tools.analyzer.lock() {
            match mutex.init_workspace(source_dirs, Vec::new()) {
                Ok(()) => {},
                Err(e) => logger(&connection,&format!("initial workspace scan failed: {}",e))
            }
        }
    }

    // Main loop
    while let Ok(msg) = connection.receiver.recv() {

        // Gather data from analysis threads
        if let Some(oldest) = tools.thread_handles.front() {
            if oldest.is_finished() {
                let done = tools.thread_handles.pop_front().unwrap();
                if let Ok(Some(result)) = done.join() {
                    tools.workspace = result.workspace;
                    if let Some(chkpt) = tools.doc_chkpts.get_mut(&result.uri.to_string()) {
                        update_client_toolbar(&connection, &result.symbols).expect("toolbar update failed");
                        chkpt.update_symbols(result.symbols);
                        chkpt.update_folding_ranges(result.folding);
                        tools.hover_provider.use_shared_symbols(chkpt.shared_symbols());
                        tools.completion_provider.use_shared_symbols(chkpt.shared_symbols());
                        tools.tokenizer.use_shared_symbols(chkpt.shared_symbols());
                        tools.formatter.use_shared_symbols(chkpt.shared_symbols());
                        tools.highlighter.use_shared_symbols(chkpt.shared_symbols());
                        tools.assembler.use_shared_symbols(chkpt.shared_symbols());
                    }
                    push_diagnostics(&connection, result.uri, result.version, result.diagnostics);
                }
            }
        }

        // Handle messages from the client
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

    io_threads.join()?;
    Ok(())
}