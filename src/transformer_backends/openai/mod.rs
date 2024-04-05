// Something more about what this file is
// NOTE: When decoding responses from OpenAI compatbile services, we don't care about every field

use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

use crate::{
    configuration::{self, ChatMessage},
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
    utils::{format_chat_messages, format_context_code},
};

use super::TransformerBackend;

pub struct OpenAI {
    configuration: configuration::OpenAI,
}

#[derive(Deserialize)]
struct OpenAICompletionsChoice {
    text: String,
}

#[derive(Deserialize)]
struct OpenAICompletionsResponse {
    choices: Option<Vec<OpenAICompletionsChoice>>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct OpenAIChatChoices {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct OpenAIChatResponse {
    choices: Option<Vec<OpenAIChatChoices>>,
    error: Option<Value>,
}

impl OpenAI {
    #[instrument]
    pub fn new(configuration: configuration::OpenAI) -> Self {
        Self { configuration }
    }

    fn get_token(&self) -> anyhow::Result<String> {
        if let Some(env_var_name) = &self.configuration.auth_token_env_var_name {
            Ok(std::env::var(env_var_name)?)
        } else if let Some(token) = &self.configuration.auth_token {
            Ok(token.to_string())
        } else {
            anyhow::bail!("set `auth_token_env_var_name` or `auth_token` in `tranformer->openai` to use an OpenAI compatible API")
        }
    }

    async fn get_completion(&self, prompt: &str, max_tokens: usize) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let res: OpenAICompletionsResponse = client
            .post(
                self.configuration
                    .completions_endpoint
                    .as_ref()
                    .context("specify `transformer->openai->completions_endpoint` to use completions. Wanted to use `chat` instead? Please specify `transformer->openai->chat_endpoint` and `transformer->openai->chat` messages.")?,
            )
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "max_tokens": max_tokens,
                "n": 1,
                "top_p": self.configuration.top_p,
                "presence_penalty": self.configuration.presence_penalty,
                "frequency_penalty": self.configuration.frequency_penalty,
                "temperature": self.configuration.temperature,
                "echo": false,
                "prompt": prompt
            }))
            .send().await?
            .json().await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(mut choices) = res.choices {
            Ok(std::mem::take(&mut choices[0].text))
        } else {
            anyhow::bail!("Uknown error while making request to OpenAI")
        }
    }

    async fn get_chat(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let res: OpenAIChatResponse = client
            .post(
                self.configuration
                    .chat_endpoint
                    .as_ref()
                    .context("must specify `completions_endpoint` to use completions")?,
            )
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "max_tokens": max_tokens,
                "n": 1,
                "top_p": self.configuration.top_p,
                "presence_penalty": self.configuration.presence_penalty,
                "frequency_penalty": self.configuration.frequency_penalty,
                "temperature": self.configuration.temperature,
                "messages": messages
            }))
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(choices) = res.choices {
            Ok(choices[0].message.content.clone())
        } else {
            anyhow::bail!("Uknown error while making request to OpenAI")
        }
    }

    async fn do_chat_completion(
        &self,
        prompt: &Prompt,
        messages: Option<&Vec<ChatMessage>>,
        max_tokens: usize,
    ) -> anyhow::Result<String> {
        match messages {
            Some(completion_messages) => {
                let messages = format_chat_messages(completion_messages, prompt);
                self.get_chat(messages, max_tokens).await
            }
            None => {
                self.get_completion(
                    &format_context_code(&prompt.context, &prompt.code),
                    max_tokens,
                )
                .await
            }
        }
    }
}

#[async_trait::async_trait]
impl TransformerBackend for OpenAI {
    #[instrument(skip(self))]
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        let max_tokens = self.configuration.max_tokens.completion;
        let messages = self
            .configuration
            .chat
            .as_ref()
            .and_then(|c| c.completion.as_ref());
        let insert_text = self
            .do_chat_completion(prompt, messages, max_tokens)
            .await?;
        Ok(DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse> {
        let max_tokens = self.configuration.max_tokens.generation;
        let messages = self
            .configuration
            .chat
            .as_ref()
            .and_then(|c| c.generation.as_ref());
        let generated_text = self
            .do_chat_completion(prompt, messages, max_tokens)
            .await?;
        Ok(DoGenerateResponse { generated_text })
    }

    #[instrument(skip(self))]
    async fn do_generate_stream(
        &self,
        request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse> {
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn openai_completion_do_completion() -> anyhow::Result<()> {
        let configuration: configuration::OpenAI = serde_json::from_value(json!({
            "completions_endpoint": "https://api.openai.com/v1/completions",
            "model": "gpt-3.5-turbo-instruct",
            "auth_token_env_var_name": "OPENAI_API_KEY",
            "max_tokens": {
                "completion": 16,
                "generation": 64
            },
            "max_context": 4096
        }))?;
        let openai = OpenAI::new(configuration);
        let prompt = Prompt::default_without_cursor();
        let response = openai.do_completion(&prompt).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn openai_chat_do_completion() -> anyhow::Result<()> {
        let configuration: configuration::OpenAI = serde_json::from_value(json!({
            "chat_endpoint": "https://api.openai.com/v1/chat/completions",
            "model": "gpt-3.5-turbo",
            "auth_token_env_var_name": "OPENAI_API_KEY",
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
        let openai = OpenAI::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let response = openai.do_completion(&prompt).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn openai_completion_do_generate() -> anyhow::Result<()> {
        let configuration: configuration::OpenAI = serde_json::from_value(json!({
            "completions_endpoint": "https://api.openai.com/v1/completions",
            "model": "gpt-3.5-turbo-instruct",
            "auth_token_env_var_name": "OPENAI_API_KEY",
            "max_tokens": {
                "completion": 16,
                "generation": 64
            },
            "max_context": 4096
        }))?;
        let openai = OpenAI::new(configuration);
        let prompt = Prompt::default_without_cursor();
        let response = openai.do_generate(&prompt).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn openai_chat_do_generate() -> anyhow::Result<()> {
        let configuration: configuration::OpenAI = serde_json::from_value(json!({
            "chat_endpoint": "https://api.openai.com/v1/chat/completions",
            "model": "gpt-3.5-turbo",
            "auth_token_env_var_name": "OPENAI_API_KEY",
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
        let openai = OpenAI::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let response = openai.do_generate(&prompt).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
