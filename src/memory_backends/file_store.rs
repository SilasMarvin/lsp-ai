use anyhow::Context;
use lsp_types::TextDocumentPositionParams;
use ropey::Rope;
use std::collections::HashMap;

use crate::configuration::Configuration;

use super::MemoryBackend;

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

    fn build_prompt(&self, position: &TextDocumentPositionParams) -> anyhow::Result<String> {
        let rope = self
            .file_map
            .get(position.text_document.uri.as_str())
            .context("Error file not found")?
            .clone();

        if self.configuration.supports_fim() {
            // We will want to have some kind of infill support we add
            // rope.insert(cursor_index, "<｜fim_hole｜>");
            // rope.insert(0, "<｜fim_start｜>");
            // rope.insert(rope.len_chars(), "<｜fim_end｜>");
            // let prompt = rope.to_string();
            unimplemented!()
        } else {
            // Convert rope to correct prompt for llm
            let cursor_index = rope.line_to_char(position.position.line as usize)
                + position.position.character as usize;

            let start = cursor_index
                .checked_sub(self.configuration.get_maximum_context_length())
                .unwrap_or(0);
            eprintln!("############ {start} - {cursor_index} #############");

            Ok(rope
                .get_slice(start..cursor_index)
                .context("Error getting rope slice")?
                .to_string())
        }
    }

    fn opened_text_document(
        &mut self,
        params: lsp_types::DidOpenTextDocumentParams,
    ) -> anyhow::Result<()> {
        let rope = Rope::from_str(&params.text_document.text);
        self.file_map
            .insert(params.text_document.uri.to_string(), rope);
        Ok(())
    }

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

    fn renamed_file(&mut self, params: lsp_types::RenameFilesParams) -> anyhow::Result<()> {
        for file_rename in params.files {
            if let Some(rope) = self.file_map.remove(&file_rename.old_uri) {
                self.file_map.insert(file_rename.new_uri, rope);
            }
        }
        Ok(())
    }
}
