use anyhow::{Context, Result};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId};
use lsp_types::{
    request::Completion, CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    RenameFilesParams, ServerCapabilities, TextDocumentSyncKind,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use pyo3::prelude::*;
use ropey::Rope;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc, thread};

mod custom_requests;
mod worker;

use custom_requests::generate::Generate;
use worker::{CompletionRequest, GenerateRequest, WorkerRequest};

use crate::{custom_requests::generate_stream::GenerateStream, worker::GenerateStreamRequest};

pub static PY_MODULE: Lazy<Result<Py<PyAny>>> = Lazy::new(|| {
    pyo3::Python::with_gil(|py| -> Result<Py<PyAny>> {
        let src = include_str!("python/transformers.py");
        Ok(pyo3::types::PyModule::from_code(py, src, "transformers.py", "transformers")?.into())
    })
});

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
    let initialization_params = connection.initialize(server_capabilities)?;

    // Activate the python venv
    Python::with_gil(|py| -> Result<()> {
        let activate: Py<PyAny> = PY_MODULE
            .as_ref()
            .map_err(anyhow::Error::msg)?
            .getattr(py, "activate_venv")?;

        activate.call1(py, ("/Users/silas/Projects/lsp-ai/venv",))?;
        Ok(())
    })?;

    main_loop(connection, initialization_params)?;
    io_threads.join()?;
    Ok(())
}

#[derive(Deserialize)]
struct Params {}

// This main loop is tricky
// We create a worker thread that actually does the heavy lifting because we do not want to process every completion request we get
// Completion requests may take a few seconds given the model configuration and hardware allowed, and we only want to process the latest completion request
fn main_loop(connection: Connection, params: serde_json::Value) -> Result<()> {
    let _params: Params = serde_json::from_value(params)?;

    // Set the model
    Python::with_gil(|py| -> Result<()> {
        let activate: Py<PyAny> = PY_MODULE
            .as_ref()
            .map_err(anyhow::Error::msg)?
            .getattr(py, "set_model")?;
        activate.call1(py, ("",))?;
        Ok(())
    })?;

    // Prep variables
    let connection = Arc::new(connection);
    let mut file_map: HashMap<String, Rope> = HashMap::new();

    // How we communicate between the worker and receiver threads
    let last_worker_request = Arc::new(Mutex::new(None));

    // Thread local variables
    let thread_last_worker_request = last_worker_request.clone();
    let thread_connection = connection.clone();
    thread::spawn(move || {
        worker::run(thread_last_worker_request, thread_connection);
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
                            let rope = file_map
                                .get(params.text_document_position.text_document.uri.as_str())
                                .context("Error file not found")?
                                .clone();
                            let mut lcr = last_worker_request.lock();
                            let completion_request = CompletionRequest::new(id, params, rope);
                            *lcr = Some(WorkerRequest::Completion(completion_request));
                        }
                        Err(err) => panic!("{err:?}"),
                    }
                } else if request_is::<Generate>(&req) {
                    match cast::<Generate>(req) {
                        Ok((id, params)) => {
                            let rope = file_map
                                .get(params.text_document_position.text_document.uri.as_str())
                                .context("Error file not found")?
                                .clone();
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateRequest::new(id, params, rope);
                            *lcr = Some(WorkerRequest::Generate(completion_request));
                        }
                        Err(err) => panic!("{err:?}"),
                    }
                } else if request_is::<GenerateStream>(&req) {
                    match cast::<GenerateStream>(req) {
                        Ok((id, params)) => {
                            let rope = file_map
                                .get(params.text_document_position.text_document.uri.as_str())
                                .context("Error file not found")?
                                .clone();
                            let mut lcr = last_worker_request.lock();
                            let completion_request = GenerateStreamRequest::new(id, params, rope);
                            *lcr = Some(WorkerRequest::GenerateStream(completion_request));
                        }
                        Err(err) => panic!("{err:?}"),
                    }
                } else {
                    eprintln!("lsp-ai currently only supports textDocument/completion, textDocument/generate and textDocument/generateStream")
                }
            }
            Message::Notification(not) => {
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(&not) {
                    let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    let rope = Rope::from_str(&params.text_document.text);
                    file_map.insert(params.text_document.uri.to_string(), rope);
                } else if notification_is::<lsp_types::notification::DidChangeTextDocument>(&not) {
                    let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    let rope = file_map
                        .get_mut(params.text_document.uri.as_str())
                        .context("Error trying to get file that does not exist")?;
                    for change in params.content_changes {
                        // If range is ommitted, text is the new text of the document
                        if let Some(range) = change.range {
                            let start_index = rope.line_to_char(range.start.line as usize)
                                + range.start.character as usize;
                            let end_index = rope.line_to_char(range.end.line as usize)
                                + range.end.character as usize;
                            rope.remove(start_index..end_index);
                            rope.insert(start_index, &change.text);
                        } else {
                            *rope = Rope::from_str(&change.text);
                        }
                    }
                } else if notification_is::<lsp_types::notification::DidRenameFiles>(&not) {
                    let params: RenameFilesParams = serde_json::from_value(not.params)?;
                    for file_rename in params.files {
                        if let Some(rope) = file_map.remove(&file_rename.old_uri) {
                            file_map.insert(file_rename.new_uri, rope);
                        }
                    }
                }
            }
            _ => (),
        }
    }
    Ok(())
}
