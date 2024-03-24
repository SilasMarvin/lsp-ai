use anyhow::Context;
use indexmap::IndexSet;
use lsp_types::TextDocumentPositionParams;
use parking_lot::Mutex;
use ropey::Rope;
use std::{collections::HashMap, sync::Arc};
use tracing::instrument;

use crate::{
    configuration::{self, Configuration},
    utils::tokens_to_estimated_characters,
};

use super::{MemoryBackend, Prompt, PromptForType};

pub struct FileStore {
    crawl: bool,
    configuration: Configuration,
    file_map: Mutex<HashMap<String, Rope>>,
    accessed_files: Mutex<IndexSet<String>>,
}

// TODO: Put some thought into the crawling here. Do we want to have a crawl option where it tries to crawl through all relevant
// files and then when asked for context it loads them in by the most recently accessed? That seems kind of silly honestly, but I could see
// how users who want to use models with massive context lengths would just want their entire project as context for generation tasks
// I'm not sure yet, this is something I need to think through more

// Ok here are some more ideas
// We take a crawl arg which is a bool of true or false for file_store
// If true we crawl until we get to the max_context_length and then we stop crawling
// We keep track of the last opened / changed files, and prioritize those when building the context for our llms

// For memory backends like PostgresML, they will need to take some kind of max_context_length to crawl or something.
// In other words, there needs to be some specification for how much they should be crawling because the limiting happens in the vector_recall
impl FileStore {
    pub fn new(file_store_config: configuration::FileStore, configuration: Configuration) -> Self {
        // TODO: maybe crawl
        Self {
            crawl: file_store_config.crawl,
            configuration,
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
        }
    }

    pub fn new_without_crawl(configuration: Configuration) -> Self {
        Self {
            crawl: false,
            configuration,
            file_map: Mutex::new(HashMap::new()),
            accessed_files: Mutex::new(IndexSet::new()),
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
            let needed = characters.saturating_sub(rope.len_chars());
            if needed == 0 {
                break;
            }
            let file_map = self.file_map.lock();
            let r = file_map.get(file).context("Error file not found")?;
            let slice_max = needed.min(r.len_chars());
            let rope_str_slice = r
                .get_slice(0..slice_max)
                .context("Error getting slice")?
                .to_string();
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
        prompt_for_type: PromptForType,
        max_context_length: usize,
    ) -> anyhow::Result<String> {
        let (mut rope, cursor_index) = self.get_rope_for_position(position, max_context_length)?;

        let is_chat_enabled = match prompt_for_type {
            PromptForType::Completion => self
                .configuration
                .get_chat()?
                .map(|c| c.completion.is_some())
                .unwrap_or(false),
            PromptForType::Generate => self
                .configuration
                .get_chat()?
                .map(|c| c.generation.is_some())
                .unwrap_or(false),
        };

        // We only want to do FIM if the user has enabled it, the cursor is not at the end of the file,
        // and the user has not enabled chat
        Ok(match (is_chat_enabled, self.configuration.get_fim()?) {
            r @ (true, _) | r @ (false, Some(_))
                if is_chat_enabled || rope.len_chars() != cursor_index =>
            {
                let max_length = tokens_to_estimated_characters(max_context_length);
                let start = cursor_index.saturating_sub(max_length / 2);
                let end = rope
                    .len_chars()
                    .min(cursor_index + (max_length - (cursor_index - start)));

                if is_chat_enabled {
                    rope.insert(cursor_index, "<CURSOR>");
                    let rope_slice = rope
                        .get_slice(start..end + "<CURSOR>".chars().count())
                        .context("Error getting rope slice")?;
                    rope_slice.to_string()
                } else {
                    let fim = r.1.unwrap(); // We can unwrap as we know it is some from the match
                    rope.insert(end, &fim.end);
                    rope.insert(cursor_index, &fim.middle);
                    rope.insert(start, &fim.start);
                    let rope_slice = rope
                        .get_slice(
                            start
                                ..end
                                    + fim.start.chars().count()
                                    + fim.middle.chars().count()
                                    + fim.end.chars().count(),
                        )
                        .context("Error getting rope slice")?;
                    rope_slice.to_string()
                }
            }
            _ => {
                let start =
                    cursor_index.saturating_sub(tokens_to_estimated_characters(max_context_length));
                let rope_slice = rope
                    .get_slice(start..cursor_index)
                    .context("Error getting rope slice")?;
                rope_slice.to_string()
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
        Ok(rope
            .get_line(position.position.line as usize)
            .context("Error getting filter_text")?
            .to_string())
    }

    #[instrument(skip(self))]
    async fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt> {
        let code = self.build_code(
            position,
            prompt_for_type,
            self.configuration.get_max_context_length()?,
        )?;
        Ok(Prompt::new("".to_string(), code))
    }

    #[instrument(skip(self))]
    async fn opened_text_document(
        &self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let rope = Rope::from_str(&params.text_document.text);
        let uri = params.text_document.uri.to_string();
        self.file_map.lock().insert(uri.clone(), rope);
        self.accessed_files.lock().shift_insert(0, uri);
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
    async fn renamed_file(&self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        for file_rename in params.files {
            let mut file_map = self.file_map.lock();
            if let Some(rope) = file_map.remove(&file_rename.old_uri) {
                file_map.insert(file_rename.new_uri, rope);
            }
        }
        Ok(())
    }
}
