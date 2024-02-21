use anyhow::Result;

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId};
use lsp_types::{
    request::Completion, CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    RenameFilesParams, ServerCapabilities, TextDocumentSyncKind,
};
use parking_lot::Mutex;
use std::{sync::Arc, thread};

mod configuration;
mod custom_requests;
mod memory_backends;
mod transformer_backends;
mod utils;
mod worker;

use configuration::Configuration;
use custom_requests::generate::Generate;
use memory_backends::MemoryBackend;
use transformer_backends::TransformerBackend;
use worker::{CompletionRequest, GenerateRequest, Worker, WorkerRequest};

use crate::{custom_requests::generate_stream::GenerateStream, worker::GenerateStreamRequest};

// Taken directly from: https://github.com/rust-lang/rust-analyzer
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
    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(&ServerCapabilities {
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

// This main loop is tricky
// We create a worker thread that actually does the heavy lifting because we do not want to process every completion request we get
// Completion requests may take a few seconds given the model configuration and hardware allowed, and we only want to process the latest completion request
// Note that we also want to have the memory backend in the worker thread as that may also involve heavy computations
fn main_loop(connection: Connection, args: serde_json::Value) -> Result<()> {
    let args = Configuration::new(args)?;

    // Set the transformer_backend
    let transformer_backend: Box<dyn TransformerBackend + Send> = args.clone().try_into()?;

    // Set the memory_backend
    let memory_backend: Arc<Mutex<Box<dyn MemoryBackend + Send>>> =
        Arc::new(Mutex::new(args.clone().try_into()?));

    // Wrap the connection for sharing between threads
    let connection = Arc::new(connection);

    // How we communicate between the worker and receiver threads
    let last_worker_request = Arc::new(Mutex::new(None));

    // Thread local variables
    let thread_memory_backend = memory_backend.clone();
    let thread_last_worker_request = last_worker_request.clone();
    let thread_connection = connection.clone();
    // TODO: Pass some backend into here
    thread::spawn(move || {
        Worker::new(
            transformer_backend,
            thread_memory_backend,
            thread_last_worker_request,
            thread_connection,
        )
        .run();
    });

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                // Right now each if / else basically does the same thing,
                // but this may change soon so it is worth making it a little
                // more verbose than it needs to be now
                if request_is::<Completion>(&req) {
                    match cast::<Completion>(req) {
                        Ok((id, params)) => {
                            eprintln!("******{:?}********", id);
                            let mut lcr = last_worker_request.lock();
                            let completion_request = CompletionRequest::new(id, params);
                            *lcr = Some(WorkerRequest::Completion(completion_request));
                        }
                        Err(err) => eprintln!("{err:?}"),
                    }
                } else if request_is::<Generate>(&req) {
                    match cast::<Generate>(req) {
                        Ok((id, params)) => {
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateRequest::new(id, params);
                            *lcr = Some(WorkerRequest::Generate(completion_request));
                        }
                        Err(err) => eprintln!("{err:?}"),
                    }
                } else if request_is::<GenerateStream>(&req) {
                    match cast::<GenerateStream>(req) {
                        Ok((id, params)) => {
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateStreamRequest::new(id, params);
                            *lcr = Some(WorkerRequest::GenerateStream(completion_request));
                        }
                        Err(err) => eprintln!("{err:?}"),
                    }
                } else {
                    eprintln!("lsp-ai currently only supports textDocument/completion, textDocument/generate and textDocument/generateStream")
                }
            }
            Message::Notification(not) => {
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(&not) {
                    let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_backend.lock().opened_text_document(params)?;
                } else if notification_is::<lsp_types::notification::DidChangeTextDocument>(&not) {
                    let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    memory_backend.lock().changed_text_document(params)?;
                } else if notification_is::<lsp_types::notification::DidRenameFiles>(&not) {
                    let params: RenameFilesParams = serde_json::from_value(not.params)?;
                    memory_backend.lock().renamed_file(params)?;
                }
            }
            _ => (),
        }
    }
    Ok(())
}
