use anyhow::Context;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

use super::{open_ai::OpenAIChatResponse, TransformerBackend};
use crate::{
    config::{self},
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
pub(crate) struct MistralFIMRunParams {
    #[serde(default = "max_tokens_default")]
    pub(crate) max_tokens: usize,
    #[serde(default = "top_p_default")]
    pub(crate) top_p: f32,
    #[serde(default = "temperature_default")]
    pub(crate) temperature: f32,
    pub(crate) min_tokens: Option<u64>,
    pub(crate) random_seed: Option<u64>,
    #[serde(default)]
    pub(crate) stop: Vec<String>,
}

pub(crate) struct MistralFIM {
    config: config::MistralFIM,
}

impl MistralFIM {
    pub(crate) fn new(config: config::MistralFIM) -> Self {
        Self { config }
    }

    fn get_token(&self) -> anyhow::Result<String> {
        if let Some(env_var_name) = &self.config.auth_token_env_var_name {
            Ok(std::env::var(env_var_name)?)
        } else if let Some(token) = &self.config.auth_token {
            Ok(token.to_string())
        } else {
            anyhow::bail!(
                "set `auth_token_env_var_name` or `auth_token` to use an MistralFIM compatible API"
            )
        }
    }

    async fn do_fim(
        &self,
        prompt: &FIMPrompt,
        params: MistralFIMRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let token = self.get_token()?;
        let res: OpenAIChatResponse = client
            .post(
                self.config
                    .fim_endpoint
                    .as_ref()
                    .context("must specify `fim_endpoint` to use fim")?,
            )
            .bearer_auth(token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "prompt": prompt.prompt,
                "suffix": prompt.suffix,
                "model": self.config.model,
                "max_tokens": params.max_tokens,
                "top_p": params.top_p,
                "temperature": params.temperature,
                "min_tokens": params.min_tokens,
                "random_seed": params.random_seed,
                "stop": params.stop
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
            anyhow::bail!(
                "Unknown error while making request to MistralFIM: {:?}",
                res.other
            );
        }
    }
}

#[async_trait::async_trait]
impl TransformerBackend for MistralFIM {
    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: MistralFIMRunParams = serde_json::from_value(params)?;
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
    async fn mistral_fim_do_generate() -> anyhow::Result<()> {
        let configuration: config::MistralFIM = from_value(json!({
            "fim_endpoint": "https://api.mistral.ai/v1/fim/completions",
            "model": "codestral-latest",
            "auth_token_env_var_name": "MISTRAL_API_KEY",
        }))?;
        let anthropic = MistralFIM::new(configuration);
        let prompt = Prompt::default_fim();
        let run_params = json!({
            "max_tokens": 2
        });
        let response = anthropic.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
