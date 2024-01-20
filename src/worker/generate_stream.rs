use lsp_server::ResponseError;
use pyo3::prelude::*;

use super::{GenerateRequest, GenerateStreamRequest};
use crate::PY_MODULE;

pub fn do_generate_stream(request: &GenerateStreamRequest) -> Result<(), ResponseError> {
    // Convert rope to correct prompt for llm
    // let cursor_index = request
    //     .rope
    //     .line_to_char(request.params.text_document_position.position.line as usize)
    //     + request.params.text_document_position.position.character as usize;

    // // We will want to have some kind of infill support we add
    // // rope.insert(cursor_index, "<｜fim_hole｜>");
    // // rope.insert(0, "<｜fim_start｜>");
    // // rope.insert(rope.len_chars(), "<｜fim_end｜>");
    // // let prompt = rope.to_string();

    // let prompt = request
    //     .rope
    //     .get_slice(0..cursor_index)
    //     .expect("Error getting rope slice")
    //     .to_string();

    // eprintln!("\n\n****{prompt}****\n\n");

    // Python::with_gil(|py| -> anyhow::Result<String> {
    //     let transform: Py<PyAny> = PY_MODULE
    //         .as_ref()
    //         .map_err(anyhow::Error::msg)?
    //         .getattr(py, "transform")?;

    //     let out: String = transform.call1(py, (prompt,))?.extract(py)?;
    //     Ok(out)
    // })
    // .map(|generated_text| DoGenerateResponse { generated_text })
    // .map_err(|e| ResponseError {
    //     code: -32603,
    //     message: e.to_string(),
    //     data: None,
    // })

    Ok(())
}
