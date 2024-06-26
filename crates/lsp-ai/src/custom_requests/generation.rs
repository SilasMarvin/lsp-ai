use lsp_types::TextDocumentPositionParams;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config;

pub(crate) enum Generation {}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationParams {
    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub(crate) text_document_position: TextDocumentPositionParams,
    // The model key to use
    pub(crate) model: String,
    #[serde(default)]
    // Args are deserialized by the backend using them
    pub(crate) parameters: Value,
    // Parameters for post processing
    #[serde(default)]
    pub(crate) post_process: config::PostProcess,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateResult {
    pub(crate) generated_text: String,
}

impl lsp_types::request::Request for Generation {
    type Params = GenerationParams;
    type Result = GenerateResult;
    const METHOD: &'static str = "textDocument/generation";
}
