//! Handle incoming responses to our requests

use lsp_server;
use std::sync::{Mutex,Arc};
use a2kit::lang::server::Checkpoint;
use a2kit::lang::merlin::diagnostics::Analyzer;
use a2kit::lang::merlin::Workspace;
use super::logger;

pub fn handle_response(connection: &lsp_server::Connection, resp: lsp_server::Response, tools: &mut super::Tools) {
    match resp.id.to_string().as_str() {
        "\"merlin6502-pull-config\"" => {
            match super::parse_configuration(resp) {
                Ok(config) => {
                    let mut workspace_data = Workspace::new();
                    tools.config = config.clone();
                    tools.hover_provider.set_config(config.clone());
                    tools.completion_provider.set_config(config.clone());
                    tools.tokenizer.set_config(&config);
                    tools.formatter.set_config(&config);
                    tools.assembler.set_config(config.clone());
                    tools.disassembler.set_config(config.clone());

                    // configure main analyzer
                    if let Ok(mut mutex) = tools.analyzer.lock() {
                        mutex.set_config(config.clone());
                        if let Err(_) = mutex.rescan_workspace(false) {
                            logger(&connection,"failed to rescan workspace after user changed settings");
                        }
                        workspace_data = mutex.get_workspace().clone();
                    }

                    // run through open documents and update
                    for (key,chkpt) in &tools.doc_chkpts {
                        let doc = chkpt.get_doc();
                        let mut loc_analyzer = Analyzer::new();
                        loc_analyzer.set_config(config.clone());
                        loc_analyzer.set_workspace(workspace_data.clone());
                        logger(&connection,&format!("updated configuration for {}",key));
                        let handle = super::launch_analysis_thread(
                            Arc::new(Mutex::new(loc_analyzer)),
                            doc,
                            crate::WorkspaceScanMethod::None
                        );
                        tools.thread_handles.push_back(handle);
                    }
                },
                Err(_) => logger(&connection,"could not parse config")
            }
        },
        "\"merlin6502-reg-config\"" => {
            logger(&connection,"registration response was received");
        }
        s => {
            logger(&connection,&format!("unhandled response: {}",s))
        }
    }
}