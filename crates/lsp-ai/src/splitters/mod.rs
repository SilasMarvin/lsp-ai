use serde::Serialize;

use crate::{config::ValidSplitter, memory_backends::file_store::File};

mod text_splitter;
mod tree_sitter;

#[derive(Serialize)]
pub struct ByteRange {
    pub start_byte: usize,
    pub end_byte: usize,
}

impl ByteRange {
    pub fn new(start_byte: usize, end_byte: usize) -> Self {
        Self {
            start_byte,
            end_byte,
        }
    }
}

#[derive(Serialize)]
pub struct Chunk {
    pub text: String,
    pub range: ByteRange,
}

impl Chunk {
    fn new(text: String, range: ByteRange) -> Self {
        Self { text, range }
    }
}

pub trait Splitter {
    fn split(&self, file: &File) -> Vec<Chunk>;
    fn split_file_contents(&self, uri: &str, contents: &str) -> Vec<Chunk>;

    fn does_use_tree_sitter(&self) -> bool {
        false
    }
}

impl TryFrom<ValidSplitter> for Box<dyn Splitter + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(value: ValidSplitter) -> Result<Self, Self::Error> {
        match value {
            ValidSplitter::TreeSitter(config) => {
                Ok(Box::new(tree_sitter::TreeSitter::new(config)?))
            }
            ValidSplitter::TextSplitter(config) => {
                Ok(Box::new(text_splitter::TextSplitter::new(config)))
            }
        }
    }
}
