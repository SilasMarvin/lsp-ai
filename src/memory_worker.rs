use std::sync::Arc;

use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};
use serde_json::Value;
use tracing::error;

use crate::{
    memory_backends::{MemoryBackend, Prompt, PromptType},
    utils::TOKIO_RUNTIME,
};

#[derive(Debug)]
pub struct PromptRequest {
    position: TextDocumentPositionParams,
    prompt_type: PromptType,
    params: Value,
    tx: tokio::sync::oneshot::Sender<Prompt>,
}

impl PromptRequest {
    pub fn new(
        position: TextDocumentPositionParams,
        prompt_type: PromptType,
        params: Value,
        tx: tokio::sync::oneshot::Sender<Prompt>,
    ) -> Self {
        Self {
            position,
            prompt_type,
            params,
            tx,
        }
    }
}

#[derive(Debug)]
pub struct FilterRequest {
    position: TextDocumentPositionParams,
    tx: tokio::sync::oneshot::Sender<String>,
}

impl FilterRequest {
    pub fn new(
        position: TextDocumentPositionParams,
        tx: tokio::sync::oneshot::Sender<String>,
    ) -> Self {
        Self { position, tx }
    }
}

pub enum WorkerRequest {
    FilterText(FilterRequest),
    Prompt(PromptRequest),
    DidOpenTextDocument(DidOpenTextDocumentParams),
    DidChangeTextDocument(DidChangeTextDocumentParams),
    DidRenameFiles(RenameFilesParams),
}

async fn do_build_prompt(
    params: PromptRequest,
    memory_backend: Arc<Box<dyn MemoryBackend + Send + Sync>>,
) -> anyhow::Result<()> {
    let prompt = memory_backend
        .build_prompt(&params.position, params.prompt_type, params.params)
        .await?;
    params
        .tx
        .send(prompt)
        .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
    Ok(())
}

fn do_task(
    request: WorkerRequest,
    memory_backend: Arc<Box<dyn MemoryBackend + Send + Sync>>,
) -> anyhow::Result<()> {
    match request {
        WorkerRequest::FilterText(params) => {
            let filter_text = memory_backend.get_filter_text(&params.position)?;
            params
                .tx
                .send(filter_text)
                .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
        }
        WorkerRequest::Prompt(params) => {
            TOKIO_RUNTIME.spawn(async move {
                if let Err(e) = do_build_prompt(params, memory_backend).await {
                    error!("error in memory worker building prompt: {e}")
                }
            });
        }
        WorkerRequest::DidOpenTextDocument(params) => {
            memory_backend.opened_text_document(params)?;
        }
        WorkerRequest::DidChangeTextDocument(params) => {
            memory_backend.changed_text_document(params)?;
        }
        WorkerRequest::DidRenameFiles(params) => memory_backend.renamed_files(params)?,
    }
    anyhow::Ok(())
}

fn do_run(
    memory_backend: Box<dyn MemoryBackend + Send + Sync>,
    rx: std::sync::mpsc::Receiver<WorkerRequest>,
) -> anyhow::Result<()> {
    let memory_backend = Arc::new(memory_backend);
    loop {
        let request = rx.recv()?;
        if let Err(e) = do_task(request, memory_backend.clone()) {
            error!("error in memory worker task: {e}")
        }
    }
}

pub fn run(
    memory_backend: Box<dyn MemoryBackend + Send + Sync>,
    rx: std::sync::mpsc::Receiver<WorkerRequest>,
) {
    if let Err(e) = do_run(memory_backend, rx) {
        error!("error in memory worker: {e}")
    }
}
