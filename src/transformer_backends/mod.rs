use crate::{
    configuration::{Configuration, ValidTransformerBackend},
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
};

mod anthropic;
mod llama_cpp;
mod openai;

#[async_trait::async_trait]
pub trait TransformerBackend {
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse>;
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse>;
    async fn do_generate_stream(
        &self,
        request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse>;
}

impl TryFrom<Configuration> for Box<dyn TransformerBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.into_transformer_backend()? {
            ValidTransformerBackend::LlamaCPP(model_gguf) => {
                Ok(Box::new(llama_cpp::LlamaCPP::new(model_gguf)?))
            }
            ValidTransformerBackend::OpenAI(openai_config) => {
                Ok(Box::new(openai::OpenAI::new(openai_config)))
            }
            ValidTransformerBackend::Anthropic(anthropic_config) => {
                Ok(Box::new(anthropic::Anthropic::new(anthropic_config)))
            }
        }
    }
}
