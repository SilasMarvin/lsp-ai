use anyhow::Context;
use lsp_server::ResponseError;
use once_cell::sync::Lazy;
use tokio::runtime;
use tree_sitter::Tree;

use crate::{config::ChatMessage, memory_backends::ContextAndCodePrompt, splitters::Chunk};

pub(crate) static TOKIO_RUNTIME: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Error building tokio runtime")
});

pub(crate) trait ToResponseError {
    fn to_response_error(&self, code: i32) -> ResponseError;
}

impl ToResponseError for anyhow::Error {
    fn to_response_error(&self, code: i32) -> ResponseError {
        ResponseError {
            code,
            message: self.to_string(),
            data: None,
        }
    }
}

pub(crate) fn tokens_to_estimated_characters(tokens: usize) -> usize {
    tokens * 4
}

pub(crate) fn format_chat_messages(
    messages: &[ChatMessage],
    prompt: &ContextAndCodePrompt,
) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| ChatMessage::new(m.role.to_owned(), format_prompt_in_str(&m.content, &prompt)))
        .collect()
}

pub(crate) fn format_prompt_in_str(s: &str, prompt: &ContextAndCodePrompt) -> String {
    s.replace("{CONTEXT}", &prompt.context)
        .replace("{CODE}", &prompt.code)
        .replace(
            "{SELECTED_TEXT}",
            prompt
                .selected_text
                .as_ref()
                .map(|x| x.as_str())
                .unwrap_or_default(),
        )
}

pub(crate) fn format_prompt(prompt: &ContextAndCodePrompt) -> String {
    format!("{}\n\n{}", &prompt.context, &prompt.code)
}

pub(crate) fn chunk_to_id(uri: &str, chunk: &Chunk) -> String {
    format!("{uri}#{}-{}", chunk.range.start_byte, chunk.range.end_byte)
}

pub(crate) fn parse_tree(
    uri: &str,
    contents: &str,
    old_tree: Option<&Tree>,
) -> anyhow::Result<Tree> {
    let path = std::path::Path::new(uri);
    let extension = path.extension().map(|x| x.to_string_lossy());
    let extension = extension.as_deref().unwrap_or("");
    let mut parser = utils_tree_sitter::get_parser_for_extension(extension)?;
    parser
        .parse(contents, old_tree)
        .with_context(|| format!("parsing tree failed for {uri}"))
}

pub(crate) fn format_file_chunk(uri: &str, excerpt: &str, root_uri: Option<&str>) -> String {
    let path = match root_uri {
        Some(root_uri) => {
            if uri.starts_with(root_uri) {
                &uri[root_uri.chars().count()..]
            } else {
                uri
            }
        }
        None => uri,
    };
    format!(
        r#"--{path}--
{excerpt}"#,
    )
}
