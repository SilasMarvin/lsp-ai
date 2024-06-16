use splitter_tree_sitter::TreeSitterCodeSplitter;
use tracing::error;
use tree_sitter::Tree;

use crate::{config, memory_backends::file_store::File, utils::parse_tree};

use super::{ByteRange, Chunk, Splitter};

pub struct TreeSitter {
    _config: config::TreeSitter,
    splitter: TreeSitterCodeSplitter,
}

impl TreeSitter {
    pub fn new(config: config::TreeSitter) -> anyhow::Result<Self> {
        Ok(Self {
            splitter: TreeSitterCodeSplitter::new(config.chunk_size, config.chunk_overlap)?,
            _config: config,
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
                        "Failed to parse tree for file with error {e:?}. Falling back to default splitter.",
                    );
                    todo!()
                }
            }
        } else {
            panic!("TreeSitter splitter requires a tree to split")
        }
    }

    fn split_file_contents(&self, uri: &str, contents: &str) -> Vec<Chunk> {
        match parse_tree(uri, contents, None) {
            Ok(tree) => match self.split_tree(&tree, contents.as_bytes()) {
                Ok(chunks) => chunks,
                Err(e) => {
                    error!(
                            "Failed to parse tree for file: {uri} with error {e:?}. Falling back to default splitter.",
                        );
                    todo!()
                }
            },
            Err(e) => {
                error!(
                    "Failed to parse tree for file {uri} with error {e:?}. Falling back to default splitter.",
                );
                todo!()
            }
        }
    }

    fn does_use_tree_sitter(&self) -> bool {
        true
    }
}
