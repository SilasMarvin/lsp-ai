use std::{
    sync::{
        mpsc::{self, Sender},
        Arc,
    },
    time::Duration,
};

use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use pgml::{Collection, Pipeline};
use serde_json::{json, Value};
use tokio::time;
use tracing::{error, instrument};

use crate::{
    config::{self, Config},
    crawl::Crawl,
    utils::{tokens_to_estimated_characters, TOKIO_RUNTIME},
};

use super::{
    file_store::FileStore, ContextAndCodePrompt, FIMPrompt, MemoryBackend, MemoryRunParams, Prompt,
    PromptType,
};

#[derive(Clone)]
pub struct PostgresML {
    _config: Config,
    file_store: Arc<FileStore>,
    collection: Collection,
    pipeline: Pipeline,
    debounce_tx: Sender<String>,
    crawl: Option<Arc<Mutex<Crawl>>>,
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
        let file_store = Arc::new(FileStore::new(
            config::FileStore::new_without_crawl(),
            configuration.clone(),
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
                    let documents = match file_uris
                        .iter()
                        .map(|uri| {
                            let text = task_file_store
                                .get_file_contents(&uri)
                                .context("Error reading file contents from file_store")?;
                            anyhow::Ok(
                                json!({
                                    "id": uri,
                                    "text": text
                                })
                                .into(),
                            )
                        })
                        .collect()
                    {
                        Ok(documents) => documents,
                        Err(e) => {
                            error!("{e}");
                            continue;
                        }
                    };
                    if let Err(e) = task_collection
                        .upsert_documents(documents, None)
                        .await
                        .context("PGML - Error adding pipeline to collection")
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
        };

        if let Err(e) = s.maybe_do_crawl(None) {
            error!("{e}")
        }
        Ok(s)
    }

    fn maybe_do_crawl(&self, triggered_file: Option<String>) -> anyhow::Result<()> {
        if let Some(crawl) = &self.crawl {
            let mut _collection = self.collection.clone();
            let mut _pipeline = self.pipeline.clone();
            let mut documents: Vec<pgml::types::Json> = vec![];
            crawl.lock().maybe_do_crawl(triggered_file, |path| {
                let uri = format!("file://{path}");
                // This means it has been opened before
                if self.file_store.contains_file(&uri) {
                    return Ok(());
                }
                // Get the contents, split, and upsert it
                let contents = std::fs::read_to_string(path)?;
                documents.push(
                    json!({
                        "id": uri,
                        "text": contents
                    })
                    .into(),
                );
                // Track the size of the documents we have
                // If it is over some amount in bytes, upsert it
                Ok(())
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
        let mut task_collection = self.collection.clone();
        let saved_uri = params.text_document.uri.to_string();
        TOKIO_RUNTIME.spawn(async move {
            let text = params.text_document.text.clone();
            let uri = params.text_document.uri.to_string();
            task_collection
                .upsert_documents(
                    vec![json!({
                        "id": uri,
                        "text": text
                    })
                    .into()],
                    None,
                )
                .await
                .expect("PGML - Error upserting documents");
        });
        if let Err(e) = self.maybe_do_crawl(Some(saved_uri)) {
            error!("{e}")
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
        let mut task_collection = self.collection.clone();
        let task_params = params.clone();
        TOKIO_RUNTIME.spawn(async move {
            for file in task_params.files {
                task_collection
                    .delete_documents(
                        json!({
                            "id": file.old_uri
                        })
                        .into(),
                    )
                    .await
                    .expect("PGML - Error deleting file");
                let text =
                    std::fs::read_to_string(&file.new_uri).expect("PGML - Error reading file");
                task_collection
                    .upsert_documents(
                        vec![json!({
                            "id": file.new_uri,
                            "text": text
                        })
                        .into()],
                        None,
                    )
                    .await
                    .expect("PGML - Error adding pipeline to collection");
            }
        });
        Ok(())
    }
}
