use crate::{
    configuration::{Configuration, ValidTransformerBackend},
    memory_backends::Prompt,
    worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
};

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

impl TryFrom<Configuration> for Box<dyn TransformerBackend + Send> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.into_transformer_backend()? {
            ValidTransformerBackend::LlamaCPP(model_gguf) => {
                Ok(Box::new(llama_cpp::LlamaCPP::new(model_gguf)?))
            }
            ValidTransformerBackend::OpenAI(openai_config) => {
                Ok(Box::new(openai::OpenAI::new(openai_config)))
            }
        }
    }
}
