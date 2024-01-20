use lsp_types::{PartialResultParams, ProgressToken, TextDocumentPositionParams};
use serde::{Deserialize, Serialize};

pub enum GenerateStream {}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateStreamParams {
    pub partial_result_token: ProgressToken,

    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateStreamResult {
    pub generated_text: String,
    pub partial_result_token: ProgressToken,
}

impl lsp_types::request::Request for GenerateStream {
    type Params = GenerateStreamParams;
    type Result = GenerateStreamResult;
    const METHOD: &'static str = "textDocument/generateStream";
}
