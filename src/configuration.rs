use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[cfg(target_os = "macos")]
const DEFAULT_LLAMA_CPP_N_CTX: usize = 1024;

const DEFAULT_MAX_COMPLETION_TOKENS: usize = 32;
const DEFAULT_MAX_GENERATION_TOKENS: usize = 256;

pub type Kwargs = HashMap<String, Value>;

pub enum ValidMemoryBackend {
    FileStore,
    PostgresML,
}

pub enum ValidTransformerBackend {
    LlamaCPP,
    PostgresML,
}

#[derive(Debug, Clone, Deserialize)]
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
pub struct MaxNewTokens {
    pub completion: usize,
    pub generation: usize,
}

impl Default for MaxNewTokens {
    fn default() -> Self {
        Self {
            completion: DEFAULT_MAX_COMPLETION_TOKENS,
            generation: DEFAULT_MAX_GENERATION_TOKENS,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ValidMemoryConfiguration {
    file_store: Option<Value>,
}

impl Default for ValidMemoryConfiguration {
    fn default() -> Self {
        Self {
            file_store: Some(json!({})),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Model {
    pub repository: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ModelGGUF {
    // The model to use
    #[serde(flatten)]
    model: Model,
    // Fill in the middle support
    fim: Option<FIM>,
    // The maximum number of new tokens to generate
    #[serde(default)]
    max_new_tokens: MaxNewTokens,
    // Chat args
    chat: Option<Chat>,
    // Kwargs passed to LlamaCPP
    #[serde(flatten)]
    kwargs: Kwargs,
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
            max_new_tokens: MaxNewTokens::default(),
            chat: None,
            kwargs: Kwargs::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ValidMacTransformerConfiguration {
    model_gguf: Option<ModelGGUF>,
}

impl Default for ValidMacTransformerConfiguration {
    fn default() -> Self {
        Self {
            model_gguf: Some(ModelGGUF::default()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ValidLinuxTransformerConfiguration {
    model_gguf: Option<ModelGGUF>,
}

impl Default for ValidLinuxTransformerConfiguration {
    fn default() -> Self {
        Self {
            model_gguf: Some(ModelGGUF::default()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
struct ValidConfiguration {
    memory: ValidMemoryConfiguration,
    #[cfg(target_os = "macos")]
    #[serde(alias = "macos")]
    transformer: ValidMacTransformerConfiguration,
    #[cfg(target_os = "linux")]
    #[serde(alias = "linux")]
    transformer: ValidLinuxTransformerConfiguration,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    valid_config: ValidConfiguration,
}

impl Configuration {
    pub fn new(mut args: Value) -> Result<Self> {
        let configuration_args = args
            .as_object_mut()
            .context("Server configuration must be a JSON object")?
            .remove("initializationOptions");
        let valid_args = match configuration_args {
            Some(configuration_args) => serde_json::from_value(configuration_args)?,
            None => ValidConfiguration::default(),
        };
        Ok(Self {
            valid_config: valid_args,
        })
    }

    pub fn get_model(&self) -> Result<&Model> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(&model_gguf.model)
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_model_kwargs(&self) -> Result<&Kwargs> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(&model_gguf.kwargs)
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_memory_backend(&self) -> Result<ValidMemoryBackend> {
        if self.valid_config.memory.file_store.is_some() {
            Ok(ValidMemoryBackend::FileStore)
        } else {
            anyhow::bail!("Invalid memory configuration")
        }
    }

    pub fn get_transformer_backend(&self) -> Result<ValidTransformerBackend> {
        if self.valid_config.transformer.model_gguf.is_some() {
            Ok(ValidTransformerBackend::LlamaCPP)
        } else {
            anyhow::bail!("Invalid model configuration")
        }
    }

    pub fn get_maximum_context_length(&self) -> Result<usize> {
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
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_max_new_tokens(&self) -> Result<&MaxNewTokens> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(&model_gguf.max_new_tokens)
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_fim(&self) -> Result<Option<&FIM>> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(model_gguf.fim.as_ref())
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_chat(&self) -> Result<Option<&Chat>> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(model_gguf.chat.as_ref())
        } else {
            anyhow::bail!("We currently only support gguf models using llama cpp")
        }
    }
}
