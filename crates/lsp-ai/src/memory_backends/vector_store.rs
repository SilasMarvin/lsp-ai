use std::sync::Arc;

use anyhow::Context;
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, RenameFilesParams,
    TextDocumentPositionParams,
};
use parking_lot::Mutex;
use serde_json::Value;

use crate::{
    config::{self, Config},
    crawl::Crawl,
    splitters::Splitter,
};

use super::{
    file_store::{AdditionalFileStoreParams, FileStore},
    MemoryBackend, Prompt, PromptType,
};

pub struct VectorStore {
    file_store: FileStore,
    // TODO: Verify we need these Arc<>
    crawl: Option<Arc<Mutex<Crawl>>>,
    splitter: Arc<Box<dyn Splitter + Send + Sync>>,
}

impl VectorStore {
    pub fn new(
        mut vector_store_config: config::VectorStore,
        config: Config,
    ) -> anyhow::Result<Self> {
        let crawl = vector_store_config
            .crawl
            .take()
            .map(|x| Arc::new(Mutex::new(Crawl::new(x, config.clone()))));

        let splitter: Arc<Box<dyn Splitter + Send + Sync>> =
            Arc::new(vector_store_config.splitter.clone().try_into()?);

        let file_store = FileStore::new_with_params(
            config::FileStore::new_without_crawl(),
            config.clone(),
            AdditionalFileStoreParams::new(splitter.does_use_tree_sitter()),
        )?;

        Ok(Self {
            file_store,
            crawl,
            splitter,
        })
    }
}

#[async_trait::async_trait]
impl MemoryBackend for VectorStore {
    fn opened_text_document(&self, params: DidOpenTextDocumentParams) -> anyhow::Result<()> {
        // Pass through
        let uri = params.text_document.uri.to_string();
        self.file_store.opened_text_document(params)?;
        // Split into chunks
        let file_map = self.file_store.file_map().lock();
        let file = file_map.get(&uri).context("file not found")?;
        let chunks = self.splitter.split(file);
        // Embed it
        Ok(())
    }

    fn changed_text_document(&self, params: DidChangeTextDocumentParams) -> anyhow::Result<()> {
        self.file_store.changed_text_document(params.clone())?;
        Ok(())
    }

    fn renamed_files(&self, params: RenameFilesParams) -> anyhow::Result<()> {
        self.file_store.renamed_files(params.clone())?;
        Ok(())
    }

    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        self.file_store.get_filter_text(position)
    }

    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> anyhow::Result<Prompt> {
        todo!()
    }
}
