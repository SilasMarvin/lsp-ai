use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::instrument;

use crate::{
    config::{self, ChatMessage, FIM},
    memory_backends::Prompt,
    transformer_worker::{
        DoGenerationResponse, DoGenerationStreamResponse, GenerationStreamRequest,
    },
    utils::{format_chat_messages, format_context_code},
};

use super::TransformerBackend;

// NOTE: We cannot deny unknown fields as the provided parameters may contain other fields relevant to other processes
#[derive(Debug, Deserialize)]
pub struct OllamaRunParams {
    pub fim: Option<FIM>,
    messages: Option<Vec<ChatMessage>>,
    #[serde(default)]
    options: HashMap<String, Value>,
    system: Option<String>,
    template: Option<String>,
    keep_alive: Option<String>,
}

pub struct Ollama {
    configuration: config::Ollama,
}

#[derive(Deserialize)]
struct OllamaCompletionsResponse {
    response: Option<String>,
    error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    other: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaChatMessage>,
    error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    other: HashMap<String, Value>,
}

impl Ollama {
    #[instrument]
    pub fn new(configuration: config::Ollama) -> Self {
        Self { configuration }
    }

    async fn get_completion(
        &self,
        prompt: &str,
        params: OllamaRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let api: &String = self.configuration.api_endpoint.as_ref().expect("http://localhost:11434");
        let res: OllamaCompletionsResponse = client
            .post(format!("{}/api/generate",api))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "prompt": prompt,
                "options": params.options,
                "keep_alive": params.keep_alive,
                "raw": true,
                "stream": false
            }))
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(response) = res.response {
            Ok(response)
        } else {
            anyhow::bail!(
                "Uknown error while making request to Ollama: {:?}",
                res.other
            )
        }
    }

    async fn get_chat(
        &self,
        messages: Vec<ChatMessage>,
        params: OllamaRunParams,
    ) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let api: &String = self.configuration.api_endpoint.as_ref().expect("http://localhost:11434");
        let res: OllamaChatResponse = client
            .post(format!("{}/api/chat",api))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&json!({
                "model": self.configuration.model,
                "system": params.system,
                "template": params.template,
                "messages": messages,
                "options": params.options,
                "keep_alive": params.keep_alive,
                "stream": false
            }))
            .send()
            .await?
            .json()
            .await?;
        if let Some(error) = res.error {
            anyhow::bail!("{:?}", error.to_string())
        } else if let Some(message) = res.message {
            Ok(message.content)
        } else {
            anyhow::bail!(
                "Unknown error while making request to Ollama: {:?}",
                res.other
            )
        }
    }

    async fn do_chat_completion(
        &self,
        prompt: &Prompt,
        params: OllamaRunParams,
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
impl TransformerBackend for Ollama {
    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,

        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: OllamaRunParams = serde_json::from_value(params)?;
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
    async fn ollama_completion_do_generate() -> anyhow::Result<()> {
        let configuration: config::Ollama = from_value(json!({
            "model": "llama3",
        }))?;
        let ollama = Ollama::new(configuration);
        let prompt = Prompt::default_without_cursor();
        let run_params = json!({
            "options": {
                "num_predict": 4
            }
        });
        let response = ollama.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn ollama_chat_do_generate() -> anyhow::Result<()> {
        let configuration: config::Ollama = from_value(json!({
            "model": "llama3",
        }))?;
        let ollama = Ollama::new(configuration);
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
            "options": {
                "num_predict": 4
            }
        });
        let response = ollama.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
