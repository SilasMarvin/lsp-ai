use splitter_tree_sitter::TreeSitterCodeSplitter;
use tracing::error;
use tree_sitter::Tree;

use crate::{config, memory_backends::file_store::File, utils::parse_tree};

use super::{text_splitter::TextSplitter, ByteRange, Chunk, Splitter};

pub struct TreeSitter {
    chunk_size: usize,
    splitter: TreeSitterCodeSplitter,
    text_splitter: TextSplitter,
}

impl TreeSitter {
    pub fn new(config: config::TreeSitter) -> anyhow::Result<Self> {
        let text_splitter = TextSplitter::new_with_chunk_size(config.chunk_size);
        Ok(Self {
            chunk_size: config.chunk_size,
            splitter: TreeSitterCodeSplitter::new(config.chunk_size, config.chunk_overlap)?,
            text_splitter,
        })
    }

    fn split_tree(&self, tree: &Tree, contents: &[u8]) -> anyhow::Result<Vec<Chunk>> {
        Ok(self
            .splitter
            .split(tree, contents)?
            .into_iter()
            .map(|c| {
                Chunk::new(
                    c.text.to_owned(),
                    ByteRange::new(c.range.start_byte, c.range.end_byte),
                )
            })
            .collect())
    }
}

impl Splitter for TreeSitter {
    fn split(&self, file: &File) -> Vec<Chunk> {
        if let Some(tree) = file.tree() {
            match self.split_tree(tree, file.rope().to_string().as_bytes()) {
                Ok(chunks) => chunks,
                Err(e) => {
                    error!(
                        "Failed to parse tree for file with error: {e:?}. Falling back to default splitter.",
                    );
                    self.text_splitter.split(file)
                }
            }
        } else {
            self.text_splitter.split(file)
        }
    }

    fn split_file_contents(&self, uri: &str, contents: &str) -> Vec<Chunk> {
        match parse_tree(uri, contents, None) {
            Ok(tree) => match self.split_tree(&tree, contents.as_bytes()) {
                Ok(chunks) => chunks,
                Err(e) => {
                    error!(
                            "Failed to parse tree for file: {uri} with error: {e:?}. Falling back to default splitter.",
                        );
                    self.text_splitter.split_file_contents(uri, contents)
                }
            },
            Err(e) => {
                error!(
                    "Failed to parse tree for file {uri} with error: {e:?}. Falling back to default splitter.",
                );
                self.text_splitter.split_file_contents(uri, contents)
            }
        }
    }

    fn does_use_tree_sitter(&self) -> bool {
        true
    }

    fn chunk_size(&self) -> usize {
        self.chunk_size
    }
}
