use std::{
    sync::mpsc::{self, Sender},
    time::Duration,
};

use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use pgml::{Collection, Pipeline};
use serde_json::json;
use tokio::time;
use tracing::instrument;

use crate::{
    config::{self, Config},
    utils::tokens_to_estimated_characters,
};

use super::{file_store::FileStore, MemoryBackend, Prompt, PromptForType};

pub struct PostgresML {
    configuration: Config,
    file_store: FileStore,
    collection: Collection,
    pipeline: Pipeline,
    debounce_tx: Sender<String>,
    added_pipeline: bool,
}

impl PostgresML {
    pub fn new(
        postgresml_config: config::PostgresML,
        configuration: Config,
    ) -> anyhow::Result<Self> {
        let file_store = FileStore::new_without_crawl(configuration.clone());
        let database_url = if let Some(database_url) = postgresml_config.database_url {
            database_url
        } else {
            std::env::var("PGML_DATABASE_URL")?
        };
        // TODO: Think on the naming of the collection
        // Maybe filter on metadata or I'm not sure
        let collection = Collection::new("test-lsp-ai-3", Some(database_url))?;
        // TODO: Review the pipeline
        let pipeline = Pipeline::new(
            "v1",
            Some(
                json!({
                    "text": {
                        "splitter": {
                            "model": "recursive_character",
                            "parameters": {
                                "chunk_size": 1500,
                                "chunk_overlap": 40
                            }
                        },
                        "semantic_search": {
                            "model": "intfloat/e5-small",
                        }
                    }
                })
                .into(),
            ),
        )?;
        // Setup up a debouncer for changed text documents
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let mut task_collection = collection.clone();
        let (debounce_tx, debounce_rx) = mpsc::channel::<String>();
        runtime.spawn(async move {
            let duration = Duration::from_millis(500);
            let mut file_paths = Vec::new();
            loop {
                time::sleep(duration).await;
                let new_paths: Vec<String> = debounce_rx.try_iter().collect();
                if !new_paths.is_empty() {
                    for path in new_paths {
                        if !file_paths.iter().any(|p| *p == path) {
                            file_paths.push(path);
                        }
                    }
                } else {
                    if file_paths.is_empty() {
                        continue;
                    }
                    let documents = file_paths
                        .into_iter()
                        .map(|path| {
                            let text = std::fs::read_to_string(&path)
                                .unwrap_or_else(|_| panic!("Error reading path: {}", path));
                            json!({
                                "id": path,
                                "text": text
                            })
                            .into()
                        })
                        .collect();
                    task_collection
                        .upsert_documents(documents, None)
                        .await
                        .expect("PGML - Error adding pipeline to collection");
                    file_paths = Vec::new();
                }
            }
        });
        Ok(Self {
            configuration,
            file_store,
            collection,
            pipeline,
            debounce_tx,
            added_pipeline: false,
        })
    }
}

#[async_trait::async_trait]
impl MemoryBackend for PostgresML {
    #[instrument(skip(self))]
    async fn get_filter_text(
        &self,
        position: &TextDocumentPositionParams,
    ) -> anyhow::Result<String> {
        self.file_store.get_filter_text(position).await
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt> {
        let query = self
            .file_store
            .get_characters_around_position(position, 512)?;
        let res = self
            .collection
            .vector_search_local(
                json!({
                    "query": {
                        "fields": {
                            "text": {
                                "query": query
                            }
                        },
                    },
                    "limit": 5
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
        let code = self.file_store.build_code(position, prompt_for_type, 512)?;
        let max_characters =
            tokens_to_estimated_characters(self.configuration.get_max_context_length());
        let context: String = context
            .chars()
            .take(max_characters - code.chars().count())
            .collect();
        Ok(Prompt::new(context, code))
    }

    #[instrument(skip(self))]
    async fn opened_text_document(
        &self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let text = params.text_document.text.clone();
        let path = params.text_document.uri.path().to_owned();
        let task_added_pipeline = self.added_pipeline;
        let mut task_collection = self.collection.clone();
        let mut task_pipeline = self.pipeline.clone();
        if !task_added_pipeline {
            task_collection
                .add_pipeline(&mut task_pipeline)
                .await
                .expect("PGML - Error adding pipeline to collection");
        }
        task_collection
            .upsert_documents(
                vec![json!({
                    "id": path,
                    "text": text
                })
                .into()],
                None,
            )
            .await
            .expect("PGML - Error upserting documents");
        self.file_store.opened_text_document(params).await
    }

    #[instrument(skip(self))]
    async fn changed_text_document(
        &self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        let path = params.text_document.uri.path().to_owned();
        self.debounce_tx.send(path)?;
        self.file_store.changed_text_document(params).await
    }

    #[instrument(skip(self))]
    async fn renamed_file(&self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        let mut task_collection = self.collection.clone();
        let task_params = params.clone();
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
            let text = std::fs::read_to_string(&file.new_uri).expect("PGML - Error reading file");
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
        self.file_store.renamed_file(params).await
    }
}
