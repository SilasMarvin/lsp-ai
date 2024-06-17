use crate::{config, memory_backends::file_store::File};

use super::{ByteRange, Chunk, Splitter};

pub struct TextSplitter {
    splitter: text_splitter::TextSplitter<text_splitter::Characters>,
}

impl TextSplitter {
    pub fn new(config: config::TextSplitter) -> Self {
        Self {
            splitter: text_splitter::TextSplitter::new(config.chunk_size),
        }
    }

    pub fn new_with_chunk_size(chunk_size: usize) -> Self {
        Self {
            splitter: text_splitter::TextSplitter::new(chunk_size),
        }
    }
}

impl Splitter for TextSplitter {
    fn split(&self, file: &File) -> Vec<Chunk> {
        self.split_file_contents("", &file.rope().to_string())
    }

    fn split_file_contents(&self, _uri: &str, contents: &str) -> Vec<Chunk> {
        self.splitter
            .chunk_indices(contents)
            .fold(vec![], |mut acc, (start_byte, text)| {
                let end_byte = start_byte + text.len();
                acc.push(Chunk::new(
                    text.to_string(),
                    ByteRange::new(start_byte, end_byte),
                ));
                acc
            })
    }
}
