use std::sync::Arc;

use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};
use tracing::error;

use crate::memory_backends::{MemoryBackend, Prompt, PromptForType};

#[derive(Debug)]
pub struct PromptRequest {
    position: TextDocumentPositionParams,
    max_context_length: usize,
    prompt_for_type: PromptForType,
    tx: tokio::sync::oneshot::Sender<Prompt>,
}

impl PromptRequest {
    pub fn new(
        position: TextDocumentPositionParams,
        max_context_length: usize,
        prompt_for_type: PromptForType,
        tx: tokio::sync::oneshot::Sender<Prompt>,
    ) -> Self {
        Self {
            position,
            max_context_length,
            prompt_for_type,
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

async fn do_task(
    request: WorkerRequest,
    memory_backend: Arc<Box<dyn MemoryBackend + Send + Sync>>,
) -> anyhow::Result<()> {
    match request {
        WorkerRequest::FilterText(params) => {
            let filter_text = memory_backend.get_filter_text(&params.position).await?;
            params
                .tx
                .send(filter_text)
                .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
        }
        WorkerRequest::Prompt(params) => {
            let prompt = memory_backend
                .build_prompt(
                    &params.position,
                    params.max_context_length,
                    params.prompt_for_type,
                )
                .await?;
            params
                .tx
                .send(prompt)
                .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
        }
        WorkerRequest::DidOpenTextDocument(params) => {
            memory_backend.opened_text_document(params).await?;
        }
        WorkerRequest::DidChangeTextDocument(params) => {
            memory_backend.changed_text_document(params).await?;
        }
        WorkerRequest::DidRenameFiles(params) => memory_backend.renamed_file(params).await?,
    }
    anyhow::Ok(())
}

fn do_run(
    memory_backend: Box<dyn MemoryBackend + Send + Sync>,
    rx: std::sync::mpsc::Receiver<WorkerRequest>,
) -> anyhow::Result<()> {
    let memory_backend = Arc::new(memory_backend);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    loop {
        let request = rx.recv()?;
        let thread_memory_backend = memory_backend.clone();
        runtime.spawn(async move {
            if let Err(e) = do_task(request, thread_memory_backend).await {
                error!("error in memory worker task: {e}")
            }
        });
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
