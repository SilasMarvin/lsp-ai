use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    Position, Range, TextEdit,
};
use parking_lot::RwLock;
use ropey::Rope;
use std::{sync::Arc, thread};

mod completion;
mod generate;

use crate::custom_requests::generate::{GenerateParams, GenerateResult};
use completion::do_completion;
use generate::do_generate;

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
pub enum WorkerRequest {
    Completion(CompletionRequest),
    Generate(GenerateRequest),
}

pub fn run(last_worker_request: Arc<RwLock<Option<WorkerRequest>>>, connection: Arc<Connection>) {
    loop {
        let option_worker_request: Option<WorkerRequest> = {
            let completion_request = last_worker_request.read();
            (*completion_request).clone()
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
            };
            connection
                .sender
                .send(Message::Response(response))
                .expect("Error sending response");
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
}
