use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};

use crate::configuration::{Configuration, ValidMemoryBackend};

pub mod file_store;
mod postgresml;

#[derive(Debug)]
pub struct Prompt {
    pub context: String,
    pub code: String,
}

impl Prompt {
    pub fn new(context: String, code: String) -> Self {
        Self { context, code }
    }
}

#[derive(Debug)]
pub enum PromptForType {
    Completion,
    Generate,
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
    async fn renamed_file(&self, params: RenameFilesParams) -> anyhow::Result<()>;
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt>;
    async fn get_filter_text(
        &self,
        position: &TextDocumentPositionParams,
    ) -> anyhow::Result<String>;
}

impl TryFrom<Configuration> for Box<dyn MemoryBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.get_memory_backend()? {
            ValidMemoryBackend::FileStore(file_store_config) => Ok(Box::new(
                file_store::FileStore::new(file_store_config, configuration),
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
        Self {
            context: r#"def test_context():\n    pass"#.to_string(),
            code: r#"def test_code():\n    <CURSOR>"#.to_string(),
        }
    }

    pub fn default_without_cursor() -> Self {
        Self {
            context: r#"def test_context():\n    pass"#.to_string(),
            code: r#"def test_code():\n    "#.to_string(),
        }
    }
}
