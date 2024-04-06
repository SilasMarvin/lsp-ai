use anyhow::Result;

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId};
use lsp_types::{
    request::Completion, CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    RenameFilesParams, ServerCapabilities, TextDocumentSyncKind,
};
use std::{
    sync::{mpsc, Arc},
    thread,
};
use tracing::error;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod configuration;
mod custom_requests;
mod memory_backends;
mod memory_worker;
mod template;
mod transformer_backends;
mod transformer_worker;
mod utils;

use configuration::Configuration;
use custom_requests::generate::Generate;
use memory_backends::MemoryBackend;
use transformer_backends::TransformerBackend;
use transformer_worker::{CompletionRequest, GenerateRequest, WorkerRequest};

use crate::{
    custom_requests::generate_stream::GenerateStream, transformer_worker::GenerateStreamRequest,
};

fn notification_is<N: lsp_types::notification::Notification>(notification: &Notification) -> bool {
    notification.method == N::METHOD
}

fn request_is<R: lsp_types::request::Request>(request: &Request) -> bool {
    request.method == R::METHOD
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

fn main() -> Result<()> {
    // Builds a tracing subscriber from the `LSP_AI_LOG` environment variable
    // If the variables value is malformed or missing, sets the default log level to ERROR
    FmtSubscriber::builder()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .with_env_filter(EnvFilter::from_env("LSP_AI_LOG"))
        .init();

    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(ServerCapabilities {
        completion_provider: Some(CompletionOptions::default()),
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        ..Default::default()
    })?;
    let initialization_args = connection.initialize(server_capabilities)?;

    main_loop(connection, initialization_args)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: Connection, args: serde_json::Value) -> Result<()> {
    // Build our configuration
    let config = Configuration::new(args)?;

    // Wrap the connection for sharing between threads
    let connection = Arc::new(connection);

    // Our channel we use to communicate with our transformer worker
    // let last_worker_request = Arc::new(Mutex::new(None));
    let (transformer_tx, transformer_rx) = mpsc::channel();

    // The channel we use to communicate with our memory worker
    let (memory_tx, memory_rx) = mpsc::channel();

    // Setup the transformer worker
    let memory_backend: Box<dyn MemoryBackend + Send + Sync> = config.clone().try_into()?;
    thread::spawn(move || memory_worker::run(memory_backend, memory_rx));

    // Setup our transformer worker
    let transformer_backend: Box<dyn TransformerBackend + Send + Sync> =
        config.clone().try_into()?;
    let thread_connection = connection.clone();
    let thread_memory_tx = memory_tx.clone();
    let thread_config = config.clone();
    thread::spawn(move || {
        transformer_worker::run(
            transformer_backend,
            thread_memory_tx,
            transformer_rx,
            thread_connection,
            thread_config,
        )
    });

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                if request_is::<Completion>(&req) {
                    match cast::<Completion>(req) {
                        Ok((id, params)) => {
                            let completion_request = CompletionRequest::new(id, params);
                            transformer_tx.send(WorkerRequest::Completion(completion_request))?;
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else if request_is::<Generate>(&req) {
                    match cast::<Generate>(req) {
                        Ok((id, params)) => {
                            let generate_request = GenerateRequest::new(id, params);
                            transformer_tx.send(WorkerRequest::Generate(generate_request))?;
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else if request_is::<GenerateStream>(&req) {
                    match cast::<GenerateStream>(req) {
                        Ok((id, params)) => {
                            let generate_stream_request = GenerateStreamRequest::new(id, params);
                            transformer_tx
                                .send(WorkerRequest::GenerateStream(generate_stream_request))?;
                        }
                        Err(err) => error!("{err:?}"),
                    }
                } else {
                    error!("lsp-ai currently only supports textDocument/completion, textDocument/generate and textDocument/generateStream")
                }
            }
            Message::Notification(not) => {
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(&not) {
                    let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidOpenTextDocument(params))?;
                } else if notification_is::<lsp_types::notification::DidChangeTextDocument>(&not) {
                    let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidChangeTextDocument(params))?;
                } else if notification_is::<lsp_types::notification::DidRenameFiles>(&not) {
                    let params: RenameFilesParams = serde_json::from_value(not.params)?;
                    memory_tx.send(memory_worker::WorkerRequest::DidRenameFiles(params))?;
                }
            }
            _ => (),
        }
    }
    Ok(())
}
