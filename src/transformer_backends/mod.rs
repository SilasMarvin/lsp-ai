use anyhow::Context;
use serde_json::Value;

use crate::{
    config::ValidModel,
    memory_backends::{Prompt, PromptType},
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
};

mod anthropic;
#[cfg(feature = "llama_cpp")]
mod llama_cpp;
mod mistral_fim;
mod ollama;
mod open_ai;

#[async_trait::async_trait]
pub trait TransformerBackend {
    async fn do_completion(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoCompletionResponse> {
        self.do_generate(prompt, params)
            .await
            .map(|x| DoCompletionResponse {
                insert_text: x.generated_text,
            })
    }

    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse>;

    async fn do_generate_stream(
        &self,
        request: &GenerationStreamRequest,
        params: Value,
    ) -> anyhow::Result<DoGenerationStreamResponse>;

    fn get_prompt_type(&self, params: &Value) -> anyhow::Result<PromptType> {
        if params
            .as_object()
            .context("params must be a JSON object")?
            .contains_key("fim")
        {
            Ok(PromptType::FIM)
        } else {
            Ok(PromptType::ContextAndCode)
        }
    }
}

impl TryFrom<ValidModel> for Box<dyn TransformerBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(valid_model: ValidModel) -> Result<Self, Self::Error> {
        match valid_model {
            #[cfg(feature = "llama_cpp")]
            ValidModel::LLaMACPP(model_gguf) => Ok(Box::new(llama_cpp::LLaMACPP::new(model_gguf)?)),
            ValidModel::OpenAI(open_ai_config) => {
                Ok(Box::new(open_ai::OpenAI::new(open_ai_config)))
            }
            ValidModel::Anthropic(anthropic_config) => {
                Ok(Box::new(anthropic::Anthropic::new(anthropic_config)))
            }
            ValidModel::MistralFIM(mistral_fim) => {
                Ok(Box::new(mistral_fim::MistralFIM::new(mistral_fim)))
            }
            ValidModel::Ollama(ollama) => Ok(Box::new(ollama::Ollama::new(ollama))),
        }
    }
}
