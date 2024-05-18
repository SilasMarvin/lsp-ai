use serde_json::Value;

use crate::{
    config::ValidModel,
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
};

mod anthropic;
#[cfg(feature = "llamacpp")]
mod llama_cpp;
mod openai;

#[async_trait::async_trait]
pub trait TransformerBackend {
    async fn do_completion(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoCompletionResponse>;
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
}

impl TryFrom<ValidModel> for Box<dyn TransformerBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(valid_model: ValidModel) -> Result<Self, Self::Error> {
        match valid_model {
            #[cfg(feature = "llamacpp")]
            ValidModel::LLaMACPP(model_gguf) => Ok(Box::new(llama_cpp::LLaMACPP::new(model_gguf)?)),
            ValidModel::OpenAI(openai_config) => Ok(Box::new(openai::OpenAI::new(openai_config))),
            ValidModel::Anthropic(anthropic_config) => {
                Ok(Box::new(anthropic::Anthropic::new(anthropic_config)))
            }
        }
    }
}
