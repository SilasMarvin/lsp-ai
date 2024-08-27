use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Range, RenameFilesParams,
    TextDocumentIdentifier, TextDocumentPositionParams,
};
use serde_json::Value;

use crate::config::{Config, ValidMemoryBackend};

pub(crate) mod file_store;
mod postgresml;
mod vector_store;

#[derive(Clone, Debug)]
pub(crate) enum PromptType {
    ContextAndCode,
    FIM,
}

#[derive(Clone)]
pub(crate) struct MemoryRunParams {
    pub(crate) is_for_chat: bool,
    pub(crate) max_context: usize,
}

impl From<&Value> for MemoryRunParams {
    fn from(value: &Value) -> Self {
        Self {
            max_context: value["max_context"].as_u64().unwrap_or(1024) as usize,
            // messages are for most backends, contents are for Gemini
            is_for_chat: value["messages"].is_array() || value["contents"].is_array(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ContextAndCodePrompt {
    pub(crate) context: String,
    pub(crate) code: String,
    pub(crate) selected_text: Option<String>,
}

#[derive(Debug)]
pub(crate) struct FIMPrompt {
    pub(crate) prompt: String,
    pub(crate) suffix: String,
}

#[derive(Debug)]
pub(crate) enum Prompt {
    FIM(FIMPrompt),
    ContextAndCode(ContextAndCodePrompt),
}

impl<'a> TryFrom<&'a Prompt> for &'a ContextAndCodePrompt {
    type Error = anyhow::Error;

    fn try_from(value: &'a Prompt) -> Result<Self, Self::Error> {
        match value {
            Prompt::ContextAndCode(code_and_context) => Ok(code_and_context),
            _ => anyhow::bail!("cannot convert Prompt into CodeAndContextPrompt"),
        }
    }
}

impl TryFrom<Prompt> for ContextAndCodePrompt {
    type Error = anyhow::Error;

    fn try_from(value: Prompt) -> Result<Self, Self::Error> {
        match value {
            Prompt::ContextAndCode(code_and_context) => Ok(code_and_context),
            _ => anyhow::bail!("cannot convert Prompt into CodeAndContextPrompt"),
        }
    }
}

impl TryFrom<Prompt> for FIMPrompt {
    type Error = anyhow::Error;

    fn try_from(value: Prompt) -> Result<Self, Self::Error> {
        match value {
            Prompt::FIM(fim) => Ok(fim),
            _ => anyhow::bail!("cannot convert Prompt into FIMPrompt"),
        }
    }
}

impl<'a> TryFrom<&'a Prompt> for &'a FIMPrompt {
    type Error = anyhow::Error;

    fn try_from(value: &'a Prompt) -> Result<Self, Self::Error> {
        match value {
            Prompt::FIM(fim) => Ok(fim),
            _ => anyhow::bail!("cannot convert Prompt into FIMPrompt"),
        }
    }
}

#[async_trait::async_trait]
pub(crate) trait MemoryBackend {
    fn opened_text_document(&self, params: DidOpenTextDocumentParams) -> anyhow::Result<()>;
    fn code_action_request(
        &self,
        text_document_identifier: &TextDocumentIdentifier,
        range: &Range,
        trigger: &str,
    ) -> anyhow::Result<bool>;
    fn file_request(
        &self,
        text_document_identifier: &TextDocumentIdentifier,
    ) -> anyhow::Result<String>;
    fn changed_text_document(&self, params: DidChangeTextDocumentParams) -> anyhow::Result<()>;
    fn renamed_files(&self, params: RenameFilesParams) -> anyhow::Result<()>;
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String>;
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> anyhow::Result<Prompt>;
}

impl TryFrom<Config> for Box<dyn MemoryBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(configuration: Config) -> Result<Self, Self::Error> {
        match configuration.config.memory.clone() {
            ValidMemoryBackend::FileStore(file_store_config) => Ok(Box::new(
                file_store::FileStore::new(file_store_config, configuration)?,
            )),
            ValidMemoryBackend::PostgresML(postgresml_config) => Ok(Box::new(
                postgresml::PostgresML::new(postgresml_config, configuration)?,
            )),
            ValidMemoryBackend::VectorStore(vector_store_config) => Ok(Box::new(
                vector_store::VectorStore::new(vector_store_config, configuration)?,
            )),
        }
    }
}

// This makes testing much easier. Every transformer backend takes in a prompt. When verifying they work, its
// easier to just pass in a default prompt.
#[cfg(test)]
impl Prompt {
    pub(crate) fn default_with_cursor() -> Self {
        Self::ContextAndCode(ContextAndCodePrompt {
            context: r#"def test_context():\n    pass"#.to_string(),
            code: r#"def test_code():\n    <CURSOR>"#.to_string(),
            selected_text: None,
        })
    }

    pub(crate) fn default_fim() -> Self {
        Self::FIM(FIMPrompt {
            prompt: r#"def test_context():\n    pass"#.to_string(),
            suffix: r#"def test_code():\n    "#.to_string(),
        })
    }

    pub(crate) fn default_without_cursor() -> Self {
        Self::ContextAndCode(ContextAndCodePrompt {
            context: r#"def test_context():\n    pass"#.to_string(),
            code: r#"def test_code():\n    "#.to_string(),
            selected_text: None,
        })
    }
}
