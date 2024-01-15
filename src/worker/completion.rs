use lsp_server::ResponseError;
use pyo3::prelude::*;

use super::CompletionRequest;
use crate::PY_MODULE;

pub struct DoCompletionResponse {
    pub insert_text: String,
    pub filter_text: String,
}

pub fn do_completion(request: &CompletionRequest) -> Result<DoCompletionResponse, ResponseError> {
    let filter_text = request
        .rope
        .get_line(request.params.text_document_position.position.line as usize)
        .ok_or(ResponseError {
            code: -32603, // Maybe we want a different error code here?
            message: "Error getting line in requested document".to_string(),
            data: None,
        })?
        .to_string();

    // Convert rope to correct prompt for llm
    let cursor_index = request
        .rope
        .line_to_char(request.params.text_document_position.position.line as usize)
        + request.params.text_document_position.position.character as usize;

    // We will want to have some kind of infill support we add
    // rope.insert(cursor_index, "<｜fim_hole｜>");
    // rope.insert(0, "<｜fim_start｜>");
    // rope.insert(rope.len_chars(), "<｜fim_end｜>");
    // let prompt = rope.to_string();

    let prompt = request
        .rope
        .get_slice(0..cursor_index)
        .expect("Error getting rope slice")
        .to_string();

    eprintln!("\n\n****{prompt}****\n\n");

    Python::with_gil(|py| -> anyhow::Result<String> {
        let transform: Py<PyAny> = PY_MODULE
            .as_ref()
            .map_err(anyhow::Error::msg)?
            .getattr(py, "transform")?;

        let out: String = transform.call1(py, (prompt,))?.extract(py)?;
        Ok(out)
    })
    .map(|insert_text| DoCompletionResponse {
        insert_text,
        filter_text,
    })
    .map_err(|e| ResponseError {
        code: -32603,
        message: e.to_string(),
        data: None,
    })
}
