use std::collections::HashMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, instrument};

use crate::{
    config::{self, ChatMessage},
    memory_backends::Prompt,
    transformer_worker::{
        DoGenerationResponse, DoGenerationStreamResponse, GenerationStreamRequest,
    },
    utils::format_chat_messages,
};

use super::TransformerBackend;

const fn max_tokens_default() -> usize {
    64
}

const fn top_p_default() -> f32 {
    0.95
}

const fn temperature_default() -> f32 {
    0.1
}

// NOTE: We cannot deny unknown fields as the provided parameters may contain other fields relevant to other processes
#[derive(Debug, Deserialize)]
pub(crate) struct AnthropicRunParams {
    system: String,
    #[serde(default)]
    messages: Vec<ChatMessage>,
    #[serde(default = "max_tokens_default")]
    pub(crate) max_tokens: usize,
    #[serde(default = "top_p_default")]
    pub(crate) top_p: f32,
    #[serde(default = "temperature_default")]
    pub(crate) temperature: f32,
}

pub(crate) struct Anthropic {
    config: config::Anthropic,
}

#[derive(Deserialize, Serialize)]
struct AnthropicResponse {
    content: Vec<AnthropicChatMessage>,
}

#[derive(Deserialize, Serialize)]
struct AnthropicChatMessage {
    text: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ChatError {
    error: Value,
}

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum ChatResponse {
    Success(AnthropicResponse),
    Error(ChatError),
    Other(HashMap<String, Value>),
}

impl Anthropic {
    pub(crate) fn new(config: config::Anthropic) -> Self {
        Self { config }
    }

    async fn get_chat(
        &self,
        system_prompt: String,
        messages: Vec<ChatMessage>,
        params: AnthropicRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = if let Some(env_var_name) = &self.config.auth_token_env_var_name {
            std::env::var(env_var_name)?
        } else if let Some(token) = &self.config.auth_token {
            token.to_string()
        } else {
            anyhow::bail!(
                "Please set `auth_token_env_var_name` or `auth_token` to use an Anthropic"
            );
        };
        let params = json!({
            "model": self.config.model,
            "system": system_prompt,
            "max_tokens": params.max_tokens,
            "top_p": params.top_p,
            "temperature": params.temperature,
            "messages": messages
        });
        info!(
            "Calling Anthropic compatible API with parameters:\n{}",
            serde_json::to_string_pretty(&params).unwrap()
        );
        let res: ChatResponse = client
            .post(
                self.config
                    .chat_endpoint
                    .as_ref()
                    .context("must specify `completions_endpoint` to use completions")?,
            )
            .header("x-api-key", token)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&params)
            .send()
            .await?
            .json()
            .await?;
        info!(
            "Response from Anthropic compatible API:\n{}",
            serde_json::to_string_pretty(&res).unwrap()
        );
        match res {
            ChatResponse::Success(mut resp) => Ok(std::mem::take(&mut resp.content[0].text)),
            ChatResponse::Error(error) => {
                anyhow::bail!("making Anthropic request: {:?}", error.error.to_string())
            }
            ChatResponse::Other(other) => {
                anyhow::bail!("unknown error while making Anthropic request: {:?}", other)
            }
        }
    }

    async fn do_get_chat(
        &self,
        prompt: &Prompt,
        params: AnthropicRunParams,
    ) -> anyhow::Result<String> {
        let mut messages = vec![ChatMessage::new(
            "system".to_string(),
            params.system.clone(),
        )];
        messages.extend_from_slice(&params.messages);
        let mut messages = format_chat_messages(&messages, prompt.try_into()?);
        let system_prompt = messages.remove(0).content;
        self.get_chat(system_prompt, messages, params).await
    }
}

#[async_trait::async_trait]
impl TransformerBackend for Anthropic {
    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: AnthropicRunParams = serde_json::from_value(params)?;
        let generated_text = self.do_get_chat(prompt, params).await?;
        Ok(DoGenerationResponse { generated_text })
    }

    #[instrument(skip(self))]
    async fn do_generate_stream(
        &self,
        request: &GenerationStreamRequest,
        _params: Value,
    ) -> anyhow::Result<DoGenerationStreamResponse> {
        anyhow::bail!("GenerationStream is not yet implemented")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::{from_value, json};

    #[tokio::test]
    async fn anthropic_chat_do_generate() -> anyhow::Result<()> {
        let configuration: config::Anthropic = from_value(json!({
            "chat_endpoint": "https://api.anthropic.com/v1/messages",
            "model": "claude-3-haiku-20240307",
            "auth_token_env_var_name": "ANTHROPIC_API_KEY",
        }))?;
        let anthropic = Anthropic::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "system": "Test",
            "messages": [
                {
                    "role": "user",
                    "content": "Test {CONTEXT} - {CODE}"
                }
            ],
            "max_tokens": 2
        });
        let response = anthropic.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
