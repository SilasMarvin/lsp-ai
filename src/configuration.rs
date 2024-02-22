use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
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

// TODO: Review this for real lol
#[derive(Clone, Deserialize)]
pub struct FIM {
    pub start: String,
    pub middle: String,
    pub end: String,
}

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
struct ValidMemoryConfiguration {
    file_store: Option<Value>,
}

#[derive(Clone, Deserialize)]
struct ChatMessages {
    role: String,
    message: String,
}

#[derive(Clone, Deserialize)]
struct Chat {
    completion: Option<Vec<ChatMessages>>,
    generation: Option<Vec<ChatMessages>>,
}

#[derive(Clone, Deserialize)]
pub struct Model {
    pub repository: String,
    pub name: Option<String>,
}

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
struct ValidMacTransformerConfiguration {
    model_gguf: Option<ModelGGUF>,
}

#[derive(Clone, Deserialize)]
struct ValidLinuxTransformerConfiguration {
    model_gguf: Option<ModelGGUF>,
}

#[derive(Clone, Deserialize)]
struct ValidConfiguration {
    memory: ValidMemoryConfiguration,
    #[cfg(target_os = "macos")]
    #[serde(alias = "macos")]
    transformer: ValidMacTransformerConfiguration,
    #[cfg(target_os = "linux")]
    #[serde(alias = "linux")]
    transformer: ValidLinuxTransformerConfiguration,
}

#[derive(Clone)]
pub struct Configuration {
    valid_config: ValidConfiguration,
}

impl Configuration {
    pub fn new(mut args: Value) -> Result<Self> {
        let configuration_args = args
            .as_object_mut()
            .context("Server configuration must be a JSON object")?
            .remove("initializationOptions")
            .unwrap_or_default();
        let valid_args: ValidConfiguration = serde_json::from_value(configuration_args)?;
        // TODO: Make sure they only specified one model or something ya know
        Ok(Self {
            valid_config: valid_args,
        })
    }

    pub fn get_model(&self) -> Result<&Model> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(&model_gguf.model)
        } else {
            panic!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_model_kwargs(&self) -> Result<&Kwargs> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            Ok(&model_gguf.kwargs)
        } else {
            panic!("We currently only support gguf models using llama cpp")
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

    pub fn get_maximum_context_length(&self) -> usize {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            model_gguf
                .kwargs
                .get("n_ctx")
                .map(|v| {
                    v.as_u64()
                        .map(|u| u as usize)
                        .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX)
                })
                .unwrap_or(DEFAULT_LLAMA_CPP_N_CTX)
        } else {
            panic!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_max_new_tokens(&self) -> &MaxNewTokens {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            &model_gguf.max_new_tokens
        } else {
            panic!("We currently only support gguf models using llama cpp")
        }
    }

    pub fn get_fim(&self) -> Option<&FIM> {
        if let Some(model_gguf) = &self.valid_config.transformer.model_gguf {
            model_gguf.fim.as_ref()
        } else {
            panic!("We currently only support gguf models using llama cpp")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn custom_mac_gguf_model() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "macos": {
                    "model_gguf": {
                        // "repository": "deepseek-coder-6.7b-base",
                        // "name": "Q4_K_M.gguf",
                        "repository": "stabilityai/stablelm-2-zephyr-1_6b",
                        "name": "stablelm-2-zephyr-1_6b-Q5_K_M.gguf",
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
                            "completion": [
                                {
                                    "role": "system",
                                    "message": "You are a code completion chatbot. Use the following context to complete the next segement of code. Keep your response brief.\n\n{context}",
                                },
                                {
                                    "role": "user",
                                    "message": "Complete the following code: \n\n{code}"
                                }
                            ],
                            "generation": [
                                {
                                    "role": "system",
                                    "message": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{context}",
                                },
                                {
                                    "role": "user",
                                    "message": "Complete the following code: \n\n{code}"
                                }
                            ]
                        },
                        "n_ctx": 2048,
                        "n_threads": 8,
                        "n_gpu_layers": 35,
                    }
                },
            }
        });
        let _ = Configuration::new(args).unwrap();
    }
}
