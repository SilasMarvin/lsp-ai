use anyhow::Context;
use anyhow::Result;
use core::panic;
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    request::Completion, CompletionItem, CompletionItemKind, CompletionList, CompletionOptions,
    CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    Position, Range, RenameFilesParams, ServerCapabilities, TextDocumentSyncKind, TextEdit,
};
use parking_lot::Mutex;
use ropey::Rope;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use once_cell::sync::Lazy;
use pyo3::prelude::*;

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

struct CompletionRequest {
    id: RequestId,
    params: CompletionParams,
    rope: Rope,
}

impl CompletionRequest {
    fn new(id: RequestId, params: CompletionParams, rope: Rope) -> Self {
        Self { id, params, rope }
    }
}

// This main loop is tricky
// We create a worker thread that actually does the heavy lifting because we do not want to process every completion request we get
// Completion requests may take a few seconds given the model configuration and hardware allowed, and we only want to process the latest completion request
fn main_loop(connection: Connection, params: serde_json::Value) -> Result<()> {
    let params: Params = serde_json::from_value(params)?;

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
    let last_completion_request = Arc::new(Mutex::new(None));

    // Thread local variables
    let thread_last_completion_request = last_completion_request.clone();
    let thread_connection = connection.clone();
    thread::spawn(move || {
        loop {
            // I think we need this drop, not 100% sure though
            let mut completion_request = thread_last_completion_request.lock();
            let params = std::mem::take(&mut *completion_request);
            drop(completion_request);
            if let Some(CompletionRequest {
                id,
                params,
                mut rope,
            }) = params
            {
                let filter_text = rope
                    .get_line(params.text_document_position.position.line as usize)
                    .expect("Error getting line with ropey")
                    .to_string();

                // Convert rope to correct prompt for llm
                let cursor_index = rope
                    .line_to_char(params.text_document_position.position.line as usize)
                    + params.text_document_position.position.character as usize;

                // We will want to have some kind of infill support we add
                // rope.insert(cursor_index, "<｜fim_hole｜>");
                // rope.insert(0, "<｜fim_start｜>");
                // rope.insert(rope.len_chars(), "<｜fim_end｜>");
                // let prompt = rope.to_string();

                let prompt = rope
                    .get_slice((0..cursor_index))
                    .expect("Error getting rope slice")
                    .to_string();

                eprintln!("\n\n****{prompt}****\n\n");

                let insert_text = Python::with_gil(|py| -> Result<String> {
                    let transform: Py<PyAny> = PY_MODULE
                        .as_ref()
                        .map_err(anyhow::Error::msg)?
                        .getattr(py, "transform")?;

                    let out: String = transform.call1(py, (prompt,))?.extract(py)?;
                    Ok(out)
                })
                .expect("Error during transform");

                eprintln!("\n{insert_text}\n");

                // Create and return the completion
                let completion_text_edit = TextEdit::new(
                    Range::new(
                        Position::new(
                            params.text_document_position.position.line,
                            params.text_document_position.position.character,
                        ),
                        Position::new(
                            params.text_document_position.position.line,
                            params.text_document_position.position.character,
                        ),
                    ),
                    insert_text.clone(),
                );
                let item = CompletionItem {
                    label: format!("ai - {insert_text}"),
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
                let result = serde_json::to_value(&result).unwrap();
                let resp = Response {
                    id,
                    result: Some(result),
                    error: None,
                };
                thread_connection
                    .sender
                    .send(Message::Response(resp))
                    .expect("Error sending response");
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                match cast::<Completion>(req) {
                    Ok((id, params)) => {
                        // Get rope
                        let rope = file_map
                            .get(params.text_document_position.text_document.uri.as_str())
                            .context("Error file not found")?
                            .clone();
                        // Update the last CompletionRequest
                        let mut lcr = last_completion_request.lock();
                        *lcr = Some(CompletionRequest::new(id, params, rope));
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
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
