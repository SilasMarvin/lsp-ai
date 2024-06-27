use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

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

#[derive(Deserialize)]
struct AnthropicChatMessage {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicChatResponse {
    content: Option<Vec<AnthropicChatMessage>>,
    error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
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
        let res: AnthropicChatResponse = client
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
            .json(&json!({
                "model": self.config.model,
                "system": system_prompt,
                "max_tokens": params.max_tokens,
                "top_p": params.top_p,
                "temperature": params.temperature,
                "messages": messages
            }))
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(mut content) = res.content {
            Ok(std::mem::take(&mut content[0].text))
        } else {
            anyhow::bail!(
                "Uknown error while making request to Anthropic: {:?}",
                res.other
            )
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
