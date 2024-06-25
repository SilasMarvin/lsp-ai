use anyhow::Context;
use indexmap::IndexSet;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::Value;
use std::{collections::HashMap, io::Read};
use tracing::{error, instrument, warn};
use tree_sitter::{InputEdit, Point, Tree};

use crate::{
    config::{self, Config},
    crawl::Crawl,
    utils::{parse_tree, tokens_to_estimated_characters},
};

use super::{ContextAndCodePrompt, FIMPrompt, MemoryBackend, MemoryRunParams, Prompt, PromptType};

#[derive(Default)]
pub struct AdditionalFileStoreParams {
    build_tree: bool,
}

impl AdditionalFileStoreParams {
    pub fn new(build_tree: bool) -> Self {
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

pub struct FileStore {
    params: AdditionalFileStoreParams,
    file_map: Mutex<HashMap<String, File>>,
    accessed_files: Mutex<IndexSet<String>>,
    crawl: Option<Mutex<Crawl>>,
}

impl FileStore {
    pub fn new(mut file_store_config: config::FileStore, config: Config) -> anyhow::Result<Self> {
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
        if let Err(e) = s.maybe_do_crawl(None) {
            error!("{e:?}")
        }
        Ok(s)
    }

    pub fn new_with_params(
        mut file_store_config: config::FileStore,
        config: Config,
        params: AdditionalFileStoreParams,
    ) -> anyhow::Result<Self> {
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
        if let Err(e) = s.maybe_do_crawl(None) {
            error!("{e:?}")
        }
        Ok(s)
    }

    fn add_new_file(&self, uri: &str, contents: String) {
        let tree = if self.params.build_tree {
            match parse_tree(uri, &contents, None) {
                Ok(tree) => Some(tree),
                Err(e) => {
                    error!(
                        "Failed to parse tree for {uri} with error {e}, falling back to no tree"
                    );
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

    fn maybe_do_crawl(&self, triggered_file: Option<String>) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<(Rope, usize)> {
        // Get the rope and set our initial cursor index
        let current_document_uri = position.text_document.uri.to_string();
        let mut rope = self
            .file_map
            .lock()
            .get(&current_document_uri)
            .context("Error file not found")?
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
            let r = &file_map.get(file).context("Error file not found")?.rope;
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
            .rope
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
        pull_from_multiple_files: bool,
    ) -> anyhow::Result<Prompt> {
        let (mut rope, cursor_index) =
            self.get_rope_for_position(position, params.max_context, pull_from_multiple_files)?;

        Ok(match prompt_type {
            PromptType::ContextAndCode => {
                if params.is_for_chat {
                    let max_length = tokens_to_estimated_characters(params.max_context);
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
                        .saturating_sub(tokens_to_estimated_characters(params.max_context));
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
                let max_length = tokens_to_estimated_characters(params.max_context);
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

    pub fn file_map(&self) -> &Mutex<HashMap<String, File>> {
        &self.file_map
    }

    pub fn contains_file(&self, uri: &str) -> bool {
        self.file_map.lock().contains_key(uri)
    }

    pub fn position_to_byte(&self, position: &TextDocumentPositionParams) -> anyhow::Result<usize> {
        let file_map = self.file_map.lock();
        let uri = position.text_document.uri.to_string();
        let file = file_map
            .get(&uri)
            .with_context(|| format!("trying to get file that does not exist {uri}"))?;
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
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        let rope = self
            .file_map
            .lock()
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .rope
            .clone();
        let line = rope
            .get_line(position.position.line as usize)
            .context("Error getting filter text")?
            .get_slice(0..position.position.character as usize)
            .context("Error getting filter text")?
            .to_string();
        Ok(line)
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_type: PromptType,
        params: &Value,
    ) -> anyhow::Result<Prompt> {
        let params: MemoryRunParams = params.try_into()?;
        self.build_code(position, prompt_type, params, true)
    }

    #[instrument(skip(self))]
    fn opened_text_document(
        &self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let uri = params.text_document.uri.to_string();
        self.add_new_file(&uri, params.text_document.text);
        if let Err(e) = self.maybe_do_crawl(Some(uri)) {
            error!("{e:?}")
        }
        Ok(())
    }

    #[instrument(skip(self))]
    fn changed_text_document(
        &self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        let uri = params.text_document.uri.to_string();
        let mut file_map = self.file_map.lock();
        let file = file_map
            .get_mut(&uri)
            .with_context(|| format!("Trying to get file that does not exist {uri}"))?;
        for change in params.content_changes {
            // If range is ommitted, text is the new text of the document
            if let Some(range) = change.range {
                // Record old positions
                let (old_end_position, old_end_byte) = {
                    let last_line_index = file.rope.len_lines() - 1;
                    (
                        file.rope
                            .get_line(last_line_index)
                            .context("getting last line for edit")
                            .map(|last_line| Point::new(last_line_index, last_line.len_chars())),
                        file.rope.bytes().count(),
                    )
                };
                // Update the document
                let start_index = file.rope.line_to_char(range.start.line as usize)
                    + range.start.character as usize;
                let end_index =
                    file.rope.line_to_char(range.end.line as usize) + range.end.character as usize;
                file.rope.remove(start_index..end_index);
                file.rope.insert(start_index, &change.text);
                // Set new end positions
                let (new_end_position, new_end_byte) = {
                    let last_line_index = file.rope.len_lines() - 1;
                    (
                        file.rope
                            .get_line(last_line_index)
                            .context("getting last line for edit")
                            .map(|last_line| Point::new(last_line_index, last_line.len_chars())),
                        file.rope.bytes().count(),
                    )
                };
                // Update the tree
                if self.params.build_tree {
                    let mut old_tree = file.tree.take();
                    let start_byte = file
                        .rope
                        .try_line_to_char(range.start.line as usize)
                        .and_then(|start_char| {
                            file.rope
                                .try_char_to_byte(start_char + range.start.character as usize)
                        })
                        .map_err(anyhow::Error::msg);
                    if let Some(old_tree) = &mut old_tree {
                        match (start_byte, old_end_position, new_end_position) {
                            (Ok(start_byte), Ok(old_end_position), Ok(new_end_position)) => {
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
                                file.tree = match parse_tree(
                                    &uri,
                                    &file.rope.to_string(),
                                    Some(old_tree),
                                ) {
                                    Ok(tree) => Some(tree),
                                    Err(e) => {
                                        error!("failed to edit tree: {e:?}");
                                        None
                                    }
                                };
                            }
                            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                                error!("failed to build tree edit: {e:?}");
                            }
                        }
                    }
                }
            } else {
                file.rope = Rope::from_str(&change.text);
                if self.params.build_tree {
                    file.tree = match parse_tree(&uri, &change.text, None) {
                        Ok(tree) => Some(tree),
                        Err(e) => {
                            error!("failed to parse new tree: {e:?}");
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
    fn renamed_files(&self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        for file_rename in params.files {
            let mut file_map = self.file_map.lock();
            if let Some(rope) = file_map.remove(&file_rename.old_uri) {
                file_map.insert(file_rename.new_uri, rope);
            }
        }
        Ok(())
    }
}

// For teesting use only
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
