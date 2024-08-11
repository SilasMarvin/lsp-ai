use std::collections::HashMap;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, instrument};

use crate::{
    config::{self, ChatMessage, FIM},
    memory_backends::Prompt,
    transformer_worker::{
        DoGenerationResponse, DoGenerationStreamResponse, GenerationStreamRequest,
    },
    utils::{format_chat_messages, format_context_code},
};

use super::TransformerBackend;

const fn max_tokens_default() -> usize {
    64
}

const fn top_p_default() -> f32 {
    0.95
}

const fn presence_penalty_default() -> f32 {
    0.
}

const fn frequency_penalty_default() -> f32 {
    0.
}

const fn temperature_default() -> f32 {
    0.1
}

// NOTE: We cannot deny unknown fields as the provided parameters may contain other fields relevant to other processes
#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIRunParams {
    pub(crate) fim: Option<FIM>,
    messages: Option<Vec<ChatMessage>>,
    #[serde(default = "max_tokens_default")]
    pub(crate) max_tokens: usize,
    #[serde(default = "top_p_default")]
    pub(crate) top_p: f32,
    #[serde(default = "presence_penalty_default")]
    pub(crate) presence_penalty: f32,
    #[serde(default = "frequency_penalty_default")]
    pub(crate) frequency_penalty: f32,
    #[serde(default = "temperature_default")]
    pub(crate) temperature: f32,
}

pub(crate) struct OpenAI {
    configuration: config::OpenAI,
}

#[derive(Deserialize)]
struct OpenAICompletionsChoice {
    text: String,
}

#[derive(Deserialize)]
struct OpenAICompletionsResponse {
    choices: Option<Vec<OpenAICompletionsChoice>>,
    error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct OpenAIChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIChatChoices {
    pub(crate) message: OpenAIChatMessage,
}

#[derive(Deserialize)]
pub(crate) struct OpenAIChatResponse {
    pub(crate) choices: Option<Vec<OpenAIChatChoices>>,
    pub(crate) error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    pub(crate) other: HashMap<String, Value>,
}

impl OpenAI {
    #[instrument]
    pub fn new(configuration: config::OpenAI) -> Self {
        Self { configuration }
    }

    fn get_token(&self) -> anyhow::Result<String> {
        if let Some(env_var_name) = &self.configuration.auth_token_env_var_name {
            Ok(std::env::var(env_var_name)?)
        } else if let Some(token) = &self.configuration.auth_token {
            Ok(token.to_string())
        } else {
            anyhow::bail!(
                "set `auth_token_env_var_name` or `auth_token` to use an OpenAI compatible API"
            )
        }
    }

    async fn get_completion(
        &self,
        prompt: &str,
        params: OpenAIRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let params = json!({
            "model": self.configuration.model,
            "max_tokens": params.max_tokens,
            "n": 1,
            "top_p": params.top_p,
            "presence_penalty": params.presence_penalty,
            "frequency_penalty": params.frequency_penalty,
            "temperature": params.temperature,
            "echo": false,
            "prompt": prompt
        });
        info!(
            "Calling OpenAI compatible completions API with parameters:\n{}",
            serde_json::to_string_pretty(&params).unwrap()
        );
        let res: OpenAICompletionsResponse = client
            .post(
                self.configuration
                    .completions_endpoint
                    .as_ref()
                    .context("specify `completions_endpoint` to use completions. Wanted to use `chat` instead? Please specify `chat_endpoint` and `messages`.")?,
            )
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&params)
            .send().await?
            .json().await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(mut choices) = res.choices {
            Ok(std::mem::take(&mut choices[0].text))
        } else {
            anyhow::bail!(
                "Uknown error while making request to OpenAI: {:?}",
                res.other
            )
        }
    }

    async fn get_chat(
        &self,
        messages: Vec<ChatMessage>,
        params: OpenAIRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let params = json!({
            "model": self.configuration.model,
            "max_tokens": params.max_tokens,
            "n": 1,
            "top_p": params.top_p,
            "presence_penalty": params.presence_penalty,
            "frequency_penalty": params.frequency_penalty,
            "temperature": params.temperature,
            "messages": messages
        });
        info!(
            "Calling OpenAI compatible chat API with parameters:\n{}",
            serde_json::to_string_pretty(&params).unwrap()
        );
        let res: OpenAIChatResponse = client
            .post(
                self.configuration
                    .chat_endpoint
                    .as_ref()
                    .context("must specify `chat_endpoint` to use completions")?,
            )
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&params)
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(choices) = res.choices {
            Ok(choices[0].message.content.clone())
        } else {
            anyhow::bail!(
                "Unknown error while making request to OpenAI: {:?}",
                res.other
            )
        }
    }

    async fn do_chat_completion(
        &self,
        prompt: &Prompt,
        params: OpenAIRunParams,
    ) -> anyhow::Result<String> {
        match prompt {
            Prompt::ContextAndCode(code_and_context) => match &params.messages {
                Some(completion_messages) => {
                    let messages = format_chat_messages(completion_messages, code_and_context);
                    self.get_chat(messages, params).await
                }
                None => {
                    self.get_completion(
                        &format_context_code(&code_and_context.context, &code_and_context.code),
                        params,
                    )
                    .await
                }
            },
            Prompt::FIM(fim) => match &params.fim {
                Some(fim_params) => {
                    self.get_completion(
                        &format!(
                            "{}{}{}{}{}",
                            fim_params.start,
                            fim.prompt,
                            fim_params.middle,
                            fim.suffix,
                            fim_params.end
                        ),
                        params,
                    )
                    .await
                }
                None => anyhow::bail!("Prompt type is FIM but no FIM parameters provided"),
            },
        }
    }
}

#[async_trait::async_trait]
impl TransformerBackend for OpenAI {
    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,

        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: OpenAIRunParams = serde_json::from_value(params)?;
        let generated_text = self.do_chat_completion(prompt, params).await?;
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
    async fn open_ai_completion_do_generate() -> anyhow::Result<()> {
        let configuration: config::OpenAI = from_value(json!({
            "completions_endpoint": "https://api.openai.com/v1/completions",
            "model": "gpt-3.5-turbo-instruct",
            "auth_token_env_var_name": "OPENAI_API_KEY",
        }))?;
        let open_ai = OpenAI::new(configuration);
        let prompt = Prompt::default_without_cursor();
        let run_params = json!({
            "max_tokens": 64
        });
        let response = open_ai.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn open_ai_chat_do_generate() -> anyhow::Result<()> {
        let configuration: config::OpenAI = serde_json::from_value(json!({
            "chat_endpoint": "https://api.openai.com/v1/chat/completions",
            "model": "gpt-3.5-turbo",
            "auth_token_env_var_name": "OPENAI_API_KEY",
        }))?;
        let open_ai = OpenAI::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "messages": [
                {
                    "role": "system",
                    "content": "Test"
                },
                {
                    "role": "user",
                    "content": "Test {CONTEXT} - {CODE}"
                }
            ],
            "max_tokens": 64
        });
        let response = open_ai.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
