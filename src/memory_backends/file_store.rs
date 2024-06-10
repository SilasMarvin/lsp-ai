use anyhow::Context;
use ignore::WalkBuilder;
use indexmap::IndexSet;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tracing::{error, instrument};

use crate::{
    config::{self, Config},
    utils::tokens_to_estimated_characters,
};

use super::{ContextAndCodePrompt, FIMPrompt, MemoryBackend, MemoryRunParams, Prompt, PromptType};

pub struct FileStore {
    config: Config,
    file_store_config: config::FileStore,
    crawled_file_types: Mutex<HashSet<String>>,
    file_map: Mutex<HashMap<String, Rope>>,
    accessed_files: Mutex<IndexSet<String>>,
}

impl FileStore {
    pub fn new(file_store_config: config::FileStore, config: Config) -> anyhow::Result<Self> {
        let s = Self {
            config,
            file_store_config,
            crawled_file_types: Mutex::new(HashSet::new()),
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
        };
        if let Err(e) = s.maybe_do_crawl(None) {
            error!("{e}")
        }
        Ok(s)
    }

    pub fn new_without_crawl(config: Config) -> Self {
        Self {
            config,
            file_store_config: config::FileStore::new_without_crawl(),
            crawled_file_types: Mutex::new(HashSet::new()),
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
        }
    }

    pub fn maybe_do_crawl(&self, triggered_file: Option<String>) -> anyhow::Result<()> {
        match (
            &self.config.client_params.root_uri,
            &self.file_store_config.crawl,
        ) {
            (Some(root_uri), Some(crawl)) => {
                let extension_to_match = triggered_file
                    .map(|tf| {
                        let path = std::path::Path::new(&tf);
                        path.extension().map(|f| f.to_str().map(|f| f.to_owned()))
                    })
                    .flatten()
                    .flatten();

                if let Some(extension_to_match) = &extension_to_match {
                    if self.crawled_file_types.lock().contains(extension_to_match) {
                        return Ok(());
                    }
                }

                if !crawl.all_files && extension_to_match.is_none() {
                    return Ok(());
                }

                if !root_uri.starts_with("file://") {
                    anyhow::bail!("Skipping crawling as root_uri does not begin with file://")
                }

                for result in WalkBuilder::new(&root_uri[7..]).build() {
                    let result = result?;
                    let path = result.path();
                    if !path.is_dir() {
                        if let Some(path_str) = path.to_str() {
                            let insert_uri = format!("file://{path_str}");
                            if self.file_map.lock().contains_key(&insert_uri) {
                                continue;
                            }
                            if crawl.all_files {
                                let contents = std::fs::read_to_string(path)?;
                                self.file_map
                                    .lock()
                                    .insert(insert_uri, Rope::from_str(&contents));
                            } else {
                                match (
                                    path.extension().map(|pe| pe.to_str()).flatten(),
                                    &extension_to_match,
                                ) {
                                    (Some(path_extension), Some(extension_to_match)) => {
                                        if path_extension == extension_to_match {
                                            let contents = std::fs::read_to_string(path)?;
                                            self.file_map
                                                .lock()
                                                .insert(insert_uri, Rope::from_str(&contents));
                                        }
                                    }
                                    _ => continue,
                                }
                            }
                        }
                    }
                }

                if let Some(extension_to_match) = extension_to_match {
                    self.crawled_file_types.lock().insert(extension_to_match);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn get_rope_for_position(
        &self,
        position: &TextDocumentPositionParams,
        characters: usize,
    ) -> anyhow::Result<(Rope, usize)> {
        // Get the rope and set our initial cursor index
        let current_document_uri = position.text_document.uri.to_string();
        let mut rope = self
            .file_map
            .lock()
            .get(&current_document_uri)
            .context("Error file not found")?
            .clone();
        let mut cursor_index = rope.line_to_char(position.position.line as usize)
            + position.position.character as usize;
        // Add to our rope if we need to
        for file in self
            .accessed_files
            .lock()
            .iter()
            .filter(|f| **f != current_document_uri)
        {
            let needed = characters.saturating_sub(rope.len_chars() + 1);
            if needed == 0 {
                break;
            }
            let file_map = self.file_map.lock();
            let r = file_map.get(file).context("Error file not found")?;
            let slice_max = needed.min(r.len_chars() + 1);
            let rope_str_slice = r
                .get_slice(0..slice_max - 1)
                .context("Error getting slice")?
                .to_string();
            rope.insert(0, "\n");
            rope.insert(0, &rope_str_slice);
            cursor_index += slice_max;
        }
        Ok((rope, cursor_index))
    }

    pub fn get_characters_around_position(
        &self,
        position: &TextDocumentPositionParams,
        characters: usize,
    ) -> anyhow::Result<String> {
        let rope = self
            .file_map
            .lock()
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .clone();
        let cursor_index = rope.line_to_char(position.position.line as usize)
            + position.position.character as usize;
        let start = cursor_index.saturating_sub(characters / 2);
        let end = rope
            .len_chars()
            .min(cursor_index + (characters - (cursor_index - start)));
        let rope_slice = rope
            .get_slice(start..end)
            .context("Error getting rope slice")?;
        Ok(rope_slice.to_string())
    }

    pub fn build_code(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: MemoryRunParams,
    ) -> anyhow::Result<Prompt> {
        let (mut rope, cursor_index) =
            self.get_rope_for_position(position, params.max_context_length)?;

        Ok(match prompt_type {
            PromptType::ContextAndCode => {
                if params.messages.is_some() {
                    let max_length = tokens_to_estimated_characters(params.max_context_length);
                    let start = cursor_index.saturating_sub(max_length / 2);
                    let end = rope
                        .len_chars()
                        .min(cursor_index + (max_length - (cursor_index - start)));

                    rope.insert(cursor_index, "<CURSOR>");
                    let rope_slice = rope
                        .get_slice(start..end + "<CURSOR>".chars().count())
                        .context("Error getting rope slice")?;
                    Prompt::ContextAndCode(ContextAndCodePrompt::new(
                        "".to_string(),
                        rope_slice.to_string(),
                    ))
                } else {
                    let start = cursor_index
                        .saturating_sub(tokens_to_estimated_characters(params.max_context_length));
                    let rope_slice = rope
                        .get_slice(start..cursor_index)
                        .context("Error getting rope slice")?;
                    Prompt::ContextAndCode(ContextAndCodePrompt::new(
                        "".to_string(),
                        rope_slice.to_string(),
                    ))
                }
            }
            PromptType::FIM => {
                let max_length = tokens_to_estimated_characters(params.max_context_length);
                let start = cursor_index.saturating_sub(max_length / 2);
                let end = rope
                    .len_chars()
                    .min(cursor_index + (max_length - (cursor_index - start)));
                let prefix = rope
                    .get_slice(start..cursor_index)
                    .context("Error getting rope slice")?;
                let suffix = rope
                    .get_slice(cursor_index..end)
                    .context("Error getting rope slice")?;
                Prompt::FIM(FIMPrompt::new(prefix.to_string(), suffix.to_string()))
            }
        })
    }
}

#[async_trait::async_trait]
impl MemoryBackend for FileStore {
    #[instrument(skip(self))]
    async fn get_filter_text(
        &self,
        position: &TextDocumentPositionParams,
    ) -> anyhow::Result<String> {
        let rope = self
            .file_map
            .lock()
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .clone();
        let line = rope
            .get_line(position.position.line as usize)
            .context("Error getting filter_text")?
            .slice(0..position.position.character as usize)
            .to_string();
        Ok(line)
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: Value,
    ) -> anyhow::Result<Prompt> {
        let params: MemoryRunParams = serde_json::from_value(params)?;
        self.build_code(position, prompt_type, params)
    }

    #[instrument(skip(self))]
    async fn opened_text_document(
        &self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let rope = Rope::from_str(&params.text_document.text);
        let uri = params.text_document.uri.to_string();
        self.file_map.lock().insert(uri.clone(), rope);
        self.accessed_files.lock().shift_insert(0, uri.clone());
        if let Err(e) = self.maybe_do_crawl(Some(uri)) {
            error!("{e}")
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn changed_text_document(
        &self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        let uri = params.text_document.uri.to_string();
        let mut file_map = self.file_map.lock();
        let rope = file_map
            .get_mut(&uri)
            .context("Error trying to get file that does not exist")?;
        for change in params.content_changes {
            // If range is ommitted, text is the new text of the document
            if let Some(range) = change.range {
                let start_index =
                    rope.line_to_char(range.start.line as usize) + range.start.character as usize;
                let end_index =
                    rope.line_to_char(range.end.line as usize) + range.end.character as usize;
                rope.remove(start_index..end_index);
                rope.insert(start_index, &change.text);
            } else {
                *rope = Rope::from_str(&change.text);
            }
        }
        self.accessed_files.lock().shift_insert(0, uri);
        Ok(())
    }

    #[instrument(skip(self))]
    async fn renamed_files(&self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        for file_rename in params.files {
            let mut file_map = self.file_map.lock();
            if let Some(rope) = file_map.remove(&file_rename.old_uri) {
                file_map.insert(file_rename.new_uri, rope);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{
        DidOpenTextDocumentParams, FileRename, Position, Range, RenameFilesParams,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    };
    use serde_json::json;

    fn generate_base_file_store() -> anyhow::Result<FileStore> {
        let config = Config::default_with_file_store_without_models();
        let file_store_config = if let config::ValidMemoryBackend::FileStore(file_store_config) =
            config.config.memory.clone()
        {
            file_store_config
        } else {
            anyhow::bail!("requires a file_store_config")
        };
        FileStore::new(file_store_config, config)
    }

    fn generate_filler_text_document(uri: Option<&str>, text: Option<&str>) -> TextDocumentItem {
        let uri = uri.unwrap_or("file://filler/");
        let text = text.unwrap_or("Here is the document body");
        TextDocumentItem {
            uri: reqwest::Url::parse(uri).unwrap(),
            language_id: "filler".to_string(),
            version: 0,
            text: text.to_string(),
        }
    }

    #[tokio::test]
    async fn can_open_document() -> anyhow::Result<()> {
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: generate_filler_text_document(None, None),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params).await?;
        let file = file_store
            .file_map
            .lock()
            .get("file://filler/")
            .unwrap()
            .clone();
        assert_eq!(file.to_string(), "Here is the document body");
        Ok(())
    }

    #[tokio::test]
    async fn can_rename_document() -> anyhow::Result<()> {
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: generate_filler_text_document(None, None),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params).await?;

        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: "file://filler/".to_string(),
                new_uri: "file://filler2/".to_string(),
            }],
        };
        file_store.renamed_files(params).await?;

        let file = file_store
            .file_map
            .lock()
            .get("file://filler2/")
            .unwrap()
            .clone();
        assert_eq!(file.to_string(), "Here is the document body");
        Ok(())
    }

    #[tokio::test]
    async fn can_change_document() -> anyhow::Result<()> {
        let text_document = generate_filler_text_document(None, None);

        let params = DidOpenTextDocumentParams {
            text_document: text_document.clone(),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params).await?;

        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: text_document.uri.clone(),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 1,
                    },
                    end: Position {
                        line: 0,
                        character: 3,
                    },
                }),
                range_length: None,
                text: "a".to_string(),
            }],
        };
        file_store.changed_text_document(params).await?;
        let file = file_store
            .file_map
            .lock()
            .get("file://filler/")
            .unwrap()
            .clone();
        assert_eq!(file.to_string(), "Hae is the document body");

        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: text_document.uri,
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "abc".to_string(),
            }],
        };
        file_store.changed_text_document(params).await?;
        let file = file_store
            .file_map
            .lock()
            .get("file://filler/")
            .unwrap()
            .clone();
        assert_eq!(file.to_string(), "abc");

        Ok(())
    }

    #[tokio::test]
    async fn can_build_prompt() -> anyhow::Result<()> {
        let text_document = generate_filler_text_document(
            None,
            Some(
                r#"Document Top
Here is a more complicated document

Some text

The end with a trailing new line
"#,
            ),
        );

        // Test basic completion
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: text_document.clone(),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params).await?;

        let prompt = file_store
            .build_prompt(
                &TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: text_document.uri.clone(),
                    },
                    position: Position {
                        line: 0,
                        character: 10,
                    },
                },
                PromptType::ContextAndCode,
                json!({}),
            )
            .await?;
        let prompt: ContextAndCodePrompt = prompt.try_into()?;
        assert_eq!(prompt.context, "");
        assert_eq!("Document T", prompt.code);

        // Test FIM
        let prompt = file_store
            .build_prompt(
                &TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: text_document.uri.clone(),
                    },
                    position: Position {
                        line: 0,
                        character: 10,
                    },
                },
                PromptType::FIM,
                json!({}),
            )
            .await?;
        let prompt: FIMPrompt = prompt.try_into()?;
        assert_eq!(prompt.prompt, r#"Document T"#);
        assert_eq!(
            prompt.suffix,
            r#"op
Here is a more complicated document

Some text

The end with a trailing new line
"#
        );

        // Test chat
        let prompt = file_store
            .build_prompt(
                &TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: text_document.uri.clone(),
                    },
                    position: Position {
                        line: 0,
                        character: 10,
                    },
                },
                PromptType::ContextAndCode,
                json!({
                    "messages": []
                }),
            )
            .await?;
        let prompt: ContextAndCodePrompt = prompt.try_into()?;
        assert_eq!(prompt.context, "");
        let text = r#"Document T<CURSOR>op
Here is a more complicated document

Some text

The end with a trailing new line
"#
        .to_string();
        assert_eq!(text, prompt.code);

        // Test multi-file
        let text_document2 = generate_filler_text_document(
            Some("file://filler2"),
            Some(
                r#"Document Top2
Here is a more complicated document

Some text

The end with a trailing new line
"#,
            ),
        );
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: text_document2.clone(),
        };
        file_store.opened_text_document(params).await?;

        let prompt = file_store
            .build_prompt(
                &TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: text_document.uri.clone(),
                    },
                    position: Position {
                        line: 0,
                        character: 10,
                    },
                },
                PromptType::ContextAndCode,
                json!({}),
            )
            .await?;
        let prompt: ContextAndCodePrompt = prompt.try_into()?;
        assert_eq!(prompt.context, "");
        assert_eq!(format!("{}\nDocument T", text_document2.text), prompt.code);

        Ok(())
    }

    #[tokio::test]
    async fn test_document_cursor_placement_corner_cases() -> anyhow::Result<()> {
        let text_document = generate_filler_text_document(None, Some("test\n"));
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: text_document.clone(),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params).await?;

        // Test chat
        let prompt = file_store
            .build_prompt(
                &TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: text_document.uri.clone(),
                    },
                    position: Position {
                        line: 1,
                        character: 0,
                    },
                },
                PromptType::ContextAndCode,
                json!({"messages": []}),
            )
            .await?;
        let prompt: ContextAndCodePrompt = prompt.try_into()?;
        assert_eq!(prompt.context, "");
        let text = r#"test
<CURSOR>"#
            .to_string();
        assert_eq!(text, prompt.code);

        Ok(())
    }

    //     #[tokio::test]
    //     async fn test_fim_placement_corner_cases() -> anyhow::Result<()> {
    //         let text_document = generate_filler_text_document(None, Some("test\n"));
    //         let params = lsp_types::DidOpenTextDocumentParams {
    //             text_document: text_document.clone(),
    //         };
    //         let file_store = generate_base_file_store()?;
    //         file_store.opened_text_document(params).await?;

    //         // Test FIM
    //         let params = json!({
    //             "fim": {
    //                 "start": "SS",
    //                 "middle": "MM",
    //                 "end": "EE"
    //             }
    //         });
    //         let prompt = file_store
    //             .build_prompt(
    //                 &TextDocumentPositionParams {
    //                     text_document: TextDocumentIdentifier {
    //                         uri: text_document.uri.clone(),
    //                     },
    //                     position: Position {
    //                         line: 1,
    //                         character: 0,
    //                     },
    //                 },
    //                 params,
    //             )
    //             .await?;
    //         assert_eq!(prompt.context, "");
    //         let text = r#"test
    // "#
    //         .to_string();
    //         assert_eq!(text, prompt.code);

    //         Ok(())
    //     }
}
