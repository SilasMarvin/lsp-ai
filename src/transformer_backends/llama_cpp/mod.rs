use anyhow::Context;
use hf_hub::api::sync::Api;

use super::TransformerBackend;
use crate::{
    configuration::Configuration,
    worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
};

mod model;
use model::Model;

pub struct LlamaCPP {
    model: Model,
    configuration: Configuration,
}

impl LlamaCPP {
    pub fn new(configuration: Configuration) -> anyhow::Result<Self> {
        let api = Api::new()?;
        let model = configuration.get_model()?;
        let name = model
            .name
            .as_ref()
            .context("Model `name` is required when using GGUF models")?;
        let repo = api.model(model.repository.to_owned());
        let model_path = repo.get(&name)?;

        let model = Model::new(model_path, configuration.get_model_kwargs()?)?;

        Ok(Self {
            model,
            configuration,
        })
    }
}

impl TransformerBackend for LlamaCPP {
    fn do_completion(&self, prompt: &str) -> anyhow::Result<DoCompletionResponse> {
        let max_new_tokens = self.configuration.get_max_new_tokens().completion;
        self.model
            .complete(prompt, max_new_tokens)
            .map(|insert_text| DoCompletionResponse { insert_text })
    }

    fn do_generate(&self, prompt: &str) -> anyhow::Result<DoGenerateResponse> {
        let max_new_tokens = self.configuration.get_max_new_tokens().generation;
        self.model
            .complete(prompt, max_new_tokens)
            .map(|generated_text| DoGenerateResponse { generated_text })
    }

    fn do_generate_stream(
        &self,
        _request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_gguf() {
        let args = json!({
            "initializationOptions": {
                "memory": {
                    "file_store": {}
                },
                "macos": {
                    "model_gguf": {
                        // "repository": "deepseek-coder-6.7b-base",
                        // "name": "Q4_K_M.gguf",
                        "repository": "stabilityai/stable-code-3b",
                        "name": "stable-code-3b-Q5_K_M.gguf",
                        "max_new_tokens": {
                            "completion": 32,
                            "generation": 256,
                        },
                        // "fim": {
                        //     "start": "",
                        //     "middle": "",
                        //     "end": ""
                        // },
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
                        "n_gpu_layers": 1000,
                    }
                },
            }
        });
        let configuration = Configuration::new(args).unwrap();
        let model = LlamaCPP::new(configuration).unwrap();
        let output = model.do_completion("def fibon").unwrap();
        println!("{}", output.insert_text);
    }
}
