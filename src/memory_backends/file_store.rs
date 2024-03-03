use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use ropey::Rope;
use std::collections::HashMap;
use tracing::instrument;

use crate::{configuration::Configuration, utils::characters_to_estimated_tokens};

use super::{MemoryBackend, Prompt, PromptForType};

pub struct FileStore {
    configuration: Configuration,
    file_map: HashMap<String, Rope>,
}

impl FileStore {
    pub fn new(configuration: Configuration) -> Self {
        Self {
            configuration,
            file_map: HashMap::new(),
        }
    }
}

impl MemoryBackend for FileStore {
    #[instrument(skip(self))]
    fn get_filter_text(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        let rope = self
            .file_map
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .clone();
        Ok(rope
            .get_line(position.position.line as usize)
            .context("Error getting filter_text")?
            .to_string())
    }

    #[instrument(skip(self))]
    fn build_prompt(
        &self,
        position: &TextDocumentPositionParams,
        prompt_for_type: PromptForType,
    ) -> anyhow::Result<Prompt> {
        let mut rope = self
            .file_map
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .clone();

        let cursor_index = rope.line_to_char(position.position.line as usize)
            + position.position.character as usize;

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
        let code = match (is_chat_enabled, self.configuration.get_fim()?) {
            r @ (true, _) | r @ (false, Some(_))
                if is_chat_enabled || rope.len_chars() != cursor_index =>
            {
                let max_length = characters_to_estimated_tokens(
                    self.configuration.get_maximum_context_length()?,
                );
                let start = cursor_index.checked_sub(max_length / 2).unwrap_or(0);
                let end = rope
                    .len_chars()
                    .min(cursor_index + (max_length - (cursor_index - start)));

                if is_chat_enabled {
                    rope.insert(cursor_index, "{CURSOR}");
                    let rope_slice = rope
                        .get_slice(start..end + "{CURSOR}".chars().count())
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
                let start = cursor_index
                    .checked_sub(characters_to_estimated_tokens(
                        self.configuration.get_maximum_context_length()?,
                    ))
                    .unwrap_or(0);
                let rope_slice = rope
                    .get_slice(start..cursor_index)
                    .context("Error getting rope slice")?;
                rope_slice.to_string()
            }
        };
        Ok(Prompt::new("".to_string(), code))
    }

    #[instrument(skip(self))]
    fn opened_text_document(
        &mut self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let rope = Rope::from_str(&params.text_document.text);
        self.file_map
            .insert(params.text_document.uri.to_string(), rope);
        Ok(())
    }

    #[instrument(skip(self))]
    fn changed_text_document(
        &mut self,
        params: lsp_types::DidChangeTextDocumentParams,
    ) -> anyhow::Result<()> {
        let rope = self
            .file_map
            .get_mut(params.text_document.uri.as_str())
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
        Ok(())
    }

    #[instrument(skip(self))]
    fn renamed_file(&mut self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        for file_rename in params.files {
            if let Some(rope) = self.file_map.remove(&file_rename.old_uri) {
                self.file_map.insert(file_rename.new_uri, rope);
            }
        }
        Ok(())
    }
}
