use lsp_types::TextDocumentPositionParams;
use serde::{Deserialize, Serialize};

pub enum Generation {}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationParams {
    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
    pub model: String,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub generated_text: String,
}

impl lsp_types::request::Request for Generation {
    type Params = GenerationParams;
    type Result = GenerateResult;
    const METHOD: &'static str = "textDocument/generation";
}
