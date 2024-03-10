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

pub trait MemoryBackend {
    fn init(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn opened_text_document(&mut self, params: DidOpenTextDocumentParams) -> anyhow::Result<()>;
    fn changed_text_document(&mut self, params: DidChangeTextDocumentParams) -> anyhow::Result<()>;
    fn renamed_file(&mut self, params: RenameFilesParams) -> anyhow::Result<()>;
    fn build_prompt(
        &mut self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt>;
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String>;
}

impl TryFrom<Configuration> for Box<dyn MemoryBackend + Send> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.get_memory_backend()? {
            ValidMemoryBackend::FileStore => {
                Ok(Box::new(file_store::FileStore::new(configuration)))
            }
            ValidMemoryBackend::PostgresML(postgresml_config) => Ok(Box::new(
                postgresml::PostgresML::new(postgresml_config, configuration)?,
            )),
        }
    }
}
