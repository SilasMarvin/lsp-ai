use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type Kwargs = HashMap<String, Value>;

const fn max_requests_per_second_default() -> f32 {
    1.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PostProcess {
    pub remove_duplicate_start: bool,
    pub remove_duplicate_end: bool,
}

impl Default for PostProcess {
    fn default() -> Self {
        Self {
            remove_duplicate_start: true,
            remove_duplicate_end: true,
        }
    }
}

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
    #[cfg(feature = "llama_cpp")]
    #[serde(rename = "llama_cpp")]
    LLaMACPP(LLaMACPP),
    #[serde(rename = "open_ai")]
    OpenAI(OpenAI),
    #[serde(rename = "anthropic")]
    Anthropic(Anthropic),
    #[serde(rename = "mistral_fim")]
    MistralFIM(MistralFIM),
    #[serde(rename = "ollama")]
    Ollama(Ollama),
    #[serde(rename = "gemini")]
    Gemini(Gemini),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: String, content: String) -> Self {
        Self {
            role,
            content,
            // tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chat {
    pub completion: Option<Vec<ChatMessage>>,
    pub generation: Option<Vec<ChatMessage>>,
    pub chat_template: Option<String>,
    pub chat_format: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
#[serde(deny_unknown_fields)]
pub struct FIM {
    pub start: String,
    pub middle: String,
    pub end: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresML {
    pub database_url: Option<String>,
    #[serde(default)]
    pub crawl: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FileStore {
    #[serde(default)]
    pub crawl: bool,
}

const fn n_gpu_layers_default() -> u32 {
    1000
}

const fn n_ctx_default() -> u32 {
    1000
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ollama {
    // The generate endpoint, default: 'http://localhost:11434/api/generate'
    pub generate_endpoint: Option<String>,
    // The chat endpoint, default: 'http://localhost:11434/api/chat'
    pub chat_endpoint: Option<String>,
    // The model name
    pub model: String,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MistralFIM {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    pub auth_token: Option<String>,
    // The fim endpoint
    pub fim_endpoint: Option<String>,
    // The model name
    pub model: String,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LLaMACPP {
    // Which model to use
    pub repository: Option<String>,
    pub name: Option<String>,
    pub file_path: Option<String>,
    // The layers to put on the GPU
    #[serde(default = "n_gpu_layers_default")]
    pub n_gpu_layers: u32,
    // The context size
    #[serde(default = "n_ctx_default")]
    pub n_ctx: u32,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAI {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    // The auth token
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: Option<String>,
    // The chat endpoint
    pub chat_endpoint: Option<String>,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
    // The model name
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gemini {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    // The auth token
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: Option<String>,
    // The chat endpoint
    pub chat_endpoint: Option<String>,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
    // The model name
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anthropic {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: Option<String>,
    // The chat endpoint
    pub chat_endpoint: Option<String>,
    // The maximum requests per second
    #[serde(default = "max_requests_per_second_default")]
    pub max_requests_per_second: f32,
    // The model name
    pub model: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Completion {
    // The model key to use
    pub model: String,
    // Args are deserialized by the backend using them
    #[serde(default)]
    pub parameters: Kwargs,
    // Parameters for post processing
    #[serde(default)]
    pub post_process: PostProcess,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

    pub fn get_completions_post_process(&self) -> Option<&PostProcess> {
        self.config.completion.as_ref().map(|x| &x.post_process)
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
            #[cfg(feature = "llama_cpp")]
            ValidModel::LLaMACPP(llama_cpp) => Ok(llama_cpp.max_requests_per_second),
            ValidModel::OpenAI(open_ai) => Ok(open_ai.max_requests_per_second),
            ValidModel::Gemini(gemini) => Ok(gemini.max_requests_per_second),
            ValidModel::Anthropic(anthropic) => Ok(anthropic.max_requests_per_second),
            ValidModel::MistralFIM(mistral_fim) => Ok(mistral_fim.max_requests_per_second),
            ValidModel::Ollama(ollama) => Ok(ollama.max_requests_per_second),
        }
    }
}

// This makes testing much easier.
#[cfg(test)]
impl Config {
    pub fn default_with_file_store_without_models() -> Self {
        Self {
            config: ValidConfig {
                memory: ValidMemoryBackend::FileStore(FileStore { crawl: false }),
                models: HashMap::new(),
                completion: None,
            },
            _client_params: ValidClientParams {
                _root_uri: None,
                _workspace_folders: None,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    #[cfg(feature = "llama_cpp")]
    fn llama_cpp_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "llama_cpp",
                        "repository": "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
                        "name": "deepseek-coder-6.7b-instruct.Q5_K_S.gguf",
                        "n_ctx": 2048,
                        "n_gpu_layers": 35
                    }
                },
                "completion": {
                    "model": "model1",
                    "parameters": {
                        "fim": {
                            "start": "<fim_prefix>",
                            "middle": "<fim_suffix>",
                            "end": "<fim_middle>"
                        },
                        "max_context": 1024,
                        "max_new_tokens": 32,
                    }
                }
            }
        });
        Config::new(args).unwrap();
    }

    #[test]
    fn ollama_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "ollama",
                        "model": "llama3"
                    }
                },
                "completion": {
                    "model": "model1",
                    "parameters": {
                        "max_context": 1024,
                        "options": {
                            "num_predict": 32
                        }
                    },
                    "post_process": {
                        "remove_duplicate_start": true,
                        "remove_duplicate_end": true,
                    }
                }
            }
        });
        Config::new(args).unwrap();
    }

    #[test]
    fn open_ai_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "open_ai",
                        "completions_endpoint": "https://api.fireworks.ai/inference/v1/completions",
                        "model": "accounts/fireworks/models/llama-v2-34b-code",
                        "auth_token_env_var_name": "FIREWORKS_API_KEY",
                    },
                },
                "completion": {
                    "model": "model1",
                    "parameters": {
                        "messages": [
                            {
                                "role": "system",
                                "content": "Test",
                            },
                            {
                                "role": "user",
                                "content": "Test {CONTEXT} - {CODE}"
                            }
                        ],
                        "max_new_tokens": 32,
                    }
                }
            }
        });
        Config::new(args).unwrap();
    }

    #[test]
    fn gemini_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "gemini",
                        "completions_endpoint": "https://generativelanguage.googleapis.com/v1beta/models/",
                        "model": "gemini-1.5-flash-latest",
                        "auth_token_env_var_name": "GEMINI_API_KEY",
                    },
                },
                "completion": {
                    "model": "model1",
                    "parameters": {
                        "systemInstruction": {
                            "role": "system",
                            "parts": [{
                                "text": "TEST system instruction"
                            }]
                        },
                        "generationConfig": {
                            "maxOutputTokens": 10
                        },
                        "contents": [
                          {
                            "role": "user",
                            "parts":[{
                             "text": "TEST - {CONTEXT} and {CODE}"}]
                            }
                         ]
                    }
                }
            }
        });
        Config::new(args).unwrap();
    }

    #[test]
    fn anthropic_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "models": {
                    "model1": {
                        "type": "anthropic",
                        "completions_endpoint": "https://api.anthropic.com/v1/messages",
                        "model": "claude-3-haiku-20240307",
                        "auth_token_env_var_name": "ANTHROPIC_API_KEY",
                    },
                },
                "completion": {
                    "model": "model1",
                    "parameters": {
                        "system": "Test",
                        "messages": [
                            {
                                "role": "user",
                                "content": "Test {CONTEXT} - {CODE}"
                            }
                        ],
                        "max_new_tokens": 32,
                    }
                }
            }
        });
        Config::new(args).unwrap();
    }
}
