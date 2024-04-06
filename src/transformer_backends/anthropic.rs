use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

use crate::{
    config::{self, ChatMessage},
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
    utils::format_chat_messages,
};

use super::TransformerBackend;

pub struct Anthropic {
    configuration: config::Anthropic,
}

#[derive(Deserialize)]
struct AnthropicChatMessage {
    text: String,
}

#[derive(Deserialize)]
struct AnthropicChatResponse {
    content: Option<Vec<AnthropicChatMessage>>,
    error: Option<Value>,
}

impl Anthropic {
    #[instrument]
    pub fn new(configuration: config::Anthropic) -> Self {
        Self { configuration }
    }

    async fn get_chat(
        &self,
        system_prompt: String,
        messages: Vec<ChatMessage>,
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = if let Some(env_var_name) = &self.configuration.auth_token_env_var_name {
            std::env::var(env_var_name)?
        } else if let Some(token) = &self.configuration.auth_token {
            token.to_string()
        } else {
            anyhow::bail!("Please set `auth_token_env_var_name` or `auth_token` in `transformer->anthropic` to use an Anthropic");
        };
        let res: AnthropicChatResponse = client
            .post(
                self.configuration
                    .chat_endpoint
                    .as_ref()
                    .context("must specify `completions_endpoint` to use completions")?,
            )
            .header("x-api-key", token)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "system": system_prompt,
                "max_tokens": max_tokens,
                "top_p": self.configuration.top_p,
                "temperature": self.configuration.temperature,
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
            anyhow::bail!("Uknown error while making request to OpenAI")
        }
    }

    async fn do_get_chat(
        &self,
        prompt: &Prompt,
        messages: &[ChatMessage],
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        let mut messages = format_chat_messages(messages, prompt);
        if messages[0].role != "system" {
            anyhow::bail!(
                "When using Anthropic, the first message in chat must have role = `system`"
            )
        }
        let system_prompt = messages.remove(0).content;
        self.get_chat(system_prompt, messages, max_tokens).await
    }
}

#[async_trait::async_trait]
impl TransformerBackend for Anthropic {
    #[instrument(skip(self))]
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        let max_tokens = self.configuration.max_tokens.completion;
        let insert_text = match &self.configuration.chat.completion {
            Some(messages) => self.do_get_chat(prompt, messages, max_tokens).await?,
            None => {
                anyhow::bail!("Please set `transformer->anthropic->chat->completion` messages")
            }
        };
        Ok(DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerationResponse> {
        let max_tokens = self.configuration.max_tokens.generation;
        let generated_text = match &self.configuration.chat.generation {
            Some(messages) => self.do_get_chat(prompt, messages, max_tokens).await?,
            None => {
                anyhow::bail!("Please set `transformer->anthropic->chat->generation` messages")
            }
        };
        Ok(DoGenerationResponse { generated_text })
    }

    #[instrument(skip(self))]
    async fn do_generate_stream(
        &self,
        request: &GenerationStreamRequest,
    ) -> anyhow::Result<DoGenerationStreamResponse> {
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn anthropic_chat_do_completion() -> anyhow::Result<()> {
        let configuration: config::Anthropic = serde_json::from_value(json!({
            "chat_endpoint": "https://api.anthropic.com/v1/messages",
            "model": "claude-3-haiku-20240307",
            "auth_token_env_var_name": "ANTHROPIC_API_KEY",
            "chat": {
                "completion": [
                    {
                        "role": "system",
                        "content": "You are a coding assistant. You job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
                    },
                    {
                        "role": "user",
                        "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
                    }
                ],
            },
            "max_tokens": {
                "completion": 16,
                "generation": 64
            },
            "max_context": 4096
        }))?;
        let anthropic = Anthropic::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let response = anthropic.do_completion(&prompt).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn anthropic_chat_do_generate() -> anyhow::Result<()> {
        let configuration: config::Anthropic = serde_json::from_value(json!({
            "chat_endpoint": "https://api.anthropic.com/v1/messages",
            "model": "claude-3-haiku-20240307",
            "auth_token_env_var_name": "ANTHROPIC_API_KEY",
            "chat": {
                "generation": [
                    {
                        "role": "system",
                        "content": "You are a coding assistant. You job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
                    },
                    {
                        "role": "user",
                        "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
                    }
                ]
            },
            "max_tokens": {
                "completion": 16,
                "generation": 64
            },
            "max_context": 4096
        }))?;
        let anthropic = Anthropic::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let response = anthropic.do_generate(&prompt).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
