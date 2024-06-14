use lsp_types::TextDocumentPositionParams;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

pub enum Generation {}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationParams {
    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
    // The model key to use
    pub model: String,
    #[serde(default)]
    // Args are deserialized by the backend using them
    pub parameters: Value,
    // Parameters for post processing
    #[serde(default)]
    pub post_process: config::PostProcess,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub generated_text: String,
}

impl lsp_types::request::Request for Generation {
    type Params = GenerationParams;
    type Result = GenerateResult;
    const METHOD: &'static str = "textDocument/generation";
}
