use crate::config::ValidEmbeddingModel;

mod ollama;

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let magnitude = (vector.iter().map(|&x| x * x).sum::<f32>()).sqrt();

    if magnitude != 0.0 {
        for element in &mut vector {
            *element /= magnitude;
        }
    }

    vector
}

#[derive(Clone, Copy)]
pub(crate) enum EmbeddingPurpose {
    Storage,
    Retrieval,
}

#[async_trait::async_trait]
pub(crate) trait EmbeddingModel {
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
