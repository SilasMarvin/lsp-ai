use std::sync::Arc;

use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};

use crate::memory_backends::{MemoryBackend, Prompt, PromptForType};

#[derive(Debug)]
pub struct PromptRequest {
    position: TextDocumentPositionParams,
    prompt_for_type: PromptForType,
    tx: tokio::sync::oneshot::Sender<Prompt>,
}

impl PromptRequest {
    pub fn new(
        position: TextDocumentPositionParams,
        prompt_for_type: PromptForType,
        tx: tokio::sync::oneshot::Sender<Prompt>,
    ) -> Self {
        Self {
            position,
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

pub fn run(
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
            match request {
                WorkerRequest::FilterText(params) => {
                    let filter_text = thread_memory_backend
                        .get_filter_text(&params.position)
                        .await
                        .unwrap();
                    params.tx.send(filter_text).unwrap();
                }
                WorkerRequest::Prompt(params) => {
                    let prompt = thread_memory_backend
                        .build_prompt(&params.position, params.prompt_for_type)
                        .await
                        .unwrap();
                    params.tx.send(prompt).unwrap();
                }
                WorkerRequest::DidOpenTextDocument(params) => {
                    thread_memory_backend
                        .opened_text_document(params)
                        .await
                        .unwrap();
                }
                WorkerRequest::DidChangeTextDocument(params) => {
                    thread_memory_backend
                        .changed_text_document(params)
                        .await
                        .unwrap();
                }
                WorkerRequest::DidRenameFiles(params) => {
                    thread_memory_backend.renamed_file(params).await.unwrap()
                }
            }
        });
    }
}
