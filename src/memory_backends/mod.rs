use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};
use serde_json::Value;

use crate::config::{Config, ValidMemoryBackend};

pub mod file_store;
mod postgresml;

#[derive(Clone, Debug)]
pub enum PromptType {
    ContextAndCode,
    FIM,
}

#[derive(Clone)]
pub struct MemoryRunParams {
    pub is_for_chat: bool,
    pub max_context_length: usize,
}

impl From<&Value> for MemoryRunParams {
    fn from(value: &Value) -> Self {
        Self {
            max_context_length: value["max_context_length"].as_u64().unwrap_or(1024) as usize,
            // messages are for most backends, contents are for Gemini
            is_for_chat: value["messages"].is_array() || value["contents"].is_array(),
        }
    }
}

#[derive(Debug)]
pub struct ContextAndCodePrompt {
    pub context: String,
    pub code: String,
}

impl ContextAndCodePrompt {
    pub fn new(context: String, code: String) -> Self {
        Self { context, code }
    }
}

#[derive(Debug)]
pub struct FIMPrompt {
    pub prompt: String,
    pub suffix: String,
}

impl FIMPrompt {
    pub fn new(prefix: String, suffix: String) -> Self {
        Self {
            prompt: prefix,
            suffix,
        }
    }
}

#[derive(Debug)]
pub enum Prompt {
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
pub trait MemoryBackend {
    async fn init(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn opened_text_document(&self, params: DidOpenTextDocumentParams) -> anyhow::Result<()>;
    async fn changed_text_document(
        &self,
        params: DidChangeTextDocumentParams,
    ) -> anyhow::Result<()>;
    async fn renamed_files(&self, params: RenameFilesParams) -> anyhow::Result<()>;
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> anyhow::Result<Prompt>;
    async fn get_filter_text(
        &self,
        position: &TextDocumentPositionParams,
    ) -> anyhow::Result<String>;
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
        }
    }
}

// This makes testing much easier. Every transformer backend takes in a prompt. When verifying they work, its
// easier to just pass in a default prompt.
#[cfg(test)]
impl Prompt {
    pub fn default_with_cursor() -> Self {
        Self::ContextAndCode(ContextAndCodePrompt::new(
            r#"def test_context():\n    pass"#.to_string(),
            r#"def test_code():\n    <CURSOR>"#.to_string(),
        ))
    }

    pub fn default_fim() -> Self {
        Self::FIM(FIMPrompt::new(
            r#"def test_context():\n    pass"#.to_string(),
            r#"def test_code():\n    "#.to_string(),
        ))
    }

    pub fn default_without_cursor() -> Self {
        Self::ContextAndCode(ContextAndCodePrompt::new(
            r#"def test_context():\n    pass"#.to_string(),
            r#"def test_code():\n    "#.to_string(),
        ))
    }
}
