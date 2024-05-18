use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type Kwargs = HashMap<String, Value>;

#[derive(Debug, Clone, Deserialize)]
pub enum ValidMemoryBackend {
    #[serde(rename = "file_store")]
    FileStore(FileStore),
    #[serde(rename = "postgresml")]
    PostgresML(PostgresML),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ValidModel {
    #[cfg(feature = "llamacpp")]
    #[serde(rename = "llamacpp")]
    LLaMACPP(LLaMACPP),
    #[serde(rename = "openai")]
    OpenAI(OpenAI),
    #[serde(rename = "anthropic")]
    Anthropic(Anthropic),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub completion: Option<Vec<ChatMessage>>,
    pub generation: Option<Vec<ChatMessage>>,
    pub chat_template: Option<String>,
    pub chat_format: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub struct FIM {
    pub start: String,
    pub middle: String,
    pub end: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresML {
    pub database_url: Option<String>,
    #[serde(default)]
    pub crawl: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct FileStore {
    #[serde(default)]
    pub crawl: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Model {
    pub repository: String,
    pub name: Option<String>,
}

const fn n_gpu_layers_default() -> u32 {
    1000
}

const fn n_ctx_default() -> u32 {
    1000
}

#[derive(Clone, Debug, Deserialize)]
pub struct LLaMACPP {
    // The model to use
    #[serde(flatten)]
    pub model: Model,
    #[serde(default = "n_gpu_layers_default")]
    pub n_gpu_layers: u32,
    #[serde(default = "n_ctx_default")]
    pub n_ctx: u32,
}

const fn api_max_requests_per_second_default() -> f32 {
    0.5
}

#[derive(Clone, Debug, Deserialize)]
pub struct OpenAI {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: Option<String>,
    // The chat endpoint
    pub chat_endpoint: Option<String>,
    // The maximum requests per second
    #[serde(default = "api_max_requests_per_second_default")]
    pub max_requests_per_second: f32,
    // The model name
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Anthropic {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: Option<String>,
    // The chat endpoint
    pub chat_endpoint: Option<String>,
    // The maximum requests per second
    #[serde(default = "api_max_requests_per_second_default")]
    pub max_requests_per_second: f32,
    // The model name
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Completion {
    // The model key to use
    pub model: String,

    // Args are deserialized by the backend using them
    #[serde(flatten)]
    #[serde(default)]
    pub kwargs: Kwargs,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ValidConfig {
    pub memory: ValidMemoryBackend,
    pub models: HashMap<String, ValidModel>,
    pub completion: Option<Completion>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct ValidClientParams {
    #[serde(alias = "rootURI")]
    _root_uri: Option<String>,
    _workspace_folders: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub config: ValidConfig,
    _client_params: ValidClientParams,
}

impl Config {
    pub fn new(mut args: Value) -> Result<Self> {
        // Validate that the models specfied are there so we can unwrap
        let configuration_args = args
            .as_object_mut()
            .context("Server configuration must be a JSON object")?
            .remove("initializationOptions");
        let valid_args = match configuration_args {
            Some(configuration_args) => serde_json::from_value(configuration_args)?,
            None => anyhow::bail!("lsp-ai does not currently provide a default configuration. Please pass a configuration. See https://github.com/SilasMarvin/lsp-ai for configuration options and examples"),
        };
        let client_params: ValidClientParams = serde_json::from_value(args)?;
        Ok(Self {
            config: valid_args,
            _client_params: client_params,
        })
    }

    ///////////////////////////////////////
    // Helpers for the backends ///////////
    ///////////////////////////////////////

    pub fn is_completions_enabled(&self) -> bool {
        self.config.completion.is_some()
    }

    pub fn get_completion_transformer_max_requests_per_second(&self) -> anyhow::Result<f32> {
        match &self
            .config
            .models
            .get(
                &self
                    .config
                    .completion
                    .as_ref()
                    .context("Completions is not enabled")?
                    .model,
            )
            .with_context(|| {
                format!(
                    "`{}` model not found in `models` config",
                    &self.config.completion.as_ref().unwrap().model
                )
            })? {
            #[cfg(feature = "llamacpp")]
            ValidModel::LLaMACPP(_) => Ok(1.),
            ValidModel::OpenAI(openai) => Ok(openai.max_requests_per_second),
            ValidModel::Anthropic(anthropic) => Ok(anthropic.max_requests_per_second),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn llama_cpp_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "llamacpp",
                        "repository": "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
                        "name": "deepseek-coder-6.7b-instruct.Q5_K_S.gguf",
                        "n_ctx": 2048,
                        "n_gpu_layers": 35
                    }
                },
                "completion": {
                    "model": "model1",
                    "fim": {
                        "start": "<fim_prefix>",
                        "middle": "<fim_suffix>",
                        "end": "<fim_middle>"
                    },
                    "max_context": 1024,
                    "max_new_tokens": 32,
                }
            }
        });
        Config::new(args).unwrap();
    }

    #[test]
    fn openai_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "openai",
                        "completions_endpoint": "https://api.fireworks.ai/inference/v1/completions",
                        "model": "accounts/fireworks/models/llama-v2-34b-code",
                        "auth_token_env_var_name": "FIREWORKS_API_KEY",
                    },
                },
                "completion": {
                    "model": "model1",
                    "messages": [
                        {
                            "role": "system",
                            "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{CONTEXT}",
                        },
                        {
                            "role": "user",
                            "content": "Complete the following code: \n\n{CODE}"
                        }
                    ],
                    "max_new_tokens": 32,
                }
            }
        });
        Config::new(args).unwrap();
    }
}
