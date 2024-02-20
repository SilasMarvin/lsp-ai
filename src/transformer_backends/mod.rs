use crate::{
    configuration::{Configuration, ValidTransformerBackend},
    worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateRequest,
        GenerateStreamRequest,
    },
};

pub mod llama_cpp;

pub trait TransformerBackend {
    fn init(&self) -> anyhow::Result<()>;
    fn do_completion(&self, prompt: &str) -> anyhow::Result<DoCompletionResponse>;
    fn do_generate(&self, prompt: &str) -> anyhow::Result<DoGenerateResponse>;
    fn do_generate_stream(
        &self,
        request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse>;
}

impl TryFrom<Configuration> for Box<dyn TransformerBackend + Send> {
    type Error = anyhow::Error;

    fn try_from(configuration: Configuration) -> Result<Self, Self::Error> {
        match configuration.get_transformer_backend()? {
            ValidTransformerBackend::LlamaCPP => {
                Ok(Box::new(llama_cpp::LlamaCPP::new(configuration)))
            }
            _ => unimplemented!(),
        }
    }
}
