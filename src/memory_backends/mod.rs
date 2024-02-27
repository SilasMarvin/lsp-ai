use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};

use crate::configuration::{Configuration, ValidMemoryBackend};

pub mod file_store;

#[derive(Debug)]
pub struct Prompt {
    pub context: String,
    pub code: String,
}

impl Prompt {
    fn new(context: String, code: String) -> Self {
        Self { context, code }
    }
}

pub trait MemoryBackend {
    fn init(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn opened_text_document(&mut self, params: DidOpenTextDocumentParams) -> anyhow::Result<()>;
    fn changed_text_document(&mut self, params: DidChangeTextDocumentParams) -> anyhow::Result<()>;
    fn renamed_file(&mut self, params: RenameFilesParams) -> anyhow::Result<()>;
    fn build_prompt(&self, position: &TextDocumentPositionParams) -> anyhow::Result<Prompt>;
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String>;
}

impl TryFrom<Configuration> for Box<dyn MemoryBackend + Send> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.get_memory_backend()? {
            ValidMemoryBackend::FileStore => {
                Ok(Box::new(file_store::FileStore::new(configuration)))
            }
            _ => unimplemented!(),
        }
    }
}
