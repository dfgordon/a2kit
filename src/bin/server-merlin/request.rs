//! Provide our response to incoming requests

use lsp_types as lsp;
use lsp::request::Request;
use lsp_server::{Connection,RequestId,Response};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use a2kit::lang::server::{Checkpoint, Tokens};
use a2kit::lang::{disk_server, merlin, normalize_client_uri, normalize_client_uri_str};
use a2kit::lang::merlin::formatter;
use a2kit::lang::merlin::disassembly::DasmRange;
use a2kit::lang::merlin::ProcessorType;
use super::logger;
use super::rpc_error::PARSE_ERROR;
use crate::launch_analysis_thread;

fn def_response(req_id: RequestId, meth: &str) -> lsp_server::Response {
    let mess = req_id.to_string();
    lsp_server::Response::new_err(req_id,PARSE_ERROR,format!("request {} ({}) not understood",mess,meth))
}

/// returns true if there was a shutdown request
pub fn handle_request(
    connection: &Connection,
    req: lsp_server::Request,
    tools: &mut super::Tools) -> bool {

    let mut resp = def_response(req.id.clone(),&req.method);
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
        lsp::request::FoldingRangeRequest::METHOD => Checkpoint::folding_range_response(chkpts, req.clone(), &mut resp),

        lsp::request::Shutdown::METHOD => {
            logger(&connection,"shutdown request");
            resp = lsp_server::Response::new_ok(req.id.clone(), ());
            connection.sender.send(resp.into()).expect("failed to respond to shutdown request");
            connection.receiver.recv_timeout(std::time::Duration::from_secs(30)).expect("failure while pausing");
            return true;
        },

        lsp::request::RangeFormatting::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::DocumentRangeFormattingParams>(req.params) {
                let normalized_uri = normalize_client_uri(params.text_document.uri);
                if let Some(chk) = tools.doc_chkpts.get(normalized_uri.as_str()) {
                    tools.tokenizer.use_shared_symbols(chk.shared_symbols());
                    resp = match formatter::format_range(chk.get_doc().text, params.range, &mut tools.tokenizer) {
                        Ok(edits) => lsp_server::Response::new_ok(req.id,edits),
                        Err(_) => lsp_server::Response::new_err(req.id,PARSE_ERROR,"formatting failed".to_string())
                    };
                }
            }
        },

        lsp::request::OnTypeFormatting::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::DocumentOnTypeFormattingParams>(req.params) {
                let normalized_uri = normalize_client_uri(params.text_document_position.text_document.uri);
                if let Some(chk) = tools.doc_chkpts.get(normalized_uri.as_str()) {
                    tools.formatter.use_shared_symbols(chk.shared_symbols());
                    let edits = tools.formatter.format_typing(&chk.get_doc(), params.text_document_position.position, &params.ch);
                    resp =  lsp_server::Response::new_ok(req.id,edits);
                }
            }
        },
        
        lsp::request::ExecuteCommand::METHOD => {
            if let Ok(params) = serde_json::from_value::<lsp::ExecuteCommandParams>(req.params) {
                match params.command.as_str() {
                    "merlin6502.getMasterList" => {
                        if params.arguments.len()==1 {
                            let uri_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            if let Ok(uri) = uri_res {
                                let normalized_uri = normalize_client_uri(uri.parse().expect("could not parse URI"));
                                let masters = tools.workspace.get_masters(&normalized_uri).iter().map(|s| s.to_owned()).collect::<Vec<String>>();
                                resp = lsp_server::Response::new_ok(req.id,masters);
                            }
                        }
                    },
                    "merlin6502.selectMaster" => {
                        if params.arguments.len()==2 {
                            let display_uri = serde_json::from_value::<String>(params.arguments[0].clone());
                            let master_uri = serde_json::from_value::<String>(params.arguments[1].clone());
                            if let (Ok(disp),Ok(mast)) = (display_uri,master_uri) {
                                let normalized_disp = normalize_client_uri_str(&disp).expect("could not parse URI");
                                let normalized_mast = normalize_client_uri_str(&mast).expect("could not parse URI");
                                if let Ok(mut mutex) = tools.analyzer.lock() {
                                    mutex.set_preferred_master(normalized_disp.to_string(), normalized_mast.to_string());
                                }
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_disp.to_string()) {
                                    let handle = launch_analysis_thread(
                                        Arc::clone(&tools.analyzer),
                                        chk.get_doc(),
                                        crate::WorkspaceScanMethod::FullUpdate
                                    );
                                    tools.thread_handles.push_back(handle);
                                }
                                resp = lsp_server::Response::new_ok(req.id,serde_json::Value::Null);
                            }
                        }
                    },
                    "merlin6502.rescan" => {
                        if params.arguments.len()==1 {
                            let uri_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            if let Ok(uri) = uri_res {
                                let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                    let handle = launch_analysis_thread(
                                        Arc::clone(&tools.analyzer),
                                        chk.get_doc(),
                                        crate::WorkspaceScanMethod::FullUpdate
                                    );
                                    tools.thread_handles.push_back(handle);
                                }
                                resp = lsp_server::Response::new_ok(req.id,serde_json::Value::Null);
                            }
                        }
                    }
                    "merlin6502.activeEditorChanged" => {
                        if params.arguments.len()==1 {
                            let uri_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            if let Ok(uri) = uri_res {
                                let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                    let handle = launch_analysis_thread(
                                        Arc::clone(&tools.analyzer),
                                        chk.get_doc(),
                                        crate::WorkspaceScanMethod::UseCheckpoints
                                    );
                                    tools.thread_handles.push_back(handle);
                                }
                                resp = lsp_server::Response::new_ok(req.id,serde_json::Value::Null);
                            }
                        }
                    },
                    "merlin6502.semantic.tokens" => {
                        if params.arguments.len()==2 {
                            let prog_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            let uri_res = serde_json::from_value::<String>(params.arguments[1].clone());
                            if let (Ok(program),Ok(uri)) = (prog_res,uri_res) {
                                let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                    tools.highlighter.use_shared_symbols(chk.shared_symbols());
                                } else {
                                    // need to clear symbols if there is no checkpoint
                                    tools.highlighter.use_shared_symbols(Arc::new(a2kit::lang::merlin::Symbols::new()));
                                }
                                // decision here is to highlight even if no symbols found
                                resp = match tools.highlighter.get(&program) {
                                    Ok(result) => {
                                        lsp_server::Response::new_ok(req.id,result)
                                    },
                                    Err(_) => lsp_server::Response::new_err(req.id,PARSE_ERROR,"semantic tokens failed".to_string())
                                };
                            }
                        }
                    },
                    "merlin6502.pasteFormat" => {
                        if params.arguments.len()==2 {
                            let prog_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            let uri_res = serde_json::from_value::<String>(params.arguments[1].clone());
                            if let (Ok(program),Ok(uri)) = (prog_res,uri_res) {
                                let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                    tools.tokenizer.use_shared_symbols(chk.shared_symbols());
                                    resp = match formatter::format_for_paste(program,&mut tools.tokenizer) {
                                        Ok(result) => {
                                            lsp_server::Response::new_ok(req.id,result)
                                        },
                                        Err(_) => lsp_server::Response::new_err(req.id,PARSE_ERROR,"formatting failed".to_string())
                                    };
                                } else {
                                    resp = lsp_server::Response::new_err(req.id,PARSE_ERROR,"cannot format due to missing checkpoint".to_string());
                                }
                            }
                        }
                    },
                    "merlin6502.detokenize" => {
                        if params.arguments.len()==1 {
                            if let Ok(buf) = serde_json::from_value::<Vec<u8>>(params.arguments[0].clone()) {
                                resp = match tools.tokenizer.detokenize(&buf) {
                                    Ok(result) => lsp_server::Response::new_ok(req.id,result),
                                    Err(_) => lsp_server::Response::new_err(req.id,PARSE_ERROR,"detokenize failed".to_string())
                                };
                            }
                        }
                    },
                    "merlin6502.disassemble" => {
                        if params.arguments.len()==7 {
                            let maybe_img = serde_json::from_value::<Vec<u8>>(params.arguments[0].clone());
                            let maybe_beg = serde_json::from_value::<usize>(params.arguments[1].clone());
                            let maybe_end = serde_json::from_value::<usize>(params.arguments[2].clone());
                            let maybe_rng_typ = serde_json::from_value::<String>(params.arguments[3].clone());
                            let maybe_xc = serde_json::from_value::<usize>(params.arguments[4].clone());
                            let maybe_mx = serde_json::from_value::<usize>(params.arguments[5].clone());
                            let maybe_lab = serde_json::from_value::<String>(params.arguments[6].clone());
                            resp = match (maybe_img,maybe_beg,maybe_end,maybe_rng_typ,maybe_xc,maybe_mx,maybe_lab) {
                                (Ok(img),Ok(beg),Ok(end),Ok(rng_typ),Ok(xc),Ok(mx),Ok(lab)) => {
                                    let proc = match xc {
                                        0 => ProcessorType::_6502,
                                        1 => ProcessorType::_65c02,
                                        _ => ProcessorType::_65c816
                                    };
                                    let range = match rng_typ.as_str() {
                                        "all" => DasmRange::All,
                                        "last dos33 bload" => DasmRange::LastBloadDos33,
                                        "last prodos bload" => DasmRange::LastBloadProDos,
                                        _ => DasmRange::Range([beg,end])
                                    };
                                    tools.disassembler.set_mx(mx & 2 > 0, mx & 1 > 0);
                                    match tools.disassembler.disassemble(&img, range, proc, &lab) {
                                        Ok(result) => Response::new_ok(req.id,result),
                                        Err(_) => lsp_server::Response::new_err(req.id,PARSE_ERROR,"dasm failed".to_string())
                                    }
                                },
                                _ => Response::new_err(req.id,PARSE_ERROR,"bad arguments to disassembler".to_string())
                            };
                        }
                    },
                    "merlin6502.toData" => {
                        if params.arguments.len()==4 {
                            let prog_res = serde_json::from_value::<String>(params.arguments[0].clone());
                            let uri_res = serde_json::from_value::<String>(params.arguments[1].clone());
                            let beg_res = serde_json::from_value::<isize>(params.arguments[2].clone());
                            let end_res = serde_json::from_value::<isize>(params.arguments[3].clone());
                            if let (Ok(program),Ok(uri),Ok(beg),Ok(end)) = (prog_res,uri_res,beg_res,end_res) {
                                let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                    let dasm_symbols = merlin::assembly::Assembler::dasm_symbols(chk.shared_symbols());
                                    tools.assembler.use_shared_symbols(Arc::new(dasm_symbols));
                                    resp = match tools.assembler.spot_assemble(program, beg, end, None) {
                                        Ok(img) => {
                                            let dasm = tools.disassembler.disassemble_as_data(&img);
                                            lsp_server::Response::new_ok(req.id,dasm)
                                        },
                                        Err(e) => {
                                            let mess = format!("spot assembler failed: {}",e.to_string());
                                            lsp_server::Response::new_err(req.id,PARSE_ERROR,mess)
                                        }
                                    };
                                } else {
                                    resp = lsp_server::Response::new_err(req.id,PARSE_ERROR,"cannot assemble due to missing checkpoint".to_string());
                                }
                            }
                        }
                    },
                    "merlin6502.disk.mount" => {
                        if params.arguments.len()==1 {
                            let maybe_img_path = serde_json::from_value::<String>(params.arguments[0].clone());
                            let white_list = vec!["a2 dos".to_string(),"prodos".to_string()];
                            if let Ok(img_path) = maybe_img_path {
                                resp = match tools.disk.mount(&img_path,&Some(white_list)) {
                                    Ok(()) => Response::new_ok(req.id,serde_json::Value::Null),
                                    Err(_) => Response::new_err(req.id,PARSE_ERROR,"unexpected format or file system".to_string())
                                };
                            } else {
                                resp = Response::new_err(req.id,PARSE_ERROR,"bad arguments while mounting image".to_string());
                            }
                        }
                    },
                    "merlin6502.disk.pick" => {
                        match tools.disk.handle_selection(&params.arguments) {
                            Ok(item) => {
                                resp = match item {
                                    disk_server::SelectionResult::Directory(dir) => Response::new_ok(req.id,serde_json::to_value(dir).expect("json")),
                                    disk_server::SelectionResult::FileData(mut sfimg) => {
                                        // encode the load address in first two bytes
                                        let mut ans = u16::to_le_bytes(sfimg.load_addr).to_vec();
                                        ans.append(&mut sfimg.data);
                                        Response::new_ok(req.id, serde_json::to_value(ans).expect("json"))
                                    }
                                };
                            },
                            Err(e) => resp = Response::new_err(req.id,PARSE_ERROR,e.to_string())
                        }
                    },
                    "merlin6502.disk.put" => {
                        if params.arguments.len()==3 {
                            let maybe_path = serde_json::from_value::<String>(params.arguments[0].clone());
                            let maybe_prog = serde_json::from_value::<String>(params.arguments[1].clone());
                            let maybe_uri = serde_json::from_value::<String>(params.arguments[2].clone());
                            resp = match (maybe_path,maybe_prog,maybe_uri) {
                                (Ok(path),Ok(program),Ok(uri)) => {
                                    let normalized_uri = normalize_client_uri_str(&uri).expect("could not parse URI");
                                    if let Some(chk) = tools.doc_chkpts.get(&normalized_uri.to_string()) {
                                        tools.tokenizer.use_shared_symbols(chk.shared_symbols());
                                        match tools.tokenizer.tokenize(program) {
                                            Ok(dat) => match tools.disk.write(&path, &dat, a2kit::commands::ItemType::MerlinTokens) {
                                                Ok(()) => Response::new_ok(req.id,serde_json::Value::Null),
                                                Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                            },
                                            Err(e) => Response::new_err(req.id,PARSE_ERROR,e.to_string())
                                        }
                                    } else {
                                        Response::new_err(req.id,PARSE_ERROR,"document symbols were not available".to_string())
                                    }
                                }
                                _ => Response::new_err(req.id,PARSE_ERROR,"parsing error during put".to_string())
                            };
                        }
                    },
                    "merlin6502.disk.delete" => {
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