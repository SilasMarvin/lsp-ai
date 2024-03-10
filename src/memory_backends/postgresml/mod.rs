use std::path::Path;

use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use pgml::{Collection, Pipeline};
use serde_json::json;
use tokio::runtime::Runtime;
use tracing::instrument;

use crate::{
    configuration::{self, Configuration},
    utils::tokens_to_estimated_characters,
};

use super::{file_store::FileStore, MemoryBackend, Prompt, PromptForType};

pub struct PostgresML {
    configuration: Configuration,
    file_store: FileStore,
    collection: Collection,
    pipeline: Pipeline,
    runtime: Runtime,
}

impl PostgresML {
    pub fn new(
        postgresml_config: configuration::PostgresML,
        configuration: Configuration,
    ) -> anyhow::Result<Self> {
        let file_store = FileStore::new(configuration.clone());
        let database_url = if let Some(database_url) = postgresml_config.database_url {
            database_url
        } else {
            std::env::var("PGML_DATABASE_URL")?
        };
        // TODO: Think on the naming of the collection
        // Maybe filter on metadata or I'm not sure
        let collection = Collection::new("test-lsp-ai", Some(database_url))?;
        // TODO: Review the pipeline
        let pipeline = Pipeline::new(
            "v1",
            Some(
                json!({
                    "text": {
                        "splitter": {
                            "model": "recursive_character",
                            "parameters": {
                                "chunk_size": 512,
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
        // Create our own runtime
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        // Add the collection to the pipeline
        let mut task_collection = collection.clone();
        let mut task_pipeline = pipeline.clone();
        runtime.spawn(async move {
            task_collection
                .add_pipeline(&mut task_pipeline)
                .await
                .expect("PGML - Error adding pipeline to collection");
        });
        // Need to crawl the root path and or workspace folders
        // Or set some kind of did crawl for it
        Ok(Self {
            configuration,
            file_store,
            collection,
            pipeline,
            runtime,
        })
    }
}

impl MemoryBackend for PostgresML {
    #[instrument(skip(self))]
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        self.file_store.get_filter_text(position)
    }

    #[instrument(skip(self))]
    fn build_prompt(
        &mut self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt> {
        // This is blocking, but this is ok as we only query for it from the worker when we are actually doing a transform
        let query = self
            .file_store
            .get_characters_around_position(position, 512)?;
        let res = self.runtime.block_on(
            self.collection.vector_search(
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
                &mut self.pipeline,
            ),
        )?;
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
            tokens_to_estimated_characters(self.configuration.get_max_context_length()?);
        let context: String = context
            .chars()
            .take(max_characters - code.chars().count())
            .collect();
        eprintln!("CONTEXT: {}", context);
        eprintln!("CODE: #########{}######", code);
        Ok(Prompt::new(context, code))
    }

    #[instrument(skip(self))]
    fn opened_text_document(
        &mut self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let text = params.text_document.text.clone();
        let path = params.text_document.uri.path().to_owned();
        let mut task_collection = self.collection.clone();
        self.runtime.spawn(async move {
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
                .expect("PGML - Error adding pipeline to collection");
        });
        self.file_store.opened_text_document(params)
    }

    #[instrument(skip(self))]
    fn changed_text_document(
        &mut self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        let path = params.text_document.uri.path().to_owned();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Error reading path: {}", path))?;
        let mut task_collection = self.collection.clone();
        self.runtime.spawn(async move {
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
                .expect("PGML - Error adding pipeline to collection");
        });
        self.file_store.changed_text_document(params)
    }

    #[instrument(skip(self))]
    fn renamed_file(&mut self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        let mut task_collection = self.collection.clone();
        let task_params = params.clone();
        self.runtime.spawn(async move {
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
        self.file_store.renamed_file(params)
    }
}
