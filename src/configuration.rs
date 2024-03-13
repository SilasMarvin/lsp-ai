use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const DEFAULT_LLAMA_CPP_N_CTX: usize = 1024;
const DEFAULT_OPENAI_MAX_CONTEXT: usize = 2048;

const DEFAULT_MAX_COMPLETION_TOKENS: usize = 32;
const DEFAULT_MAX_GENERATION_TOKENS: usize = 256;

pub type Kwargs = HashMap<String, Value>;

pub enum ValidMemoryBackend {
    FileStore(FileStore),
    PostgresML(PostgresML),
}

pub enum ValidTransformerBackend {
    LlamaCPP(ModelGGUF),
    OpenAI(OpenAI),
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
    pub crawl: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct FileStore {
    pub crawl: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ValidMemoryConfiguration {
    file_store: Option<FileStore>,
    postgresml: Option<PostgresML>,
}

impl Default for ValidMemoryConfiguration {
    fn default() -> Self {
        Self {
            file_store: Some(FileStore::default()),
            postgresml: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Model {
    pub repository: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelGGUF {
    // The model to use
    #[serde(flatten)]
    pub model: Model,
    // Fill in the middle support
    pub fim: Option<FIM>,
    // The maximum number of new tokens to generate
    #[serde(default)]
    pub max_tokens: MaxTokens,
    // Chat args
    pub chat: Option<Chat>,
    // Kwargs passed to LlamaCPP
    #[serde(flatten)]
    pub kwargs: Kwargs,
}

impl Default for ModelGGUF {
    fn default() -> Self {
        Self {
            model: Model {
                repository: "stabilityai/stable-code-3b".to_string(),
                name: Some("stable-code-3b-Q5_K_M.gguf".to_string()),
            },
            fim: Some(FIM {
                start: "<fim_prefix>".to_string(),
                middle: "<fim_suffix>".to_string(),
                end: "<fim_middle>".to_string(),
            }),
            max_tokens: MaxTokens::default(),
            chat: None,
            kwargs: Kwargs::default(),
        }
    }
}

const fn openai_top_p_default() -> f32 {
    0.95
}

const fn openai_top_k_default() -> usize {
    40
}

const fn openai_presence_penalty() -> f32 {
    0.
}

const fn openai_frequency_penalty() -> f32 {
    0.
}

const fn openai_temperature() -> f32 {
    0.1
}

const fn openai_max_context() -> usize {
    DEFAULT_OPENAI_MAX_CONTEXT
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct OpenAI {
    // The auth token env var name
    pub auth_token_env_var_name: Option<String>,
    pub auth_token: Option<String>,
    // The completions endpoint
    pub completions_endpoint: String,
    // The model name
    pub model: String,
    // Fill in the middle support
    pub fim: Option<FIM>,
    // The maximum number of new tokens to generate
    #[serde(default)]
    pub max_tokens: MaxTokens,
    // Chat args
    pub chat: Option<Chat>,
    // Other available args
    #[serde(default = "openai_top_p_default")]
    pub top_p: f32,
    #[serde(default = "openai_top_k_default")]
    pub top_k: usize,
    #[serde(default = "openai_presence_penalty")]
    pub presence_penalty: f32,
    #[serde(default = "openai_frequency_penalty")]
    pub frequency_penalty: f32,
    #[serde(default = "openai_temperature")]
    pub temperature: f32,
    #[serde(default = "openai_max_context")]
    max_context: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct ValidTransformerConfiguration {
    openai: Option<OpenAI>,
    model_gguf: Option<ModelGGUF>,
}

impl Default for ValidTransformerConfiguration {
    fn default() -> Self {
        Self {
            model_gguf: Some(ModelGGUF::default()),
            openai: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
struct ValidConfiguration {
    memory: ValidMemoryConfiguration,
    transformer: ValidTransformerConfiguration,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct ValidClientParams {
    #[serde(alias = "rootURI")]
    root_uri: Option<String>,
    workspace_folders: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    valid_config: ValidConfiguration,
    client_params: ValidClientParams,
}

impl Configuration {
    pub fn new(mut args: Value) -> Result<Self> {
        eprintln!("\n\n{}\n\n", args.to_string());
        let configuration_args = args
            .as_object_mut()
            .context("Server configuration must be a JSON object")?
            .remove("initializationOptions");
        let valid_args = match configuration_args {
            Some(configuration_args) => serde_json::from_value(configuration_args)?,
            None => ValidConfiguration::default(),
        };
        let client_params: ValidClientParams = serde_json::from_value(args)?;
        Ok(Self {
            valid_config: valid_args,
            client_params,
        })
    }

    pub fn get_memory_backend(&self) -> Result<ValidMemoryBackend> {
        // if self.valid_config.memory.file_store.is_some() {
        if let Some(file_store) = &self.valid_config.memory.file_store {
            Ok(ValidMemoryBackend::FileStore(file_store.to_owned()))
        } else if let Some(postgresml) = &self.valid_config.memory.postgresml {
            Ok(ValidMemoryBackend::PostgresML(postgresml.to_owned()))
        } else {
            anyhow::bail!("Invalid memory configuration")
        }
    }

    pub fn into_transformer_backend(self) -> Result<ValidTransformerBackend> {
        if let Some(model_gguf) = self.valid_config.transformer.model_gguf {
            Ok(ValidTransformerBackend::LlamaCPP(model_gguf))
        } else if let Some(openai) = self.valid_config.transformer.openai {
            Ok(ValidTransformerBackend::OpenAI(openai))
        } else {
            anyhow::bail!("Invalid model configuration")
        }
    }

    ///////////////////////////////////////
    // Helpers for the Memory Backend /////
    ///////////////////////////////////////

    pub fn get_max_context_length(&self) -> Result<usize> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(model_gguf
                .kwargs
                .get("n_ctx")
                .map(|v| {
                    v.as_u64()
                        .map(|u| u as usize)
                        .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX)
                })
                .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX))
        } else if let Some(openai_config) = &self.valid_config.transformer.openai {
            Ok(openai_config.max_context)
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_fim(&self) -> Result<Option<&FIM>> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(model_gguf.fim.as_ref())
        } else if let Some(openai_config) = &self.valid_config.transformer.openai {
            Ok(openai_config.fim.as_ref())
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_chat(&self) -> Result<Option<&Chat>> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(model_gguf.chat.as_ref())
        } else if let Some(openai_config) = &self.valid_config.transformer.openai {
            Ok(openai_config.chat.as_ref())
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_gguf_config() {
        let args = json!({
                "memory": {
                    "file_store": {}
                },
                "transformer": {
                    "model_gguf": {
                        "repository": "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
                        "name": "deepseek-coder-6.7b-instruct.Q5_K_S.gguf",
                        "max_new_tokens": {
                            "completion": 32,
                            "generation": 256,
                        },
                        "fim": {
                            "start": "<fim_prefix>",
                            "middle": "<fim_suffix>",
                            "end": "<fim_middle>"
                        },
                        "chat": {
                            // "completion": [
                            //     {
                            //         "role": "system",
                            //         "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. Keep your response brief. Do not produce any text besides code. \n\n{context}",
                            //     },
                            //     {
                            //         "role": "user",
                            //         "content": "Complete the following code: \n\n{code}"
                            //     }
                            // ],
                            // "generation": [
                            //     {
                            //         "role": "system",
                            //         "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{context}",
                            //     },
                            //     {
                            //         "role": "user",
                            //         "content": "Complete the following code: \n\n{code}"
                            //     }
                            // ],
                            "chat_template": "{% if not add_generation_prompt is defined %}\n{% set add_generation_prompt = false %}\n{% endif %}\n{%- set ns = namespace(found=false) -%}\n{%- for message in messages -%}\n    {%- if message['role'] == 'system' -%}\n        {%- set ns.found = true -%}\n    {%- endif -%}\n{%- endfor -%}\n{{bos_token}}{%- if not ns.found -%}\n{{'You are an AI programming assistant, utilizing the Deepseek Coder model, developed by Deepseek Company, and you only answer questions related to computer science. For politically sensitive questions, security and privacy issues, and other non-computer science questions, you will refuse to answer\\n'}}\n{%- endif %}\n{%- for message in messages %}\n    {%- if message['role'] == 'system' %}\n{{ message['content'] }}\n    {%- else %}\n        {%- if message['role'] == 'user' %}\n{{'### Instruction:\\n' + message['content'] + '\\n'}}\n        {%- else %}\n{{'### Response:\\n' + message['content'] + '\\n<|EOT|>\\n'}}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{% if add_generation_prompt %}\n{{'### Response:'}}\n{% endif %}"
                        },
                        "n_ctx": 2048,
                        "n_gpu_layers": 35,
                    }
                }
        });
        Configuration::new(args).unwrap();
    }

    #[test]
    fn openai_config() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "transformer": {
                    "openai": {
                        "completions_endpoint": "https://api.fireworks.ai/inference/v1/completions",
                        "model": "accounts/fireworks/models/llama-v2-34b-code",
                        "auth_token_env_var_name": "FIREWORKS_API_KEY",
                        "chat": {
                            // Not sure what to do here yet
                        },
                        "max_tokens": {
                            "completion": 16,
                            "generation": 64
                        },
                        "max_context": 4096
                    },
                }
            }
        });
        Configuration::new(args).unwrap();
    }
}
