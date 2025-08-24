//! Provide our response to incoming requests

use lsp_types as lsp;
use lsp::request::Request;
use lsp_server::{Connection,RequestId,Response};
use serde_json;
use std::collections::HashMap;
use a2kit::lang::server::{send_edit_req,Checkpoint,Tokens};
use a2kit::lang::disk_server;
use a2kit::lang::integer;
use super::logger;
use super::rpc_error::PARSE_ERROR;
use std::sync::Arc;

fn def_response(req_id: RequestId) -> Response {
    let mess = req_id.to_string();
    Response::new_err(req_id,PARSE_ERROR,format!("request {} not understood",mess))
}

fn renumber_or_move(connection: &Connection, req_id: RequestId, params: &lsp::ExecuteCommandParams, allow_move: bool) -> Response {
    let mut resp = def_response(req_id.clone());
    if params.arguments.len()==5 {
        let doc_r = serde_json::from_value::<lsp::TextDocumentItem>(params.arguments[0].clone());
        let sel_r = serde_json::from_value::<Option<lsp::Range>>(params.arguments[1].clone());
        let start_r = serde_json::from_value::<String>(params.arguments[2].clone());
        let step_r = serde_json::from_value::<String>(params.arguments[3].clone());
        let update_refs_r = serde_json::from_value::<bool>(params.arguments[4].clone());
        if let (
            Ok(doc),
            Ok(sel),
            Ok(start),
            Ok(step),
            Ok(update_refs)) = (doc_r,sel_r,start_r,step_r,update_refs_r) {
            let mut renumberer = integer::renumber::Renumberer::new();
            let mut flags = match allow_move { true => integer::renumber::flags::REORDER, false => 0 };
            flags += match update_refs { true => 0, false => integer::renumber::flags::PASS_OVER_REFS };
            renumberer.set_flags(flags);
            resp = match renumberer.get_edits(&doc.text, sel, &start, &step) {
                Ok(edits) => {
                    match send_edit_req(connection, &doc, edits) {
                        Ok(()) => {},
                        Err(s) => return Response::new_err(req_id.clone(),PARSE_ERROR,s)
                    };
                    Response::new_ok(req_id,serde_json::Value::Null)
                },
                Err(s) => Response::new_err(req_id.clone(),PARSE_ERROR,s)
            };
        }
    }
    resp
}

/// returns true if there was a shutdown request
pub fn handle_request(
    connection: &Connection,
    req: lsp_server::Request,
    tools: &mut super::Tools) -> bool {

    let mut resp = def_response(req.id.clone());
    let mut chkpts = HashMap::new();
    for (k,v) in &tools.doc_chkpts {
        chkpts.insert(k.to_string(),Arc::new(v));
    }

    match req.method.as_str() {
        lsp::request::GotoDeclaration::METHOD => Checkpoint::goto_dec_response(chkpts, req.clone(), &mut resp),
        lsp::request::GotoDefinition::METHOD => Checkpoint::goto_def_response(chkpts, req.clone(), &mut resp),
        lsp::request::DocumentSymbolRequest::METHOD => Checkpoint::symbol_response(chkpts, req.clone(), &mut resp),
        lsp::request::References::METHOD => Checkpoint::goto_ref_response(chkpts, req.clone(), &mut resp),
        lsp::request::Rename::METHOD => Checkpoint::rename_response(chkpts, req.clone(), &mut resp),
        lsp::request::HoverRequest::METHOD => Checkpoint::hover_response(chkpts, &mut tools.hover_provider, req.clone(), &mut resp),
        lsp::request::Completion::METHOD => Checkpoint::completion_response(chkpts, &mut tools.completion_provider, req.clone(), &mut resp),
        lsp::request::SemanticTokensFullRequest::METHOD => Checkpoint::sem_tok_response(chkpts, &mut tools.highlighter, req.clone(), &mut resp),

        lsp::request::Shutdown::METHOD => {
            logger(&connection,"shutdown request");
            resp = Response::new_ok(req.id.clone(), ());
            connection.sender.send(resp.into()).expect("failed to respond to shutdown request");
            connection.receiver.recv_timeout(std::time::Duration::from_secs(30)).expect("failure while pausing");
            return true;
        },

        lsp::request::ExecuteCommand::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::ExecuteCommandParams>(req.params) {
                match params.command.as_str() {
                    "integerbasic.semantic.tokens" => {
                        if params.arguments.len()==1 {
                            if let Ok(program) = serde_json::from_value::<String>(params.arguments[0].clone()) {
                                resp = match tools.highlighter.get(&program) {
                                    Ok(result) => {
                                        Response::new_ok(req.id,result)
                                    },
                                    Err(_) => Response::new_err(req.id,PARSE_ERROR,"semantic tokens failed".to_string())
                                };
                            }
                        }
                    }
                    "integerbasic.tokenize" => {
                        if params.arguments.len()==1 {
                            if let Ok(program) = serde_json::from_value::<String>(params.arguments[0].clone()) {
                                resp = match tools.tokenizer.tokenize(program) {
                                    Ok(result) => {
                                        Response::new_ok(req.id,result)
                                    },
                                    Err(_) => Response::new_err(req.id,PARSE_ERROR,"tokenize failed".to_string())
                                };
                            }
                        }
                    },
                    "integerbasic.detokenize" => {
                        if params.arguments.len()==1 {
                            if let Ok(buf) = serde_json::from_value::<Vec<u8>>(params.arguments[0].clone()) {
                                resp = match tools.tokenizer.detokenize_from_ram(&buf) {
                                    Ok(result) => {
                                        Response::new_ok(req.id,result)
                                    },
                                    Err(_) => Response::new_err(req.id,PARSE_ERROR,"detokenize failed".to_string())
                                };
                            }
                        }
                    },
                    "integerbasic.renumber" => {
                        resp = renumber_or_move(&connection,req.id,&params,false);
                    },
                    "integerbasic.move" => {
                        resp = renumber_or_move(&connection,req.id,&params,true);
                    },
                    "integerbasic.disk.mount" => {
                        if params.arguments.len()==1 {
                            let maybe_img_path = serde_json::from_value::<String>(params.arguments[0].clone());
                            let white_list = vec!["a2 dos".to_string(),"prodos".to_string()];
                            if let Ok(img_path) = maybe_img_path {
                                resp = match tools.disk.mount(&img_path,&Some(white_list),None) {
                                    Ok(()) => Response::new_ok(req.id,serde_json::Value::Null),
                                    Err(_) => Response::new_err(req.id,PARSE_ERROR,"unexpected format or file system".to_string())
                                };
                            } else {
                                resp = Response::new_err(req.id,PARSE_ERROR,"bad arguments while mounting image".to_string());
                            }
                        }
                    },
                    "integerbasic.disk.pick" => {
                        match tools.disk.handle_selection(&params.arguments) {
                            Ok(item) => {
                                resp = match item {
                                    disk_server::SelectionResult::Directory(dir) => Response::new_ok(req.id,serde_json::to_value(dir).expect("json")),
                                    disk_server::SelectionResult::FileData(sfimg) => {
                                        match tools.tokenizer.detokenize(&sfimg.data) {
                                            Ok(prog) => Response::new_ok(req.id,prog),
                                            Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                        }
                                    }
                                };
                            },
                            Err(e) => resp = Response::new_err(req.id,PARSE_ERROR,e.to_string())
                        }
                    },
                    "integerbasic.disk.put" => {
                        if params.arguments.len()==2 {
                            let maybe_path = serde_json::from_value::<String>(params.arguments[0].clone());
                            let maybe_prog = serde_json::from_value::<String>(params.arguments[1].clone());
                            resp = match (maybe_path,maybe_prog) {
                                (Ok(path),Ok(program)) => {
                                    match tools.tokenizer.tokenize(program) {
                                        Ok(dat) => match tools.disk.write(&path, &dat, a2kit::commands::ItemType::IntegerTokens) {
                                            Ok(()) => Response::new_ok(req.id,serde_json::Value::Null),
                                            Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                        },
                                        Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                    }
                                }
                                _ => Response::new_err(req.id,PARSE_ERROR,"parsing error during put".to_string())
                            };
                        }
                    },
                    "integerbasic.disk.delete" => {
                        if params.arguments.len()==1 {
                            resp = match serde_json::from_value::<String>(params.arguments[0].clone()) {
                                Ok(path) => {
                                    match tools.disk.delete(&path) {
                                        Ok(()) => Response::new_ok(req.id,serde_json::Value::Null),
                                        Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                    }
                                }
                                _ => Response::new_err(req.id,PARSE_ERROR,"parsing error during delete".to_string())
                            };
                        }
                    },
                    _ => {
                        logger(&connection,&format!("unhandled command {}",params.command));
                    }
                }
            }
        },
        _ => {
            logger(&connection,&format!("unhandled request: {}",req.method))
        }
    }
    // if let Some(x) = &resp.result {
    //     logger(&connection,&format!("{} : {}",req.method,x.to_string()));
    // }
    // if let Some(x) = &resp.error {
    //     logger(&connection,&format!("{}",x.message));
    // }
    if let Err(_) = connection.sender.send(lsp_server::Message::Response(resp)) {
        logger(&connection,&format!("could not send response to {}",req.method));
    }
    false
}