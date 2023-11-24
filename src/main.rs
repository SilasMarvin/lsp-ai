use anyhow::Context;
use anyhow::Result;
use core::panic;
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    request::Completion, CompletionItem, CompletionItemKind, CompletionList, CompletionOptions,
    CompletionResponse, DidChangeTextDocumentParams, DidOpenTextDocumentParams, Position, Range,
    RenameFilesParams, ServerCapabilities, TextDocumentSyncKind, TextEdit,
};
use parking_lot::Mutex;
use serde::Deserialize;
// use pyo3::prelude::*;
// use pyo3::types::PyTuple;
use ropey::Rope;
use std::collections::HashMap;

mod transformer;

static FILE_MAP: once_cell::sync::Lazy<Mutex<HashMap<String, Rope>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

// Taken directly from: https://github.com/rust-lang/rust-analyzer
fn notification_is<N: lsp_types::notification::Notification>(notification: &Notification) -> bool {
    notification.method == N::METHOD
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
    main_loop(connection, initialization_params)?;
    io_threads.join()?;
    Ok(())
}

#[derive(Deserialize)]
struct Params {
    model: Option<String>,
    model_file: Option<String>,
    model_type: Option<String>,
    device: Option<String>,
}

fn main_loop(connection: Connection, params: serde_json::Value) -> Result<()> {
    let params: Params = serde_json::from_value(params)?;
    let mut text_generation = transformer::build()?;
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                match cast::<Completion>(req) {
                    Ok((id, params)) => {
                        // Get rope
                        let file_map = FILE_MAP.lock();
                        let mut rope = file_map
                            .get(params.text_document_position.text_document.uri.as_str())
                            .context("Error file not found")?
                            .clone();
                        let filter_text = rope
                            .get_line(params.text_document_position.position.line as usize)
                            .context("Error getting line with ropey")?
                            .to_string();

                        // Convert rope to correct prompt for llm
                        let start_index = rope
                            .line_to_char(params.text_document_position.position.line as usize)
                            + params.text_document_position.position.character as usize;
                        rope.insert(start_index, "<fim_suffix>");
                        let prompt = format!("<fim_prefix>{}<fim_middle>", rope);
                        let insert_text = text_generation.run(&prompt, 64)?;

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
                            text_edit: Some(lsp_types::CompletionTextEdit::Edit(
                                completion_text_edit,
                            )),
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
                        connection.sender.send(Message::Response(resp))?;
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
            }
            Message::Notification(not) => {
                eprintln!("got notification: {not:?}");
                if notification_is::<lsp_types::notification::DidOpenTextDocument>(&not) {
                    let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                    let rope = Rope::from_str(&params.text_document.text);
                    let mut file_map = FILE_MAP.lock();
                    file_map.insert(params.text_document.uri.to_string(), rope);
                } else if notification_is::<lsp_types::notification::DidChangeTextDocument>(&not) {
                    let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                    let mut file_map = FILE_MAP.lock();
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
                    let mut file_map = FILE_MAP.lock();
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

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_lsp() -> Result<()> {
//         let prompt = "def sum_two_numers(x: int, y:";
//         let result = Python::with_gil(|py| -> Result<String> {
//             let transform: Py<PyAny> = PY_MODULE
//                 .as_ref()
//                 .expect("Error getting python module")
//                 .getattr(py, "transform")
//                 .expect("Error getting transform");

//             let output = transform
//                 .call1(py, PyTuple::new(py, &[prompt]))
//                 .expect("Error calling transform");

//             Ok(output.extract(py).expect("Error extracting result"))
//         })?;
//         println!("\n\nTHE RESULT\n{:?}\n\n", result);
//         Ok(())
//     }
// }
