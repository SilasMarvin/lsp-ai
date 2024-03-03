use lsp_server::ResponseError;

use crate::{configuration::ChatMessage, memory_backends::Prompt};

pub trait ToResponseError {
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

pub fn characters_to_estimated_tokens(characters: usize) -> usize {
    characters * 4
}

pub fn format_chat_messages(messages: &Vec<ChatMessage>, prompt: &Prompt) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| ChatMessage {
            role: m.role.to_owned(),
            content: m
                .content
                .replace("{context}", &prompt.context)
                .replace("{code}", &prompt.code),
        })
        .collect()
}
