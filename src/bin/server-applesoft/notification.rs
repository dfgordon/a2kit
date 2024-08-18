//! Handle incoming notifications

use lsp_types as lsp;
use lsp::{notification::Notification, NumberOrString};
use lsp_server;
use serde_json;
use std::sync::Arc;
use a2kit::lang::{normalize_client_uri,applesoft};
use crate::launch_analysis_thread;

use super::logger;

pub fn handle_notification(
    connection: &lsp_server::Connection,
    note: lsp_server::Notification,
    tools: &mut super::Tools) {
    
    match note.method.as_str() {
        lsp::notification::DidChangeConfiguration::METHOD => {
            match super::request_configuration(&connection) {
                Ok(()) => {},
                Err(_) => logger(&connection,"request for configuration failed")
            }
        },
        lsp::notification::DidOpenTextDocument::METHOD => {
            // Create checkpoint and analyzer for this document
            if let Ok(params) = serde_json::from_value::<lsp::DidOpenTextDocumentParams>(note.params) {
                let mut chkpt = applesoft::checkpoint::CheckpointManager::new();
                let normalized_uri = normalize_client_uri(params.text_document.uri);
                chkpt.update_doc(normalized_uri.clone(),params.text_document.text.clone(),Some(params.text_document.version));
                tools.doc_chkpts.insert(normalized_uri.to_string(),chkpt);
                let handle = launch_analysis_thread(
                    Arc::clone(&tools.analyzer),
                    a2kit::lang::Document {
                        uri: normalized_uri.clone(),
                        version: Some(params.text_document.version),
                        text: params.text_document.text
                    }
                );
                tools.thread_handles.push_back(handle);
            }
        },
        lsp::notification::DidCloseTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::DidCloseTextDocumentParams>(note.params) {
                let normalized_uri = normalize_client_uri(params.text_document.uri);
                tools.doc_chkpts.remove(&normalized_uri.to_string());
            }
        },
        lsp::notification::DidChangeTextDocument::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::DidChangeTextDocumentParams>(note.params) {
                let normalized_uri = normalize_client_uri(params.text_document.uri);
                if let Some(chkpt) = tools.doc_chkpts.get_mut(&normalized_uri.to_string()) {
                    for change in &params.content_changes {
                        // we asked for full documents so expect just one iteration
                        chkpt.update_doc(normalized_uri.clone(),change.text.clone(),Some(params.text_document.version));
                    }
                }
                for change in params.content_changes {
                    // we asked for full documents so expect just one iteration
                    let handle = launch_analysis_thread(
                        Arc::clone(&tools.analyzer),
                        a2kit::lang::Document {
                            uri: normalized_uri.clone(),
                            version: Some(params.text_document.version),
                            text: change.text
                        }
                    );
                    tools.thread_handles.push_back(handle);
                }
            }
        },
        lsp::notification::Cancel::METHOD => {
            // TODO: figure out when this needs to be handled specially
            if let Ok(params) = serde_json::from_value::<lsp::CancelParams>(note.params) {
                let id = match params.id {
                    NumberOrString::Number(id) => lsp_server::RequestId::from(id),
                    NumberOrString::String(s) => lsp_server::RequestId::from(s)
                };
                logger(&connection,&format!("request {} was canceled",id.to_string()));
                //let resp = lsp_server::Response::new_ok(id,serde_json::Value::Null);
                //connection.sender.send(resp.into())?;
            }
        },
        lsp::notification::SetTrace::METHOD => {
            if let Ok(_params) = serde_json::from_value::<lsp::SetTraceParams>(note.params) {
                logger(&connection,"ignoring the SetTrace notification");
            }
        }
        lsp::notification::DidChangeWatchedFiles::METHOD => {
            if let Ok(_params) = serde_json::from_value::<lsp::DidChangeWatchedFilesParams>(note.params) {
                logger(&connection,"ignoring the DidChangeWatchedFiles notification");
            }
        }
        which_method => {
            logger(&connection,&format!("unhandled notification {}",which_method))
        }
    }
}