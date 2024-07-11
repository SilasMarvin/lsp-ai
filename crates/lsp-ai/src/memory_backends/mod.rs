use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};
use serde_json::Value;

use crate::config::{Config, ValidMemoryBackend};

pub(crate) mod file_store;
mod postgresml;

#[derive(thiserror::Error, Debug)]
pub(crate) enum MemoryBackendError {
    #[error("failed to convert Prompt into {0}")]
    ConvertPrompt(String),
    #[error("crawl error: {0}")]
    Crawl(#[from] crate::crawl::CrawlError),
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("file store error: {0}")]
    FileStore(#[from] file_store::FileStoreError),
    #[error("line out of bounds: {0}")]
    LineOutOfBounds(usize),
    #[error("file store error: {0}")]
    PostgresML(#[from] postgresml::PostgresMLError),
    #[error("ropey error: {0}")]
    Ropey(#[from] ropey::Error),
    #[error("slice range out of bounds: {0}..{1}")]
    SliceRangeOutOfBounds(usize, usize),
}

pub(crate) type Result<T> = std::result::Result<T, MemoryBackendError>;

#[derive(Clone, Debug)]
pub enum PromptType {
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
    type Error = MemoryBackendError;

    fn try_from(value: &'a Prompt) -> Result<Self> {
        match value {
            Prompt::ContextAndCode(code_and_context) => Ok(code_and_context),
            _ => Err(MemoryBackendError::ConvertPrompt(
                "CodeAndContextPrompt".to_owned(),
            )),
        }
    }
}

impl TryFrom<Prompt> for ContextAndCodePrompt {
    type Error = MemoryBackendError;

    fn try_from(value: Prompt) -> Result<Self> {
        match value {
            Prompt::ContextAndCode(code_and_context) => Ok(code_and_context),
            _ => Err(MemoryBackendError::ConvertPrompt(
                "CodeAndContextPrompt".to_owned(),
            )),
        }
    }
}

impl TryFrom<Prompt> for FIMPrompt {
    type Error = MemoryBackendError;

    fn try_from(value: Prompt) -> Result<Self> {
        match value {
            Prompt::FIM(fim) => Ok(fim),
            _ => Err(MemoryBackendError::ConvertPrompt("FIMPrompt".to_owned())),
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
    async fn init(&self) -> Result<()> {
        Ok(())
    }
    fn opened_text_document(&self, params: DidOpenTextDocumentParams) -> Result<()>;
    fn changed_text_document(&self, params: DidChangeTextDocumentParams) -> Result<()>;
    fn renamed_files(&self, params: RenameFilesParams) -> Result<()>;
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> Result<String>;
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> Result<Prompt>;
}

impl TryFrom<Config> for Box<dyn MemoryBackend + Send + Sync> {
    type Error = MemoryBackendError;

    fn try_from(configuration: Config) -> Result<Self> {
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
