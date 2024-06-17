use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

use super::TransformerBackend;
use crate::{
    config,
    memory_backends::{FIMPrompt, Prompt, PromptType},
    transformer_worker::{
        DoGenerationResponse, DoGenerationStreamResponse, GenerationStreamRequest,
    },
};

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
pub struct GeminiRunParams {
    #[serde(default = "max_tokens_default")]
    pub max_tokens: usize,
    #[serde(default = "top_p_default")]
    pub top_p: f32,
    #[serde(default = "temperature_default")]
    pub temperature: f32,
    pub min_tokens: Option<u64>,
    pub random_seed: Option<u64>,
    #[serde(default)]
    pub stop: Vec<String>,
}

pub struct Gemini {
    config: config::Gemini,
}

impl Gemini {
    pub fn new(config: config::Gemini) -> Self {
        Self { config }
    }

    fn get_token(&self) -> anyhow::Result<String> {
        if let Some(env_var_name) = &self.config.auth_token_env_var_name {
            Ok(std::env::var(env_var_name)?)
        } else if let Some(token) = &self.config.auth_token {
            Ok(token.to_string())
        } else {
            anyhow::bail!(
                "set `auth_token_env_var_name` or `auth_token` to use an Gemini compatible API"
            )
        }
    }

    async fn do_fim(&self, prompt: &FIMPrompt, params: GeminiRunParams) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let res: serde_json::Value = client
            .post(
                self.config
                    .completions_endpoint
                    .as_ref()
                    .context("must specify `gemini_endpoint` to use gemini")?
                    .to_owned()
                    + token.as_ref(),
            )
            .header("Content-Type", "application/json")
            .json(&json!(
                {
                    "contents":[
                        {
                            "parts":[
                                {
                                    "text": prompt.prompt
                                }
                            ]
                        }
                    ]
                }
            ))
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.get("error") {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(candidates) = res.get("candidates") {
            Ok(candidates
                .get(0)
                .unwrap()
                .get("content")
                .unwrap()
                .get("parts")
                .unwrap()
                .get(0)
                .unwrap()
                .get("text")
                .unwrap()
                .clone()
                .to_string())
        } else {
            anyhow::bail!("Unknown error while making request to Gemini: {:?}", res);
        }
    }
    async fn do_chat_completion(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let res: serde_json::Value = client
            .post(
                self.config
                    .chat_endpoint
                    .as_ref()
                    .context("must specify `gemini_endpoint` to use gemini")?
                    .to_owned()
                    + token.as_ref(),
            )
            .header("Content-Type", "application/json")
            .json(&params)
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.get("error") {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(candidates) = res.get("candidates") {
            Ok(candidates
                .get(0)
                .unwrap()
                .get("content")
                .unwrap()
                .get("parts")
                .unwrap()
                .get(0)
                .unwrap()
                .get("text")
                .unwrap()
                .clone()
                .to_string())
        } else {
            anyhow::bail!("Unknown error while making request to Gemini: {:?}", res);
        }
    }
}

#[async_trait::async_trait]
impl TransformerBackend for Gemini {
    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: GeminiRunParams = serde_json::from_value(params)?;
        let generated_text = self.do_fim(prompt.try_into()?, params).await?;
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

    fn get_prompt_type(&self, _params: &Value) -> anyhow::Result<PromptType> {
        Ok(PromptType::FIM)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::{from_value, json};

    #[tokio::test]
    async fn gemini_completion_do_generate() -> anyhow::Result<()> {
        let configuration: config::Gemini = from_value(json!({
            "completions_endpoint": "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash-latest:generateContent?key=",
            "model": "gemini-1.5-flash-latest",
            "auth_token_env_var_name": "GEMINI_API_KEY",
        }))?;
        let anthropic = Gemini::new(configuration);
        let prompt = Prompt::default_fim();
        let run_params = json!({
            "max_tokens": 2
        });
        let response = anthropic.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        dbg!(response.generated_text);
        Ok(())
    }
    #[tokio::test]
    async fn gemini_chat_do_generate() -> anyhow::Result<()> {
        let configuration: config::Gemini = serde_json::from_value(json!({
            "chat_endpoint": "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent?key=",
            "model": "gemini-1.5-flash",
            "auth_token_env_var_name": "GEMINI_API_KEY",
        }))?;
        let gemini = Gemini::new(configuration);
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "contents": [
              {
                "role":"user",
                "parts":[{
                 "text": "Pretend you're a snowman and stay in character for each response."}]
                },
              {
                "role": "model",
                "parts":[{
                 "text": "Hello! It's so cold! Isn't that great?"}]
                },
              {
                "role": "user",
                "parts":[{
                 "text": "What's your favorite season of the year?"}]
                }
             ]
        });
        let response = gemini.do_chat_completion(&prompt, run_params).await?;
        dbg!(&response);
        assert!(!response.is_empty());
        Ok(())
    }
}
