use anyhow::Context;
use hf_hub::api::sync::Api;
use tracing::{debug, instrument};

use super::TransformerBackend;
use crate::{
    configuration::Configuration,
    memory_backends::Prompt,
    utils::format_chat_messages,
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
    #[instrument]
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

    #[instrument(skip(self))]
    fn get_prompt_string(&self, prompt: &Prompt) -> anyhow::Result<String> {
        // We need to check that they not only set the `chat` key, but they set the `completion` sub key
        Ok(match self.configuration.get_chat()? {
            Some(c) => {
                if let Some(completion_messages) = &c.completion {
                    let chat_messages = format_chat_messages(completion_messages, prompt);
                    self.model
                        .apply_chat_template(chat_messages, c.chat_template.to_owned())?
                } else {
                    prompt.code.to_owned()
                }
            }
            None => prompt.code.to_owned(),
        })
    }
}

impl TransformerBackend for LlamaCPP {
    #[instrument(skip(self))]
    fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        let prompt = self.get_prompt_string(prompt)?;
        // debug!("Prompt string for LLM: {}", prompt);
        let max_new_tokens = self.configuration.get_max_new_tokens()?.completion;
        self.model
            .complete(&prompt, max_new_tokens)
            .map(|insert_text| DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse> {
        let prompt = self.get_prompt_string(prompt)?;
        // debug!("Prompt string for LLM: {}", prompt);
        let max_new_tokens = self.configuration.get_max_new_tokens()?.completion;
        self.model
            .complete(&prompt, max_new_tokens)
            .map(|generated_text| DoGenerateResponse { generated_text })
    }

    #[instrument(skip(self))]
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
                        "n_gpu_layers": 1000,
                    }
                },
            }
        });
        let configuration = Configuration::new(args).unwrap();
        let _model = LlamaCPP::new(configuration).unwrap();
        // let output = model.do_completion("def fibon").unwrap();
        // println!("{}", output.insert_text);
    }
}
