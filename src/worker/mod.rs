use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    Position, Range, TextEdit,
};
use parking_lot::Mutex;
use ropey::Rope;
use std::{sync::Arc, thread};

mod completion;
mod generate;
mod generate_stream;

use crate::custom_requests::generate::{GenerateParams, GenerateResult};
use crate::custom_requests::generate_stream::{GenerateStreamParams, GenerateStreamResult};
use completion::do_completion;
use generate::do_generate;
use generate_stream::do_generate_stream;

#[derive(Clone)]
pub struct CompletionRequest {
    id: RequestId,
    params: CompletionParams,
    rope: Rope,
}

impl CompletionRequest {
    pub fn new(id: RequestId, params: CompletionParams, rope: Rope) -> Self {
        Self { id, params, rope }
    }
}

#[derive(Clone)]
pub struct GenerateRequest {
    id: RequestId,
    params: GenerateParams,
    rope: Rope,
}

impl GenerateRequest {
    pub fn new(id: RequestId, params: GenerateParams, rope: Rope) -> Self {
        Self { id, params, rope }
    }
}

#[derive(Clone)]
pub struct GenerateStreamRequest {
    id: RequestId,
    params: GenerateStreamParams,
    rope: Rope,
}

impl GenerateStreamRequest {
    pub fn new(id: RequestId, params: GenerateStreamParams, rope: Rope) -> Self {
        Self { id, params, rope }
    }
}

#[derive(Clone)]
pub enum WorkerRequest {
    Completion(CompletionRequest),
    Generate(GenerateRequest),
    GenerateStream(GenerateStreamRequest),
}

pub fn run(last_worker_request: Arc<Mutex<Option<WorkerRequest>>>, connection: Arc<Connection>) {
    loop {
        let option_worker_request: Option<WorkerRequest> = {
            let mut completion_request = last_worker_request.lock();
            std::mem::take(&mut *completion_request)
        };
        if let Some(request) = option_worker_request {
            let response = match request {
                WorkerRequest::Completion(request) => match do_completion(&request) {
                    Ok(response) => {
                        let completion_text_edit = TextEdit::new(
                            Range::new(
                                Position::new(
                                    request.params.text_document_position.position.line,
                                    request.params.text_document_position.position.character,
                                ),
                                Position::new(
                                    request.params.text_document_position.position.line,
                                    request.params.text_document_position.position.character,
                                ),
                            ),
                            response.insert_text.clone(),
                        );
                        let item = CompletionItem {
                            label: format!("ai - {}", response.insert_text),
                            filter_text: Some(response.filter_text),
                            text_edit: Some(lsp_types::CompletionTextEdit::Edit(
                                completion_text_edit,
                            )),
                            kind: Some(CompletionItemKind::TEXT),
                            ..Default::default()
                        };
                        let completion_list = CompletionList {
                            is_incomplete: false,
                            items: vec![item],
                        };
                        let result = Some(CompletionResponse::List(completion_list));
                        let result = serde_json::to_value(&result).unwrap();
                        Response {
                            id: request.id,
                            result: Some(result),
                            error: None,
                        }
                    }
                    Err(e) => Response {
                        id: request.id,
                        result: None,
                        error: Some(e),
                    },
                },
                WorkerRequest::Generate(request) => match do_generate(&request) {
                    Ok(result) => {
                        let result = GenerateResult {
                            generated_text: result.generated_text,
                        };
                        let result = serde_json::to_value(&result).unwrap();
                        Response {
                            id: request.id,
                            result: Some(result),
                            error: None,
                        }
                    }
                    Err(e) => Response {
                        id: request.id,
                        result: None,
                        error: Some(e),
                    },
                },
                WorkerRequest::GenerateStream(request) => match do_generate_stream(&request) {
                    Ok(result) => {
                        // let result = GenerateResult {
                        //     generated_text: result.generated_text,
                        // };
                        // let result = serde_json::to_value(&result).unwrap();
                        let result = GenerateStreamResult {
                            generated_text: "test".to_string(),
                            partial_result_token: request.params.partial_result_token,
                        };
                        let result = serde_json::to_value(&result).unwrap();
                        Response {
                            id: request.id,
                            result: Some(result),
                            error: None,
                        }
                    }
                    Err(e) => Response {
                        id: request.id,
                        result: None,
                        error: Some(e),
                    },
                },
            };
            connection
                .sender
                .send(Message::Response(response.clone()))
                .expect("Error sending response");
            connection
                .sender
                .send(Message::Response(response.clone()))
                .expect("Error sending response");
            connection
                .sender
                .send(Message::Response(response.clone()))
                .expect("Error sending response");
            // connection
            //     .sender
            //     .send(Message::Response(Response {
            //         id: response.id,
            //         result: None,
            //         error: None,
            //     }))
            //     .expect("Error sending  message");
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
}
