//! Handle incoming responses to our requests

use lsp_server;
use std::sync::{Mutex,Arc};
use a2kit::lang::applesoft;
use a2kit::lang::server::Checkpoint;
use a2kit::lang::applesoft::diagnostics::Analyzer;
use super::logger;

pub fn handle_response(connection: &lsp_server::Connection, resp: lsp_server::Response, tools: &mut super::Tools) {
    match resp.id.to_string().as_str() {
        "\"applesoft-pull-config\"" => {
            match super::parse_configuration(resp) {
                Ok(config) => {
                    tools.config = config.clone();
                    tools.hover_provider.set_config(config.clone());
                    tools.completion_provider.set_config(config.clone());
                    tools.tokenizer.set_config(config.clone());
                    tools.minifier.set_flags(applesoft::minifier::FLAG_SAFE);

                    // configure main analyzer
                    if let Ok(mut mutex) = tools.analyzer.lock() {
                        mutex.set_config(config.clone());
                    }
                    
                    // run through open documents and update
                    for (key,chkpt) in &tools.doc_chkpts {
                        let doc = chkpt.get_doc();
                        let mut loc_analyzer = Analyzer::new();
                        loc_analyzer.set_config(config.clone());
                        logger(&connection,&format!("updated configuration for {}",key));
                        let handle = super::launch_analysis_thread(
                            Arc::new(Mutex::new(loc_analyzer)),
                            doc
                        );
                        tools.thread_handles.push_back(handle);
                    }
                    
                },
                Err(_) => logger(&connection,"could not parse config")
            }
        },
        s => {
            logger(&connection,&format!("unhandled response: {}",s))
        }
    }
}