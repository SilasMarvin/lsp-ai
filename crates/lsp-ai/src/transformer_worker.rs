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

use crate::config::{self, Config};
use crate::custom_requests::generation::{GenerateResult, GenerationParams};
use crate::custom_requests::generation_stream::GenerationStreamParams;
use crate::memory_backends::Prompt;
use crate::memory_worker::{self, FilterRequest, PromptRequest};
use crate::transformer_backends::TransformerBackend;
use crate::utils::{ToResponseError, TOKIO_RUNTIME};

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

impl WorkerRequest {
    fn get_id(&self) -> RequestId {
        match self {
            WorkerRequest::Completion(r) => r.id.clone(),
            WorkerRequest::Generation(r) => r.id.clone(),
            WorkerRequest::GenerationStream(r) => r.id.clone(),
        }
    }
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

fn post_process_start(response: String, front: &str) -> String {
    let mut front_match = response.len();
    loop {
        if response.len() == 0 || front.ends_with(&response[..front_match]) {
            break;
        } else {
            front_match -= 1;
        }
    }
    if front_match > 0 {
        (&response[front_match..]).to_owned()
    } else {
        response
    }
}

fn post_process_end(response: String, back: &str) -> String {
    let mut back_match = 0;
    loop {
        if back_match == response.len() {
            break;
        } else if back.starts_with(&response[back_match..]) {
            break;
        } else {
            back_match += 1;
        }
    }
    if back_match > 0 {
        (&response[..back_match]).to_owned()
    } else {
        response
    }
}

// Some basic post processing that will clean up duplicate characters at the front and back
fn post_process_response(
    response: String,
    prompt: &Prompt,
    config: &config::PostProcess,
) -> String {
    match prompt {
        Prompt::ContextAndCode(context_and_code) => {
            if context_and_code.code.contains("<CURSOR>") {
                let mut split = context_and_code.code.split("<CURSOR>");
                let response = if config.remove_duplicate_start {
                    post_process_start(response, split.next().unwrap())
                } else {
                    response
                };
                if config.remove_duplicate_end {
                    post_process_end(response, split.next().unwrap())
                } else {
                    response
                }
            } else {
                if config.remove_duplicate_start {
                    post_process_start(response, &context_and_code.code)
                } else {
                    response
                }
            }
        }
        Prompt::FIM(fim) => {
            let response = if config.remove_duplicate_start {
                post_process_start(response, &fim.prompt)
            } else {
                response
            };
            if config.remove_duplicate_end {
                post_process_end(response, &fim.suffix)
            } else {
                response
            }
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

fn do_run(
    transformer_backends: HashMap<String, Box<dyn TransformerBackend + Send + Sync>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    transformer_rx: std::sync::mpsc::Receiver<WorkerRequest>,
    connection: Arc<Connection>,
    config: Config,
) -> anyhow::Result<()> {
    let transformer_backends = Arc::new(transformer_backends);

    // If they have disabled completions, this function will fail. We set it to MIN_POSITIVE to never process a completions request
    let max_requests_per_second = config
        .get_completion_transformer_max_requests_per_second()
        .unwrap_or(f32::MIN_POSITIVE);
    let mut last_completion_request_time = SystemTime::now();
    let mut last_completion_request = None;

    let run_dispatch_request = |request| {
        let task_connection = connection.clone();
        let task_transformer_backends = transformer_backends.clone();
        let task_memory_backend_tx = memory_backend_tx.clone();
        let task_config = config.clone();
        TOKIO_RUNTIME.spawn(async move {
            dispatch_request(
                request,
                task_connection,
                task_transformer_backends,
                task_memory_backend_tx,
                task_config,
            )
            .await;
        });
    };

    loop {
        // We want to rate limit completions without dropping the last rate limited request
        let request = transformer_rx.recv_timeout(Duration::from_millis(5));

        match request {
            Ok(request) => match request {
                WorkerRequest::Completion(_) => last_completion_request = Some(request),
                _ => run_dispatch_request(request),
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
            run_dispatch_request(request);
        }
    }
}

#[instrument(skip(connection, transformer_backends, memory_backend_tx, config))]
async fn dispatch_request(
    request: WorkerRequest,
    connection: Arc<Connection>,
    transformer_backends: Arc<HashMap<String, Box<dyn TransformerBackend + Send + Sync>>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    config: Config,
) {
    let response = match generate_response(
        request.clone(),
        transformer_backends,
        memory_backend_tx,
        config,
    )
    .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("generating response: {e}");
            Response {
                id: request.get_id(),
                result: None,
                error: Some(e.to_response_error(-32603)),
            }
        }
    };

    if let Err(e) = connection.sender.send(Message::Response(response)) {
        error!("sending response: {e}");
    }
}

async fn generate_response(
    request: WorkerRequest,
    transformer_backends: Arc<HashMap<String, Box<dyn TransformerBackend + Send + Sync>>>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    config: Config,
) -> anyhow::Result<Response> {
    match request {
        WorkerRequest::Completion(request) => {
            let completion_config = config
                .config
                .completion
                .as_ref()
                .context("Completions is none")?;
            let transformer_backend = transformer_backends
                .get(&completion_config.model)
                .clone()
                .with_context(|| format!("can't find model: {}", &completion_config.model))?;
            do_completion(transformer_backend, memory_backend_tx, &request, &config).await
        }
        WorkerRequest::Generation(request) => {
            let transformer_backend = transformer_backends
                .get(&request.params.model)
                .clone()
                .with_context(|| format!("can't find model: {}", &request.params.model))?;
            do_generate(transformer_backend, memory_backend_tx, &request).await
        }
        WorkerRequest::GenerationStream(_) => {
            anyhow::bail!("Streaming is not yet supported")
        }
    }
}

async fn do_completion(
    transformer_backend: &Box<dyn TransformerBackend + Send + Sync>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: &CompletionRequest,
    config: &Config,
) -> anyhow::Result<Response> {
    let params = serde_json::to_value(
        config
            .config
            .completion
            .as_ref()
            .context("Completions is None")?
            .parameters
            .clone(),
    )
    .unwrap();

    // Build the prompt
    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
        request.params.text_document_position.clone(),
        transformer_backend.get_prompt_type(&params)?,
        params.clone(),
        tx,
    )))?;
    let prompt = rx.await?;

    // Get the filter text
    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::FilterText(
        FilterRequest::new(request.params.text_document_position.clone(), tx),
    ))?;
    let filter_text = rx.await?;

    // Get the response
    let mut response = transformer_backend.do_completion(&prompt, params).await?;

    if let Some(post_process) = config.get_completions_post_process() {
        response.insert_text = post_process_response(response.insert_text, &prompt, &post_process);
    }

    // Build and send the response
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
    transformer_backend: &Box<dyn TransformerBackend + Send + Sync>,
    memory_backend_tx: std::sync::mpsc::Sender<memory_worker::WorkerRequest>,
    request: &GenerationRequest,
) -> anyhow::Result<Response> {
    let params = serde_json::to_value(request.params.parameters.clone()).unwrap();

    let (tx, rx) = oneshot::channel();
    memory_backend_tx.send(memory_worker::WorkerRequest::Prompt(PromptRequest::new(
        request.params.text_document_position.clone(),
        transformer_backend.get_prompt_type(&params)?,
        params.clone(),
        tx,
    )))?;
    let prompt = rx.await?;

    let mut response = transformer_backend.do_generate(&prompt, params).await?;
    response.generated_text = post_process_response(
        response.generated_text,
        &prompt,
        &request.params.post_process,
    );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_backends::{ContextAndCodePrompt, FIMPrompt};

    #[test]
    fn test_post_process_fim() {
        let config = config::PostProcess::default();

        let prompt = Prompt::FIM(FIMPrompt {
            prompt: "test 1234 ".to_string(),
            suffix: "ttabc".to_string(),
        });
        let response = "4 zz tta".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "zz ");

        let prompt = Prompt::FIM(FIMPrompt {
            prompt: "test".to_string(),
            suffix: "test".to_string(),
        });
        let response = "zzzz".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "zzzz");
    }

    #[test]
    fn test_post_process_context_and_code() {
        let config = config::PostProcess::default();

        let prompt = Prompt::ContextAndCode(ContextAndCodePrompt {
            context: "".to_string(),
            code: "tt ".to_string(),
        });
        let response = "tt abc".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "abc");

        let prompt = Prompt::ContextAndCode(ContextAndCodePrompt {
            context: "".to_string(),
            code: "ff".to_string(),
        });
        let response = "zz".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "zz");

        let prompt = Prompt::ContextAndCode(ContextAndCodePrompt {
            context: "".to_string(),
            code: "tt <CURSOR> tt".to_string(),
        });
        let response = "tt abc tt".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "abc");

        let prompt = Prompt::ContextAndCode(ContextAndCodePrompt {
            context: "".to_string(),
            code: "d<CURSOR>d".to_string(),
        });
        let response = "zz".to_string();
        let new_response = post_process_response(response.clone(), &prompt, &config);
        assert_eq!(new_response, "zz");
    }
}
