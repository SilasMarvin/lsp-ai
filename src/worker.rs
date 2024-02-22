use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    Position, Range, TextEdit,
};
use parking_lot::Mutex;
use std::{sync::Arc, thread};

use crate::custom_requests::generate::{GenerateParams, GenerateResult};
use crate::custom_requests::generate_stream::GenerateStreamParams;
use crate::memory_backends::MemoryBackend;
use crate::transformer_backends::TransformerBackend;
use crate::utils::ToResponseError;

#[derive(Clone)]
pub struct CompletionRequest {
    id: RequestId,
    params: CompletionParams,
}

impl CompletionRequest {
    pub fn new(id: RequestId, params: CompletionParams) -> Self {
        Self { id, params }
    }
}

#[derive(Clone)]
pub struct GenerateRequest {
    id: RequestId,
    params: GenerateParams,
}

impl GenerateRequest {
    pub fn new(id: RequestId, params: GenerateParams) -> Self {
        Self { id, params }
    }
}

#[derive(Clone)]
pub struct GenerateStreamRequest {
    id: RequestId,
    params: GenerateStreamParams,
}

impl GenerateStreamRequest {
    pub fn new(id: RequestId, params: GenerateStreamParams) -> Self {
        Self { id, params }
    }
}

#[derive(Clone)]
pub enum WorkerRequest {
    Completion(CompletionRequest),
    Generate(GenerateRequest),
    GenerateStream(GenerateStreamRequest),
}

pub struct DoCompletionResponse {
    pub insert_text: String,
}

pub struct DoGenerateResponse {
    pub generated_text: String,
}

pub struct DoGenerateStreamResponse {
    pub generated_text: String,
}

pub struct Worker {
    transformer_backend: Box<dyn TransformerBackend>,
    memory_backend: Arc<Mutex<Box<dyn MemoryBackend + Send>>>,
    last_worker_request: Arc<Mutex<Option<WorkerRequest>>>,
    connection: Arc<Connection>,
}

impl Worker {
    pub fn new(
        transformer_backend: Box<dyn TransformerBackend>,
        memory_backend: Arc<Mutex<Box<dyn MemoryBackend + Send>>>,
        last_worker_request: Arc<Mutex<Option<WorkerRequest>>>,
        connection: Arc<Connection>,
    ) -> Self {
        Self {
            transformer_backend,
            memory_backend,
            last_worker_request,
            connection,
        }
    }

    fn do_completion(&self, request: &CompletionRequest) -> anyhow::Result<Response> {
        let prompt = self
            .memory_backend
            .lock()
            .build_prompt(&request.params.text_document_position)?;
        let filter_text = self
            .memory_backend
            .lock()
            .get_filter_text(&request.params.text_document_position)?;
        eprintln!("\nPROMPT**************\n{}\n******************\n", prompt);
        let response = self.transformer_backend.do_completion(&prompt)?;
        eprintln!(
            "\nINSERT TEXT&&&&&&&&&&&&&&&&&&&\n{}\n&&&&&&&&&&&&&&&&&&\n",
            response.insert_text
        );
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
            filter_text: Some(filter_text),
            text_edit: Some(lsp_types::CompletionTextEdit::Edit(completion_text_edit)),
            kind: Some(CompletionItemKind::TEXT),
            ..Default::default()
        };
        let completion_list = CompletionList {
            is_incomplete: false,
            items: vec![item],
        };
        let result = Some(CompletionResponse::List(completion_list));
        let result = serde_json::to_value(&result).unwrap();
        Ok(Response {
            id: request.id.clone(),
            result: Some(result),
            error: None,
        })
    }

    fn do_generate(&self, request: &GenerateRequest) -> anyhow::Result<Response> {
        let prompt = self
            .memory_backend
            .lock()
            .build_prompt(&request.params.text_document_position)?;
        eprintln!("\n\n****************{}***************\n\n", prompt);
        let response = self.transformer_backend.do_generate(&prompt)?;
        let result = GenerateResult {
            generated_text: response.generated_text,
        };
        let result = serde_json::to_value(&result).unwrap();
        Ok(Response {
            id: request.id.clone(),
            result: Some(result),
            error: None,
        })
    }

    pub fn run(self) {
        loop {
            let option_worker_request: Option<WorkerRequest> = {
                let mut completion_request = self.last_worker_request.lock();
                std::mem::take(&mut *completion_request)
            };
            if let Some(request) = option_worker_request {
                let response = match request {
                    WorkerRequest::Completion(request) => match self.do_completion(&request) {
                        Ok(r) => r,
                        Err(e) => Response {
                            id: request.id,
                            result: None,
                            error: Some(e.to_response_error(-32603)),
                        },
                    },
                    WorkerRequest::Generate(request) => match self.do_generate(&request) {
                        Ok(r) => r,
                        Err(e) => Response {
                            id: request.id,
                            result: None,
                            error: Some(e.to_response_error(-32603)),
                        },
                    },
                    WorkerRequest::GenerateStream(_) => panic!("Streaming is not supported yet"),
                };
                self.connection
                    .sender
                    .send(Message::Response(response))
                    .expect("Error sending  message");
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
