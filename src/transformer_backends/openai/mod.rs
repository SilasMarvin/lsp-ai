use serde::Deserialize;
use serde_json::json;
use tracing::instrument;

use super::TransformerBackend;
use crate::{
    configuration,
    memory_backends::Prompt,
    worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
};

pub struct OpenAI {
    configuration: configuration::OpenAI,
}

#[derive(Deserialize)]
struct OpenAICompletionsChoice {
    text: String,
}

#[derive(Deserialize)]
struct OpenAICompletionsResponse {
    choices: Vec<OpenAICompletionsChoice>,
}

impl OpenAI {
    #[instrument]
    pub fn new(configuration: configuration::OpenAI) -> Self {
        Self { configuration }
    }

    fn get_completion(&self, prompt: &str, max_tokens: usize) -> anyhow::Result<String> {
        let client = reqwest::blocking::Client::new();
        let token = if let Some(env_var_name) = &self.configuration.auth_token_env_var_name {
            std::env::var(env_var_name)?
        } else if let Some(token) = &self.configuration.auth_token {
            token.to_string()
        } else {
            anyhow::bail!("Please set `auth_token_env_var_name` or `auth_token` in `openai` to use an OpenAI compatible API");
        };
        let res: OpenAICompletionsResponse = client
            .post(&self.configuration.completions_endpoint)
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "max_tokens": max_tokens,
                "n": 1,
                "top_p": self.configuration.top_p,
                "top_k": self.configuration.top_k,
                "presence_penalty": self.configuration.presence_penalty,
                "frequency_penalty": self.configuration.frequency_penalty,
                "temperature": self.configuration.temperature,
                "echo": false,
                "prompt": prompt
            }))
            .send()?
            .json()?;
        Ok(res.choices[0].text.clone())
    }
}

impl TransformerBackend for OpenAI {
    #[instrument(skip(self))]
    fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        let insert_text =
            self.get_completion(&prompt.code, self.configuration.max_tokens.completion)?;
        Ok(DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse> {
        let generated_text =
            self.get_completion(&prompt.code, self.configuration.max_tokens.completion)?;
        Ok(DoGenerateResponse { generated_text })
    }

    #[instrument(skip(self))]
    fn do_generate_stream(
        &self,
        request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse> {
        unimplemented!()
    }
}
