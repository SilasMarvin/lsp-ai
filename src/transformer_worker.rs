use anyhow::Context;
use lsp_server::{Connection, Message, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    Position, Range, TextEdit,
};
use std::collections::HashMap;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;
use tracing::{error, instrument};

use crate::config::Config;
use crate::custom_requests::generation::{GenerateResult, GenerationParams};
use crate::custom_requests::generation_stream::GenerationStreamParams;
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
pub struct GenerationRequest {
    id: RequestId,
    params: GenerationParams,
}

impl GenerationRequest {
    pub fn new(id: RequestId, params: GenerationParams) -> Self {
        Self { id, params }
    }
}

// The generate stream is not yet ready but we don't want to remove it
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GenerationStreamRequest {
    id: RequestId,
    params: GenerationStreamParams,
}

impl GenerationStreamRequest {
    pub fn new(id: RequestId, params: GenerationStreamParams) -> Self {
        Self { id, params }
    }
}

#[derive(Clone, Debug)]
pub enum WorkerRequest {
    Completion(CompletionRequest),
    Generation(GenerationRequest),
    GenerationStream(GenerationStreamRequest),
}

pub struct DoCompletionResponse {
    pub insert_text: String,
}

pub struct DoGenerationResponse {
    pub generated_text: String,
}

pub struct DoGenerationStreamResponse {
    pub generated_text: String,
}

#[instrument(skip(config, transformer_backend, memory_backend_tx, connection))]
async fn do_task(
    transformer_backend: Arc<Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: WorkerRequest,
    connection: Arc<Connection>,
    config: Config,
) -> anyhow::Result<()> {
    let response = match request {
        WorkerRequest::Completion(request) => {
            match do_completion(transformer_backend, memory_backend_tx, &request, &config).await {
                Ok(r) => r,
                Err(e) => Response {
                    id: request.id,
                    result: None,
                    error: Some(e.to_response_error(-32603)),
                },
            }
        }
        WorkerRequest::Generation(request) => {
            match do_generate(transformer_backend, memory_backend_tx, &request).await {
                Ok(r) => r,
                Err(e) => Response {
                    id: request.id,
                    result: None,
                    error: Some(e.to_response_error(-32603)),
                },
            }
        }
        WorkerRequest::GenerationStream(_) => {
            panic!("Streaming is not yet supported")
        }
    };
    connection
        .sender
        .send(Message::Response(response))
        .expect("Error sending response");
    Ok(())
}

fn do_run(
    transformer_backends: HashMap<String, Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    transformer_rx: std::sync::mpsc::Receiver<WorkerRequest>,
    connection: Arc<Connection>,
    config: Config,
) -> anyhow::Result<()> {
    let transformer_backends: HashMap<String, Arc<Box<dyn TransformerBackend + Send + Sync>>> =
        transformer_backends
            .into_iter()
            .map(|(key, backend)| (key, Arc::new(backend)))
            .collect();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;

    let max_requests_per_second = config.get_completion_transformer_max_requests_per_second();
    let mut last_completion_request_time = SystemTime::now();
    let mut last_completion_request = None;

    let dispatch_request = move |request| {
        let thread_transformer_backend = transformer_backends
            .get(&config.config.completion.model)
            .clone()
            .with_context(|| format!("can't find model: {}", config.config.completion.model))?
            .clone();
        let thread_config = config.clone();
        let thread_memory_backend_tx = memory_backend_tx.clone();
        let thread_connection = connection.clone();
        runtime.spawn(async move {
            if let Err(e) = do_task(
                thread_transformer_backend,
                thread_memory_backend_tx,
                request,
                thread_connection,
                thread_config,
            )
            .await
            {
                error!("transformer worker task: {e}")
            }
        });
        anyhow::Ok(())
    };

    loop {
        // We want to rate limit completions without dropping the last rate limited request
        let request = transformer_rx.recv_timeout(Duration::from_millis(5));

        match request {
            Ok(request) => match request {
                WorkerRequest::Completion(_) => last_completion_request = Some(request),
                _ => {
                    dispatch_request(request)?;
                }
            },
            Err(RecvTimeoutError::Disconnected) => anyhow::bail!("channel disconnected"),
            _ => {}
        }

        if SystemTime::now()
            .duration_since(last_completion_request_time)?
            .as_secs_f32()
            < 1. / max_requests_per_second
        {
            continue;
        }

        if let Some(request) = last_completion_request.take() {
            last_completion_request_time = SystemTime::now();
            dispatch_request(request)?;
        }
    }
}

pub fn run(
    transformer_backends: HashMap<String, Box<dyn TransformerBackend + Send + Sync>>,
    memory_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    transformer_rx: std::sync::mpsc::Receiver<WorkerRequest>,
    connection: Arc<Connection>,
    config: Config,
) {
    if let Err(e) = do_run(
        transformer_backends,
        memory_tx,
        transformer_rx,
        connection,
        config,
    ) {
        error!("error in transformer worker: {e}")
    }
}

async fn do_completion(
    transformer_backend: Arc<Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: &CompletionRequest,
    config: &Config,
) -> anyhow::Result<Response> {
    // TODO: Fix this
    // we need to be subtracting the completion / generation tokens from max_context_length
    // not sure if we should be doing that for the chat maybe leave a note here for that?

    let params = serde_json::to_value(config.config.completion.kwargs.clone()).unwrap();

    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
        request.params.text_document_position.clone(),
        params.clone(),
        tx,
    )))?;
    let prompt = rx.await?;

    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::FilterText(
        FilterRequest::new(request.params.text_document_position.clone(), tx),
    ))?;
    let filter_text = rx.await?;

    let response = transformer_backend.do_completion(&prompt, params).await?;
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
    request: &GenerationRequest,
) -> anyhow::Result<Response> {
    todo!()
    // let (tx, rx) = oneshot::channel();
    // memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
    //     request.params.text_document_position.clone(),
    //     PromptForType::Completion,
    //     tx,
    // )))?;
    // let prompt = rx.await?;

    // let response = transformer_backend.do_generate(&prompt).await?;
    // let result = GenerateResult {
    //     generated_text: response.generated_text,
    // };
    // let result = serde_json::to_value(result).unwrap();
    // Ok(Response {
    //     id: request.id.clone(),
    //     result: Some(result),
    //     error: None,
    // })
}
