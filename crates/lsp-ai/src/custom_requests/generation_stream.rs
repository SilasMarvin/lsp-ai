use lsp_types::{ProgressToken, TextDocumentPositionParams};
use serde::{Deserialize, Serialize};

pub(crate) enum GenerationStream {}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationStreamParams {
    pub partial_result_token: ProgressToken,

    // This field was "mixed-in" from TextDocumentPositionParams
    #[serde(flatten)]
    pub text_document_position: TextDocumentPositionParams,
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationStreamResult {
    pub(crate) generated_text: String,
    pub(crate) partial_result_token: ProgressToken,
}

impl lsp_types::request::Request for GenerationStream {
    type Params = GenerationStreamParams;
    type Result = GenerationStreamResult;
    const METHOD: &'static str = "textDocument/generationStream";
}
