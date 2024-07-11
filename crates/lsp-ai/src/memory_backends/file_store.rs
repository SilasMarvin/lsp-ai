use anyhow::Context;
use indexmap::IndexSet;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::Value;
use std::{collections::HashMap, io::Read};
use tracing::{error, instrument, warn};
use tree_sitter::{InputEdit, Point, Tree};
use utils_tree_sitter::parse_tree;

use crate::{
    config::{self, Config, FileStore},
    crawl::Crawl,
    utils::tokens_to_estimated_characters,
};

use super::{
    ContextAndCodePrompt, FIMPrompt, MemoryBackend, MemoryBackendError, MemoryRunParams, Prompt,
    PromptType, Result,
};

#[derive(Default)]
pub(crate) struct AdditionalFileStoreParams {
    build_tree: bool,
}

impl AdditionalFileStoreParams {
    pub(crate) fn new(build_tree: bool) -> Self {
        Self { build_tree }
    }
}

#[derive(Clone)]
pub struct File {
    rope: Rope,
    tree: Option<Tree>,
}

impl File {
    fn new(rope: Rope, tree: Option<Tree>) -> Self {
        Self { rope, tree }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }
}

pub(crate) struct FileStore {
    params: AdditionalFileStoreParams,
    file_map: Mutex<HashMap<String, File>>,
    accessed_files: Mutex<IndexSet<String>>,
    crawl: Option<Mutex<Crawl>>,
}

impl FileStore {
    pub(crate) fn new(mut file_store_config: config::FileStore, config: Config) -> Result<Self> {
        let crawl = file_store_config
            .crawl
            .take()
            .map(|x| Mutex::new(Crawl::new(x, config.clone())));
        let s = Self {
            params: AdditionalFileStoreParams::default(),
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
            crawl,
        };
        s.maybe_do_crawl(None)?;
        Ok(s)
    }

    pub(crate) fn new_with_params(
        mut file_store_config: config::FileStore,
        config: Config,
        params: AdditionalFileStoreParams,
    ) -> Result<Self> {
        let crawl = file_store_config
            .crawl
            .take()
            .map(|x| Mutex::new(Crawl::new(x, config.clone())));
        let s = Self {
            params,
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
            crawl,
        };
        s.maybe_do_crawl(None)?;
        Ok(s)
    }

    fn add_new_file(&self, uri: &str, contents: String) {
        let tree = if self.params.build_tree {
            match parse_tree(uri, &contents, None) {
                Ok(tree) => Some(tree),
                Err(e) => {
                    error!("Failed to parse tree, falling back to no tree. err: {e}");
                    None
                }
            }
        } else {
            None
        };
        self.file_map
            .lock()
            .insert(uri.to_string(), File::new(Rope::from_str(&contents), tree));
        self.accessed_files.lock().insert(uri.to_string());
    }

    fn maybe_do_crawl(&self, triggered_file: Option<String>) -> Result<()> {
        let mut total_bytes = 0;
        let mut current_bytes = 0;
        if let Some(crawl) = &self.crawl {
            crawl
                .lock()
                .maybe_do_crawl(triggered_file, |config, path| {
                    // Break if total bytes is over the max crawl memory
                    if total_bytes as u64 >= config.max_crawl_memory {
                        warn!("Ending crawl early due to `max_crawl_memory` resetraint");
                        return Ok(false);
                    }
                    // This means it has been opened before
                    let insert_uri = format!("file:///{path}");
                    if self.file_map.lock().contains_key(&insert_uri) {
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
                    self.add_new_file(&insert_uri, contents);
                    Ok(true)
                })?;
        }
        Ok(())
    }

    fn get_rope_for_position(
        &self,
        position: &TextDocumentPositionParams,
        characters: usize,
        pull_from_multiple_files: bool,
    ) -> Result<(Rope, usize)> {
        // Get the rope and set our initial cursor index
        let current_document_uri = position.text_document.uri.to_string();
        let mut rope = self
            .file_map
            .lock()
            .get(&current_document_uri)
            .ok_or(MemoryBackendError::FileNotFound(current_document_uri))?
            .rope
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
            if needed == 0 || !pull_from_multiple_files {
                break;
            }
            let file_map = self.file_map.lock();
            let r = &file_map
                .get(file)
                .ok_or_else(|| MemoryBackendError::FileNotFound(file.to_owned()))?
                .rope;
            let slice_max = needed.min(r.len_chars() + 1);
            let rope_str_slice = r
                .get_slice(0..slice_max - 1)
                .ok_or(MemoryBackendError::SliceRangeOutOfBounds(0, slice_max - 1))?
                .to_string();
            rope.insert(0, "\n");
            rope.insert(0, &rope_str_slice);
            cursor_index += slice_max;
        }
        Ok((rope, cursor_index))
    }

    pub(crate) fn get_characters_around_position(
        &self,
        position: &TextDocumentPositionParams,
        characters: usize,
    ) -> Result<String> {
        let rope = self
            .file_map
            .lock()
            .get(position.text_document.uri.as_str())
            .ok_or_else(|| {
                MemoryBackendError::FileNotFound(position.text_document.uri.to_string())
            })?
            .rope
            .clone();
        let cursor_index = rope.try_line_to_char(position.position.line as usize)?
            + position.position.character as usize;
        let start = cursor_index.saturating_sub(characters / 2);
        let end = rope
            .len_chars()
            .min(cursor_index + (characters - (cursor_index - start)));
        let rope_slice = rope
            .get_slice(start..end)
            .ok_or(MemoryBackendError::SliceRangeOutOfBounds(start, end))?;
        Ok(rope_slice.to_string())
    }

    pub(crate) fn build_code(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: MemoryRunParams,
        pull_from_multiple_files: bool,
    ) -> Result<Prompt> {
        let (mut rope, cursor_index) =
            self.get_rope_for_position(position, params.max_context, pull_from_multiple_files)?;

        Ok(match prompt_type {
            PromptType::ContextAndCode => {
                if params.is_for_chat {
                    let max_length = tokens_to_estimated_characters(params.max_context);
                    let start = cursor_index.saturating_sub(max_length / 2);
                    let end = rope
                        .len_chars()
                        .min(cursor_index + (max_length - (cursor_index - start)))
                        + "<CURSOR>".chars().count();

                    rope.try_insert(cursor_index, "<CURSOR>")?;
                    let rope_slice = rope
                        .get_slice(start..end)
                        .ok_or(MemoryBackendError::SliceRangeOutOfBounds(start, end))?;
                    Prompt::ContextAndCode(ContextAndCodePrompt::new(
                        "".to_string(),
                        rope_slice.to_string(),
                    ))
                } else {
                    let start = cursor_index
                        .saturating_sub(tokens_to_estimated_characters(params.max_context));
                    let rope_slice = rope.get_slice(start..cursor_index).ok_or(
                        MemoryBackendError::SliceRangeOutOfBounds(start, cursor_index),
                    )?;
                    Prompt::ContextAndCode(ContextAndCodePrompt::new(
                        "".to_string(),
                        rope_slice.to_string(),
                    ))
                }
            }
            PromptType::FIM => {
                let max_length = tokens_to_estimated_characters(params.max_context);
                let start = cursor_index.saturating_sub(max_length / 2);
                let end = rope
                    .len_chars()
                    .min(cursor_index + (max_length - (cursor_index - start)));
                let prefix = rope.get_slice(start..cursor_index).ok_or(
                    MemoryBackendError::SliceRangeOutOfBounds(start, cursor_index),
                )?;
                let suffix = rope
                    .get_slice(cursor_index..end)
                    .ok_or(MemoryBackendError::SliceRangeOutOfBounds(cursor_index, end))?;
                Prompt::FIM(FIMPrompt::new(prefix.to_string(), suffix.to_string()))
            }
        })
    }

    pub(crate) fn file_map(&self) -> &Mutex<HashMap<String, File>> {
        &self.file_map
    }

    pub(crate) fn contains_file(&self, uri: &str) -> bool {
        self.file_map.lock().contains_key(uri)
    }

    pub(crate) fn position_to_byte(&self, position: &TextDocumentPositionParams) -> Result<usize> {
        let file_map = self.file_map.lock();
        let uri = position.text_document.uri.to_string();
        let file = file_map
            .get(&uri)
            .ok_or(MemoryBackendError::FileNotFound(uri))?;
        let line_char_index = file
            .rope
            .try_line_to_char(position.position.line as usize)?;
        Ok(file
            .rope
            .try_char_to_byte(line_char_index + position.position.character as usize)?)
    }
}

#[async_trait::async_trait]
impl MemoryBackend for FileStore {
    #[instrument(skip(self))]
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> Result<String> {
        let rope = self
            .file_map
            .lock()
            .get(position.text_document.uri.as_str())
            .ok_or_else(|| {
                MemoryBackendError::FileNotFound(position.text_document.uri.to_string())
            })?
            .rope
            .clone();
        let line = rope
            .get_line(position.position.line as usize)
            .ok_or(MemoryBackendError::LineOutOfBounds(
                position.position.line as usize,
            ))?
            .get_slice(0..position.position.character as usize)
            .ok_or(MemoryBackendError::SliceRangeOutOfBounds(
                0,
                position.position.character as usize,
            ))?
            .to_string();
        Ok(line)
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> Result<Prompt> {
        let params: MemoryRunParams = params.into();
        self.build_code(position, prompt_type, params, true)
    }

    #[instrument(skip(self))]
    fn opened_text_document(&self, params: lsp_types::DidOpenTextDocumentParams) -> Result<()> {
        let uri = params.text_document.uri.to_string();
        self.add_new_file(&uri, params.text_document.text);
        self.maybe_do_crawl(Some(uri))?;
        Ok(())
    }

    #[instrument(skip(self))]
    fn changed_text_document(&self, params: lsp_types::DidChangeTextDocumentParams) -> Result<()> {
        let uri = params.text_document.uri.to_string();
        let mut file_map = self.file_map.lock();
        let file = file_map
            .get_mut(&uri)
            .ok_or(MemoryBackendError::FileNotFound(uri))?;
        for change in params.content_changes {
            // If range is ommitted, text is the new text of the document
            if let Some(range) = change.range {
                // Record old positions
                let (old_end_position, old_end_byte) = {
                    let last_line_index = file.rope.len_lines() - 1;
                    let last_line = file
                        .rope
                        .get_line(last_line_index)
                        .ok_or(MemoryBackendError::LineOutOfBounds(last_line_index))?;
                    (
                        Point::new(last_line_index, last_line.len_chars()),
                        file.rope.bytes().count(),
                    )
                };
                // Update the document
                let start_index = file.rope.try_line_to_char(range.start.line as usize)?
                    + range.start.character as usize;
                let end_index = file.rope.try_line_to_char(range.end.line as usize)?
                    + range.end.character as usize;
                file.rope.try_remove(start_index..end_index)?;
                file.rope.try_insert(start_index, &change.text)?;
                // Set new end positions
                let (new_end_position, new_end_byte) = {
                    let last_line_index = file.rope.len_lines() - 1;
                    let last_line = file
                        .rope
                        .get_line(last_line_index)
                        .ok_or(MemoryBackendError::LineOutOfBounds(last_line_index))?;
                    (
                        Point::new(last_line_index, last_line.len_chars()),
                        file.rope.bytes().count(),
                    )
                };
                // Update the tree
                if self.params.build_tree {
                    let mut old_tree = file.tree.take();
                    let start_char = file.rope.try_line_to_char(range.start.line as usize)?;
                    let start_byte = file
                        .rope
                        .try_char_to_byte(start_char + range.start.character as usize)?;
                    if let Some(old_tree) = &mut old_tree {
                        old_tree.edit(&InputEdit {
                            start_byte,
                            old_end_byte,
                            new_end_byte,
                            start_position: Point::new(
                                range.start.line as usize,
                                range.start.character as usize,
                            ),
                            old_end_position,
                            new_end_position,
                        });
                        file.tree = match parse_tree(&uri, &file.rope.to_string(), Some(old_tree)) {
                            Ok(tree) => Some(tree),
                            Err(e) => {
                                error!("failed to edit tree: {e}");
                                None
                            }
                        };
                    }
                }
            } else {
                file.rope = Rope::from_str(&change.text);
                if self.params.build_tree {
                    file.tree = match parse_tree(&uri, &change.text, None) {
                        Ok(tree) => Some(tree),
                        Err(e) => {
                            error!("failed to parse new tree: {e}");
                            None
                        }
                    };
                }
            }
        }
        self.accessed_files.lock().shift_insert(0, uri);
        Ok(())
    }

    #[instrument(skip(self))]
    fn renamed_files(&self, params: lsp_types::RenameFilesParams) -> Result<()> {
        for file_rename in params.files {
            let mut file_map = self.file_map.lock();
            if let Some(rope) = file_map.try_remove(&file_rename.old_uri)? {
                file_map.try_insert(file_rename.new_uri, rope)?;
            }
        }
        Ok(())
    }
}

// For testing use only
#[cfg(test)]
impl FileStore {
    pub fn default_with_filler_file() -> anyhow::Result<Self> {
        let config = Config::default_with_file_store_without_models();
        let file_store_config = if let config::ValidMemoryBackend::FileStore(file_store_config) =
            config.config.memory.clone()
        {
            file_store_config
        } else {
            anyhow::bail!("requires a file_store_config")
        };
        let f = FileStore::new(file_store_config, config)?;

        let uri = "file:///filler.py";
        let text = r#"# Multiplies two numbers
def multiply_two_numbers(x, y):
    return

# A singular test
assert multiply_two_numbers(2, 3) == 6
"#;
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: lsp_types::TextDocumentItem {
                uri: reqwest::Url::parse(uri).unwrap(),
                language_id: "filler".to_string(),
                version: 0,
                text: text.to_string(),
            },
        };
        f.opened_text_document(params)?;

        Ok(f)
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
        let uri = uri.unwrap_or("file:///filler/");
        let text = text.unwrap_or("Here is the document body");
        TextDocumentItem {
            uri: reqwest::Url::parse(uri).unwrap(),
            language_id: "filler".to_string(),
            version: 0,
            text: text.to_string(),
        }
    }

    #[test]
    fn can_open_document() -> anyhow::Result<()> {
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: generate_filler_text_document(None, None),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params)?;
        let file = file_store
            .file_map
            .lock()
            .get("file:///filler/")
            .unwrap()
            .clone();
        assert_eq!(file.rope.to_string(), "Here is the document body");
        Ok(())
    }

    #[test]
    fn can_rename_document() -> anyhow::Result<()> {
        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: generate_filler_text_document(None, None),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params)?;

        let params = RenameFilesParams {
            files: vec![FileRename {
                old_uri: "file:///filler/".to_string(),
                new_uri: "file:///filler2/".to_string(),
            }],
        };
        file_store.renamed_files(params)?;

        let file = file_store
            .file_map
            .lock()
            .get("file:///filler2/")
            .unwrap()
            .clone();
        assert_eq!(file.rope.to_string(), "Here is the document body");
        Ok(())
    }

    #[test]
    fn can_change_document() -> anyhow::Result<()> {
        let text_document = generate_filler_text_document(None, None);

        let params = DidOpenTextDocumentParams {
            text_document: text_document.clone(),
        };
        let file_store = generate_base_file_store()?;
        file_store.opened_text_document(params)?;

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
        file_store.changed_text_document(params)?;
        let file = file_store
            .file_map
            .lock()
            .get("file:///filler/")
            .unwrap()
            .clone();
        assert_eq!(file.rope.to_string(), "Hae is the document body");

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
        file_store.changed_text_document(params)?;
        let file = file_store
            .file_map
            .lock()
            .get("file:///filler/")
            .unwrap()
            .clone();
        assert_eq!(file.rope.to_string(), "abc");

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
        file_store.opened_text_document(params)?;

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
                &json!({}),
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
                &json!({}),
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
                &json!({
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
            Some("file:///filler2"),
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
        file_store.opened_text_document(params)?;

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
                &json!({}),
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
        file_store.opened_text_document(params)?;

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
                &json!({"messages": []}),
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

    #[test]
    fn test_file_store_tree_sitter() -> anyhow::Result<()> {
        crate::init_logger();

        let config = Config::default_with_file_store_without_models();
        let file_store_config = if let config::ValidMemoryBackend::FileStore(file_store_config) =
            config.config.memory.clone()
        {
            file_store_config
        } else {
            anyhow::bail!("requires a file_store_config")
        };
        let params = AdditionalFileStoreParams { build_tree: true };
        let file_store = FileStore::new_with_params(file_store_config, config, params)?;

        let uri = "file:///filler/test.rs";
        let text = r#"#[derive(Debug)]
struct Rectangle {
    width: u32,
    height: u32,
}

impl Rectangle {
    fn area(&self) -> u32 {

    }
}

fn main() {
    let rect1 = Rectangle {
        width: 30,
        height: 50,
    };

    println!(
        "The area of the rectangle is {} square pixels.",
        rect1.area()
    );
}"#;
        let text_document = TextDocumentItem {
            uri: reqwest::Url::parse(uri).unwrap(),
            language_id: "".to_string(),
            version: 0,
            text: text.to_string(),
        };
        let params = DidOpenTextDocumentParams {
            text_document: text_document.clone(),
        };

        file_store.opened_text_document(params)?;

        // Test insert
        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: text_document.uri.clone(),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 8,
                        character: 0,
                    },
                    end: Position {
                        line: 8,
                        character: 0,
                    },
                }),
                range_length: None,
                text: "        self.width * self.height".to_string(),
            }],
        };
        file_store.changed_text_document(params)?;
        let file = file_store.file_map.lock().get(uri).unwrap().clone();
        assert_eq!(file.tree.unwrap().root_node().to_sexp(), "(source_file (attribute_item (attribute (identifier) arguments: (token_tree (identifier)))) (struct_item name: (type_identifier) body: (field_declaration_list (field_declaration name: (field_identifier) type: (primitive_type)) (field_declaration name: (field_identifier) type: (primitive_type)))) (impl_item type: (type_identifier) body: (declaration_list (function_item name: (identifier) parameters: (parameters (self_parameter (self))) return_type: (primitive_type) body: (block (binary_expression left: (field_expression value: (self) field: (field_identifier)) right: (field_expression value: (self) field: (field_identifier))))))) (function_item name: (identifier) parameters: (parameters) body: (block (let_declaration pattern: (identifier) value: (struct_expression name: (type_identifier) body: (field_initializer_list (field_initializer field: (field_identifier) value: (integer_literal)) (field_initializer field: (field_identifier) value: (integer_literal))))) (expression_statement (macro_invocation macro: (identifier) (token_tree (string_literal (string_content)) (identifier) (identifier) (token_tree)))))))");

        // Test delete
        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: text_document.uri.clone(),
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 12,
                        character: 0,
                    },
                }),
                range_length: None,
                text: "".to_string(),
            }],
        };
        file_store.changed_text_document(params)?;
        let file = file_store.file_map.lock().get(uri).unwrap().clone();
        assert_eq!(file.tree.unwrap().root_node().to_sexp(), "(source_file (function_item name: (identifier) parameters: (parameters) body: (block (let_declaration pattern: (identifier) value: (struct_expression name: (type_identifier) body: (field_initializer_list (field_initializer field: (field_identifier) value: (integer_literal)) (field_initializer field: (field_identifier) value: (integer_literal))))) (expression_statement (macro_invocation macro: (identifier) (token_tree (string_literal (string_content)) (identifier) (identifier) (token_tree)))))))");

        // Test replace
        let params = lsp_types::DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: text_document.uri,
                version: 1,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "fn main() {}".to_string(),
            }],
        };
        file_store.changed_text_document(params)?;
        let file = file_store.file_map.lock().get(uri).unwrap().clone();
        assert_eq!(file.tree.unwrap().root_node().to_sexp(), "(source_file (function_item name: (identifier) parameters: (parameters) body: (block)))");

        Ok(())
    }
}
