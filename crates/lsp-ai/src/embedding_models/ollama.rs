use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::config;

use super::{normalize, EmbeddingModel, EmbeddingPurpose};

#[derive(Deserialize)]
pub struct EmbedResponse {
    embedding: Option<Vec<f32>>,
    error: Option<Value>,
    #[serde(default)]
    #[serde(flatten)]
    other: HashMap<String, Value>,
}

pub struct Ollama {
    config: config::OllamaEmbeddingModel,
}

impl Ollama {
    pub fn new(config: config::OllamaEmbeddingModel) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl EmbeddingModel for Ollama {
    async fn embed(
        &self,
        batch: Vec<&str>,
        purpose: EmbeddingPurpose,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = vec![];
        let prefix = match purpose {
            EmbeddingPurpose::Storage => &self.config.prefix.storage,
            EmbeddingPurpose::Retrieval => &self.config.prefix.retrieval,
        };
        let client = reqwest::Client::new();
        for item in batch {
            let prompt = format!("{prefix}{item}");
            let res: EmbedResponse = client
                .post(
                    self.config
                        .endpoint
                        .as_deref()
                        .unwrap_or("http://localhost:11434/api/embeddings"),
                )
                .header("Content-Type", "application/json")
                .header("Accept", "application/json")
                .json(&json!({
                    "model": self.config.model,
                    "prompt": prompt
                }))
                .send()
                .await?
                .json()
                .await?;
            if let Some(error) = res.error {
                anyhow::bail!("{:?}", error.to_string())
            } else if let Some(embedding) = res.embedding {
                results.push(normalize(embedding));
            } else {
                anyhow::bail!(
                    "Unknown error while making request to Ollama: {:?}",
                    res.other
                )
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn ollama_embeding() -> anyhow::Result<()> {
        let configuration: config::OllamaEmbeddingModel = serde_json::from_value(json!({
            "model": "nomic-embed-text",
            "prefix": {
                "retrieval": "search_query",
                "storage": "search_document"
            }
        }))?;

        let ollama = Ollama::new(configuration);
        let results = ollama
            .embed(
                vec!["Hello world!", "How are you?"],
                EmbeddingPurpose::Retrieval,
            )
            .await?;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 768);

        Ok(())
    }
}
