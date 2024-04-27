use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::memory_backends::PromptForType;

const DEFAULT_LLAMA_CPP_N_CTX: usize = 1024;
const DEFAULT_OPENAI_MAX_CONTEXT: usize = 2048;

const DEFAULT_MAX_COMPLETION_TOKENS: usize = 16;
const DEFAULT_MAX_GENERATION_TOKENS: usize = 64;

pub type Kwargs = HashMap<String, Value>;

#[derive(Debug, Clone, Deserialize)]
pub enum ValidMemoryBackend {
    #[serde(rename = "file_store")]
    FileStore(FileStore),
    #[serde(rename = "postgresml")]
    PostgresML(PostgresML),
}

impl Default for ValidMemoryBackend {
    fn default() -> Self {
        ValidMemoryBackend::FileStore(FileStore::default())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ValidModel {
    #[serde(rename = "llamacpp")]
    LLaMACPP(LLaMACPP),
    #[serde(rename = "openai")]
    OpenAI(OpenAI),
    #[serde(rename = "anthropic")]
    Anthropic(Anthropic),
}

impl Default for ValidModel {
    fn default() -> Self {
        ValidModel::LLaMACPP(LLaMACPP::default())
    }
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
pub struct MaxTokens {
    pub completion: usize,
    pub generation: usize,
}

impl Default for MaxTokens {
    fn default() -> Self {
        Self {
            completion: DEFAULT_MAX_COMPLETION_TOKENS,
            generation: DEFAULT_MAX_GENERATION_TOKENS,
        }
    }
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

#[derive(Clone, Debug, Deserialize)]
pub struct LLaMACPP {
    // The model to use
    #[serde(flatten)]
    pub model: Model,
    // // Fill in the middle support
    // pub fim: Option<FIM>,
    // // The maximum number of new tokens to generate
    // #[serde(default)]
    // pub max_tokens: MaxTokens,
    // // Chat args
    // pub chat: Option<Chat>,
    // Kwargs passed to LlamaCPP
    #[serde(flatten)]
    pub kwargs: Kwargs,
}

impl Default for LLaMACPP {
    fn default() -> Self {
        Self {
            model: Model {
                repository: "stabilityai/stable-code-3b".to_string(),
                name: Some("stable-code-3b-Q5_K_M.gguf".to_string()),
            },
            // fim: Some(FIM {
            //     start: "<fim_prefix>".to_string(),
            //     middle: "<fim_suffix>".to_string(),
            //     end: "<fim_middle>".to_string(),
            // }),
            // max_tokens: MaxTokens::default(),
            // chat: None,
            kwargs: Kwargs::default(),
        }
    }
}

const fn api_max_requests_per_second_default() -> f32 {
    0.5
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

const fn max_context_default() -> usize {
    DEFAULT_OPENAI_MAX_CONTEXT
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
    // // Fill in the middle support
    // pub fim: Option<FIM>,
    // // The maximum number of new tokens to generate
    // #[serde(default)]
    // pub max_tokens: MaxTokens,
    // // Chat args
    // pub chat: Option<Chat>,
    // // Other available args
    // #[serde(default = "top_p_default")]
    // pub top_p: f32,
    // #[serde(default = "presence_penalty_default")]
    // pub presence_penalty: f32,
    // #[serde(default = "frequency_penalty_default")]
    // pub frequency_penalty: f32,
    // #[serde(default = "temperature_default")]
    // pub temperature: f32,
    // #[serde(default = "max_context_default")]
    // max_context: usize,
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
    // // The maximum number of new tokens to generate
    // #[serde(default)]
    // pub max_tokens: MaxTokens,
    // // Chat args
    // pub chat: Chat,
    // #[serde(default = "top_p_default")]
    // pub top_p: f32,
    // #[serde(default = "temperature_default")]
    // pub temperature: f32,
    // #[serde(default = "max_context_default")]
    // max_context: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Completion {
    // The model key to use
    pub model: String,

    // Model args
    #[serde(default)]
    pub max_tokens: MaxTokens,
    #[serde(default = "presence_penalty_default")]
    pub presence_penalty: f32,
    #[serde(default = "frequency_penalty_default")]
    pub frequency_penalty: f32,
    #[serde(default = "top_p_default")]
    pub top_p: f32,
    #[serde(default = "temperature_default")]
    pub temperature: f32,
    #[serde(default = "max_context_default")]
    max_context: usize,

    // FIM args
    pub fim: Option<FIM>,

    // Chat args
    pub chat: Option<Vec<ChatMessage>>,
    pub chat_template: Option<String>,
    pub chat_format: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct ValidConfig {
    #[serde(default)]
    pub memory: ValidMemoryBackend,
    #[serde(default)]
    pub transformer: ValidModel,
    #[serde(default)]
    pub models: HashMap<String, ValidModel>,
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
        let configuration_args = args
            .as_object_mut()
            .context("Server configuration must be a JSON object")?
            .remove("initializationOptions");
        let valid_args = match configuration_args {
            Some(configuration_args) => serde_json::from_value(configuration_args)?,
            None => ValidConfig::default(),
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

    pub fn get_transformer_max_requests_per_second(&self) -> f32 {
        match &self.config.transformer {
            ValidModel::LLaMACPP(_) => 1.,
            ValidModel::OpenAI(openai) => openai.max_requests_per_second,
            ValidModel::Anthropic(anthropic) => anthropic.max_requests_per_second,
        }
    }

    pub fn get_max_context_length(&self) -> usize {
        match &self.config.transformer {
            ValidModel::LLaMACPP(llama_cpp) => llama_cpp
                .kwargs
                .get("n_ctx")
                .map(|v| {
                    v.as_u64()
                        .map(|u| u as usize)
                        .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX)
                })
                .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX),
            ValidModel::OpenAI(openai) => openai.max_context,
            ValidModel::Anthropic(anthropic) => anthropic.max_context,
        }
    }

    pub fn get_fim(&self) -> Option<&FIM> {
        match &self.config.transformer {
            ValidModel::LLaMACPP(llama_cpp) => llama_cpp.fim.as_ref(),
            ValidModel::OpenAI(openai) => openai.fim.as_ref(),
            ValidModel::Anthropic(_) => None,
        }
    }

    pub fn get_chat(&self) -> Option<&Chat> {
        match &self.config.transformer {
            ValidModel::LLaMACPP(llama_cpp) => llama_cpp.chat.as_ref(),
            ValidModel::OpenAI(openai) => openai.chat.as_ref(),
            ValidModel::Anthropic(anthropic) => Some(&anthropic.chat),
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
                    "chat": [
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
