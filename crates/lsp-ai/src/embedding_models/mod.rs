use crate::config::ValidEmbeddingModel;

mod ollama;

#[derive(Clone, Copy)]
pub enum EmbeddingPurpose {
    Storage,
    Retrieval,
}

#[async_trait::async_trait]
pub trait EmbeddingModel {
    async fn embed(
        &self,
        batch: Vec<&str>,
        purpose: EmbeddingPurpose,
    ) -> anyhow::Result<Vec<Vec<f32>>>;
}

impl TryFrom<ValidEmbeddingModel> for Box<dyn EmbeddingModel + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(value: ValidEmbeddingModel) -> Result<Self, Self::Error> {
        match value {
            ValidEmbeddingModel::Ollama(config) => Ok(Box::new(ollama::Ollama::new(config))),
        }
    }
}
