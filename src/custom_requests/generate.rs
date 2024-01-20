use lsp_types::TextDocumentPositionParams;
use serde::{Deserialize, Serialize};

pub enum Generate {}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateParams {
    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub generated_text: String,
}

impl lsp_types::request::Request for Generate {
    type Params = GenerateParams;
    type Result = GenerateResult;
    const METHOD: &'static str = "textDocument/generate";
}
