use crate::{
    config::{Config, ValidModel},
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
};

mod anthropic;
mod llama_cpp;
mod openai;

#[async_trait::async_trait]
pub trait TransformerBackend {
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse>;
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerationResponse>;
    async fn do_generate_stream(
        &self,
        request: &GenerationStreamRequest,
    ) -> anyhow::Result<DoGenerationStreamResponse>;
}

impl TryFrom<Config> for Box<dyn TransformerBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(configuration: Config) -> Result<Self, Self::Error> {
        match configuration.config.transformer {
            ValidModel::LLaMACPP(model_gguf) => Ok(Box::new(llama_cpp::LLaMACPP::new(model_gguf)?)),
            ValidModel::OpenAI(openai_config) => Ok(Box::new(openai::OpenAI::new(openai_config))),
            ValidModel::Anthropic(anthropic_config) => {
                Ok(Box::new(anthropic::Anthropic::new(anthropic_config)))
            }
        }
    }
}
