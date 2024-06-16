use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use pgml::{Collection, Pipeline};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::{
        mpsc::{self, Sender},
        Arc,
    },
    time::Duration,
};
use tokio::time;
use tracing::{error, instrument, warn};

use crate::{
    config::{self, Config},
    crawl::Crawl,
    splitters::{Chunk, Splitter},
    utils::{chunk_to_id, tokens_to_estimated_characters, TOKIO_RUNTIME},
};

use super::{
    file_store::{AdditionalFileStoreParams, FileStore},
    ContextAndCodePrompt, FIMPrompt, MemoryBackend, MemoryRunParams, Prompt, PromptType,
};

fn chunk_to_document(uri: &str, chunk: Chunk) -> Value {
    json!({
        "id": chunk_to_id(uri, &chunk),
        "uri": uri,
        "text": chunk.text,
        "range": chunk.range
    })
}

async fn split_and_upsert_file(
    uri: &str,
    collection: &mut Collection,
    file_store: Arc<FileStore>,
    splitter: Arc<Box<dyn Splitter + Send + Sync>>,
) -> anyhow::Result<()> {
    // We need to make sure we don't hold the file_store lock while performing a network call
    let chunks = {
        file_store
            .file_map()
            .lock()
            .get(uri)
            .map(|f| splitter.split(f))
    };
    let chunks = chunks.with_context(|| format!("file not found for splitting: {uri}"))?;
    let documents = chunks
        .into_iter()
        .map(|chunk| chunk_to_document(uri, chunk).into())
        .collect();
    collection
        .upsert_documents(documents, None)
        .await
        .context("PGML - Error upserting documents")
}

#[derive(Clone)]
pub struct PostgresML {
    _config: Config,
    file_store: Arc<FileStore>,
    collection: Collection,
    pipeline: Pipeline,
    debounce_tx: Sender<String>,
    crawl: Option<Arc<Mutex<Crawl>>>,
    splitter: Arc<Box<dyn Splitter + Send + Sync>>,
}

impl PostgresML {
    #[instrument]
    pub fn new(
        mut postgresml_config: config::PostgresML,
        configuration: Config,
    ) -> anyhow::Result<Self> {
        let crawl = postgresml_config
            .crawl
            .take()
            .map(|x| Arc::new(Mutex::new(Crawl::new(x, configuration.clone()))));

        let splitter: Arc<Box<dyn Splitter + Send + Sync>> =
            Arc::new(postgresml_config.splitter.try_into()?);

        let file_store = Arc::new(FileStore::new_with_params(
            config::FileStore::new_without_crawl(),
            configuration.clone(),
            AdditionalFileStoreParams::new(splitter.does_use_tree_sitter()),
        )?);

        let database_url = if let Some(database_url) = postgresml_config.database_url {
            database_url
        } else {
            std::env::var("PGML_DATABASE_URL")?
        };

        // TODO: Think through Collections and Pipelines
        let mut collection = Collection::new("test-lsp-ai-5", Some(database_url))?;
        let mut pipeline = Pipeline::new(
            "v1",
            Some(
                json!({
                    "text": {
                        "semantic_search": {
                            "model": "intfloat/e5-small-v2",
                            "parameters": {
                                "prompt": "passage: "
                            }
                        }
                    }
                })
                .into(),
            ),
        )?;

        // Add the Pipeline to the Collection
        TOKIO_RUNTIME.block_on(async {
            collection
                .add_pipeline(&mut pipeline)
                .await
                .context("PGML - Error adding pipeline to collection")
        })?;

        // Setup up a debouncer for changed text documents
        let (debounce_tx, debounce_rx) = mpsc::channel::<String>();
        let mut task_collection = collection.clone();
        let task_file_store = file_store.clone();
        let task_splitter = splitter.clone();
        TOKIO_RUNTIME.spawn(async move {
            let duration = Duration::from_millis(500);
            let mut file_uris = Vec::new();
            loop {
                time::sleep(duration).await;
                let new_uris: Vec<String> = debounce_rx.try_iter().collect();
                if !new_uris.is_empty() {
                    for uri in new_uris {
                        if !file_uris.iter().any(|p| *p == uri) {
                            file_uris.push(uri);
                        }
                    }
                } else {
                    if file_uris.is_empty() {
                        continue;
                    }

                    // Build the chunks for our changed files
                    let chunks: Vec<Vec<Chunk>> = match file_uris
                        .iter()
                        .map(|uri| {
                            let file_store = task_file_store.file_map().lock();
                            let file = file_store
                                .get(uri)
                                .with_context(|| format!("getting file for splitting: {uri}"))?;
                            anyhow::Ok(task_splitter.split(file))
                        })
                        .collect()
                    {
                        Ok(chunks) => chunks,
                        Err(e) => {
                            error!("{e}");
                            continue;
                        }
                    };

                    // Delete old chunks that no longer exist after the latest file changes
                    let delete_or_statements: Vec<Value> = file_uris
                        .iter()
                        .zip(&chunks)
                        .map(|(uri, chunks)| {
                            let ids: Vec<String> =
                                chunks.iter().map(|c| chunk_to_id(uri, c)).collect();
                            json!({
                                "$and": [
                                    {
                                        "uri": {
                                            "$eq": uri
                                        }
                                    },
                                    {
                                        "id": {
                                            "$nin": ids
                                        }
                                    }
                                ]
                            })
                        })
                        .collect();
                    if let Err(e) = task_collection
                        .delete_documents(
                            json!({
                                "$or": delete_or_statements
                            })
                            .into(),
                        )
                        .await
                    {
                        error!("PGML - Error deleting file: {e:?}");
                    }

                    // Prepare and upsert our new chunks
                    let documents: Vec<pgml::types::Json> = chunks
                        .into_iter()
                        .zip(&file_uris)
                        .map(|(chunks, uri)| {
                            chunks
                                .into_iter()
                                .map(|chunk| chunk_to_document(&uri, chunk))
                                .collect::<Vec<Value>>()
                        })
                        .flatten()
                        .map(|f: Value| f.into())
                        .collect();
                    if let Err(e) = task_collection
                        .upsert_documents(documents, None)
                        .await
                        .context("PGML - Error upserting changed files")
                    {
                        error!("{e}");
                        continue;
                    }

                    file_uris = Vec::new();
                }
            }
        });

        let s = Self {
            _config: configuration,
            file_store,
            collection,
            pipeline,
            debounce_tx,
            crawl,
            splitter,
        };

        if let Err(e) = s.maybe_do_crawl(None) {
            error!("{e}")
        }
        Ok(s)
    }

    fn maybe_do_crawl(&self, triggered_file: Option<String>) -> anyhow::Result<()> {
        if let Some(crawl) = &self.crawl {
            let mut documents: Vec<(String, Vec<Chunk>)> = vec![];
            let mut total_bytes = 0;
            let mut current_bytes = 0;
            crawl
                .lock()
                .maybe_do_crawl(triggered_file, |config, path| {
                    // Break if total bytes is over the max crawl memory
                    if total_bytes as u64 >= config.max_crawl_memory {
                        warn!("Ending crawl early due to `max_crawl_memory` resetraint");
                        return Ok(false);
                    }
                    // This means it has been opened before
                    let uri = format!("file://{path}");
                    if self.file_store.contains_file(&uri) {
                        return Ok(true);
                    }
                    // Open the file and see if it is small enough to read
                    let mut f = std::fs::File::open(path)?;
                    let metadata = f.metadata()?;
                    if metadata.len() > config.max_file_size {
                        warn!("Skipping file: {path} because it is too large");
                        return Ok(true);
                    }
                    // Read the file contents
                    let mut contents = vec![];
                    f.read_to_end(&mut contents)?;
                    let contents = String::from_utf8(contents)?;
                    current_bytes += contents.len();
                    total_bytes += contents.len();
                    let chunks = self.splitter.split_file_contents(&uri, &contents);
                    documents.push((uri, chunks));
                    // If we have over 100 mega bytes of data do the upsert
                    if current_bytes >= 100_000_000 || total_bytes as u64 >= config.max_crawl_memory
                    {
                        // Prepare our chunks
                        let to_upsert_documents: Vec<pgml::types::Json> =
                            std::mem::take(&mut documents)
                                .into_iter()
                                .map(|(uri, chunks)| {
                                    chunks
                                        .into_iter()
                                        .map(|chunk| chunk_to_document(&uri, chunk))
                                        .collect::<Vec<Value>>()
                                })
                                .flatten()
                                .map(|f: Value| f.into())
                                .collect();
                        // Do the upsert
                        let mut collection = self.collection.clone();
                        TOKIO_RUNTIME.spawn(async move {
                            if let Err(e) = collection
                                .upsert_documents(to_upsert_documents, None)
                                .await
                                .context("PGML - Error upserting changed files")
                            {
                                error!("{e}");
                            }
                        });
                        // Reset everything
                        current_bytes = 0;
                        documents = vec![];
                    }
                    Ok(true)
                })?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl MemoryBackend for PostgresML {
    #[instrument(skip(self))]
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        self.file_store.get_filter_text(position)
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> anyhow::Result<Prompt> {
        let params: MemoryRunParams = params.try_into()?;

        // Build the query
        let query = self
            .file_store
            .get_characters_around_position(position, 512)?;

        // Get the code around the Cursor
        let mut file_store_params = params.clone();
        file_store_params.max_context_length = 512;
        let code = self
            .file_store
            .build_code(position, prompt_type, file_store_params)?;

        // Get the context
        let limit = params.max_context_length / 512;
        let res = self
            .collection
            .vector_search_local(
                json!({
                    "query": {
                        "fields": {
                            "text": {
                                "query": query,
                                "parameters": {
                                    "prompt": "query: "
                                }
                            }
                        },
                    },
                    "limit": limit
                })
                .into(),
                &self.pipeline,
            )
            .await?;
        let context = res
            .into_iter()
            .map(|c| {
                c["chunk"]
                    .as_str()
                    .map(|t| t.to_owned())
                    .context("PGML - Error getting chunk from vector search")
            })
            .collect::<anyhow::Result<Vec<String>>>()?
            .join("\n\n");

        let chars = tokens_to_estimated_characters(params.max_context_length.saturating_sub(512));
        let context = &context[..chars.min(context.len())];

        // Reconstruct the Prompts
        Ok(match code {
            Prompt::ContextAndCode(context_and_code) => Prompt::ContextAndCode(
                ContextAndCodePrompt::new(context.to_owned(), context_and_code.code),
            ),
            Prompt::FIM(fim) => Prompt::FIM(FIMPrompt::new(
                format!("{context}\n\n{}", fim.prompt),
                fim.suffix,
            )),
        })
    }

    #[instrument(skip(self))]
    fn opened_text_document(
        &self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        self.file_store.opened_text_document(params.clone())?;

        let saved_uri = params.text_document.uri.to_string();

        let mut collection = self.collection.clone();
        let file_store = self.file_store.clone();
        let splitter = self.splitter.clone();
        TOKIO_RUNTIME.spawn(async move {
            let uri = params.text_document.uri.to_string();
            if let Err(e) = split_and_upsert_file(&uri, &mut collection, file_store, splitter).await
            {
                error!("{e:?}")
            }
        });

        if let Err(e) = self.maybe_do_crawl(Some(saved_uri)) {
            error!("{e:?}")
        }
        Ok(())
    }

    #[instrument(skip(self))]
    fn changed_text_document(
        &self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        self.file_store.changed_text_document(params.clone())?;
        let uri = params.text_document.uri.to_string();
        self.debounce_tx.send(uri)?;
        Ok(())
    }

    #[instrument(skip(self))]
    fn renamed_files(&self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        self.file_store.renamed_files(params.clone())?;

        let mut collection = self.collection.clone();
        let file_store = self.file_store.clone();
        let splitter = self.splitter.clone();
        TOKIO_RUNTIME.spawn(async move {
            for file in params.files {
                if let Err(e) = collection
                    .delete_documents(
                        json!({
                            "uri": {
                                "$eq": file.old_uri
                            }
                        })
                        .into(),
                    )
                    .await
                {
                    error!("PGML - Error deleting file: {e:?}");
                }
                if let Err(e) = split_and_upsert_file(
                    &file.new_uri,
                    &mut collection,
                    file_store.clone(),
                    splitter.clone(),
                )
                .await
                {
                    error!("{e:?}")
                }
            }
        });
        Ok(())
    }
}
