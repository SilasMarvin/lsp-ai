use std::sync::Arc;

use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Range, RenameFilesParams,
    TextDocumentIdentifier, TextDocumentPositionParams,
};
use serde_json::Value;
use tracing::error;

use crate::{
    memory_backends::{MemoryBackend, Prompt, PromptType},
    utils::TOKIO_RUNTIME,
};

#[derive(Debug)]
pub(crate) struct PromptRequest {
    position: TextDocumentPositionParams,
    prompt_type: PromptType,
    params: Value,
    tx: tokio::sync::oneshot::Sender<Prompt>,
}

impl PromptRequest {
    pub(crate) fn new(
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
pub(crate) struct FilterRequest {
    position: TextDocumentPositionParams,
    tx: tokio::sync::oneshot::Sender<String>,
}

impl FilterRequest {
    pub(crate) fn new(
        position: TextDocumentPositionParams,
        tx: tokio::sync::oneshot::Sender<String>,
    ) -> Self {
        Self { position, tx }
    }
}

#[derive(Debug)]
pub(crate) struct CodeActionRequest {
    text_document_identifier: TextDocumentIdentifier,
    range: Range,
    trigger: String,
    tx: tokio::sync::oneshot::Sender<bool>,
}

impl CodeActionRequest {
    pub(crate) fn new(
        text_document_identifier: TextDocumentIdentifier,
        range: Range,
        trigger: String,
        tx: tokio::sync::oneshot::Sender<bool>,
    ) -> Self {
        Self {
            text_document_identifier,
            range,
            trigger,
            tx,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FileRequest {
    text_document_identifier: TextDocumentIdentifier,
    tx: tokio::sync::oneshot::Sender<String>,
}

impl FileRequest {
    pub(crate) fn new(
        text_document_identifier: TextDocumentIdentifier,
        tx: tokio::sync::oneshot::Sender<String>,
    ) -> Self {
        Self {
            text_document_identifier,
            tx,
        }
    }
}

pub(crate) enum WorkerRequest {
    Shutdown,
    FilterText(FilterRequest),
    File(FileRequest),
    Prompt(PromptRequest),
    CodeActionRequest(CodeActionRequest),
    DidOpenTextDocument(DidOpenTextDocumentParams),
    DidChangeTextDocument(DidChangeTextDocumentParams),
    DidRenameFiles(RenameFilesParams),
}

async fn do_build_prompt(
    params: PromptRequest,
    memory_backend: Arc<Box<dyn MemoryBackend + Send + Sync>>,
) -> anyhow::Result<()> {
    let prompt = memory_backend
        .build_prompt(&params.position, params.prompt_type, &params.params)
        .await?;
    params
        .tx
        .send(prompt)
        .map_err(|_| anyhow::anyhow!("sending on channel failed"))
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
        WorkerRequest::CodeActionRequest(params) => {
            let res = memory_backend.code_action_request(
                &params.text_document_identifier,
                &params.range,
                &params.trigger,
            )?;
            params
                .tx
                .send(res)
                .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
        }
        WorkerRequest::File(params) => {
            let res = memory_backend.file_request(&params.text_document_identifier)?;
            params
                .tx
                .send(res)
                .map_err(|_| anyhow::anyhow!("sending on channel failed"))?;
        }
        WorkerRequest::DidOpenTextDocument(params) => {
            memory_backend.opened_text_document(params)?;
        }
        WorkerRequest::DidChangeTextDocument(params) => {
            memory_backend.changed_text_document(params)?;
        }
        WorkerRequest::DidRenameFiles(params) => memory_backend.renamed_files(params)?,
        WorkerRequest::Shutdown => unreachable!(),
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
        match &request {
            WorkerRequest::Shutdown => {
                return Ok(());
            }
            _ => {
                if let Err(e) = do_task(request, memory_backend.clone()) {
                    error!("error in memory worker task: {e}")
                }
            }
        }
    }
}

pub(crate) fn run(
    memory_backend: Box<dyn MemoryBackend + Send + Sync>,
    rx: std::sync::mpsc::Receiver<WorkerRequest>,
) {
    if let Err(e) = do_run(memory_backend, rx) {
        error!("error in memory worker: {e}")
    }
}
