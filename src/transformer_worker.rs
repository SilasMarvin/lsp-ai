use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    Position, Range, TextEdit,
};
use parking_lot::Mutex;
use std::{sync::Arc, thread};
use tokio::sync::oneshot;

use crate::custom_requests::generate::{GenerateParams, GenerateResult};
use crate::custom_requests::generate_stream::GenerateStreamParams;
use crate::memory_backends::PromptForType;
use crate::memory_worker::{self, FilterRequest, PromptRequest};
use crate::transformer_backends::TransformerBackend;
use crate::utils::ToResponseError;

#[derive(Clone, Debug)]
pub struct CompletionRequest {
    id: RequestId,
    params: CompletionParams,
}

impl CompletionRequest {
    pub fn new(id: RequestId, params: CompletionParams) -> Self {
        Self { id, params }
    }
}

#[derive(Clone, Debug)]
pub struct GenerateRequest {
    id: RequestId,
    params: GenerateParams,
}

impl GenerateRequest {
    pub fn new(id: RequestId, params: GenerateParams) -> Self {
        Self { id, params }
    }
}

// The generate stream is not yet ready but we don't want to remove it
#[allow(dead_code)]
#[derive(Clone, Debug)]
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

pub fn run(
    transformer_backend: Box<dyn TransformerBackend + Send + Sync>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    last_worker_request: Arc<Mutex<Option<WorkerRequest>>>,
    connection: Arc<Connection>,
) -> anyhow::Result<()> {
    let transformer_backend = Arc::new(transformer_backend);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    loop {
        let option_worker_request: Option<WorkerRequest> = {
            let mut completion_request = last_worker_request.lock();
            std::mem::take(&mut *completion_request)
        };
        if let Some(request) = option_worker_request {
            let thread_connection = connection.clone();
            let thread_transformer_backend = transformer_backend.clone();
            let thread_memory_backend_tx = memory_backend_tx.clone();
            runtime.spawn(async move {
                let response = match request {
                    WorkerRequest::Completion(request) => match do_completion(
                        thread_transformer_backend,
                        thread_memory_backend_tx,
                        &request,
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => Response {
                            id: request.id,
                            result: None,
                            error: Some(e.to_response_error(-32603)),
                        },
                    },
                    WorkerRequest::Generate(request) => match do_generate(
                        thread_transformer_backend,
                        thread_memory_backend_tx,
                        &request,
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => Response {
                            id: request.id,
                            result: None,
                            error: Some(e.to_response_error(-32603)),
                        },
                    },
                    WorkerRequest::GenerateStream(_) => {
                        panic!("Streaming is not supported yet")
                    }
                };
                thread_connection
                    .sender
                    .send(Message::Response(response))
                    .expect("Error sending  message");
            });
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
}

async fn do_completion(
    transformer_backend: Arc<Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: &CompletionRequest,
) -> anyhow::Result<Response> {
    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
        request.params.text_document_position.clone(),
        PromptForType::Completion,
        tx,
    )))?;
    let prompt = rx.await?;

    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::FilterText(
        FilterRequest::new(request.params.text_document_position.clone(), tx),
    ))?;
    let filter_text = rx.await?;

    let response = transformer_backend.do_completion(&prompt).await?;
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
    let result = serde_json::to_value(result).unwrap();
    Ok(Response {
        id: request.id.clone(),
        result: Some(result),
        error: None,
    })
}

async fn do_generate(
    transformer_backend: Arc<Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: &GenerateRequest,
) -> anyhow::Result<Response> {
    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
        request.params.text_document_position.clone(),
        PromptForType::Completion,
        tx,
    )))?;
    let prompt = rx.await?;

    let response = transformer_backend.do_generate(&prompt).await?;
    let result = GenerateResult {
        generated_text: response.generated_text,
    };
    let result = serde_json::to_value(result).unwrap();
    Ok(Response {
        id: request.id.clone(),
        result: Some(result),
        error: None,
    })
}
