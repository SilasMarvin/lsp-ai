use anyhow::Context;
use hf_hub::api::sync::ApiBuilder;
use tracing::{debug, instrument};

use crate::{
    configuration::{self},
    memory_backends::Prompt,
    template::apply_chat_template,
    transformer_worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
    utils::format_chat_messages,
};

mod model;
use model::Model;

use super::TransformerBackend;

pub struct LlamaCPP {
    model: Model,
    configuration: configuration::LLaMACPP,
}

impl LlamaCPP {
    #[instrument]
    pub fn new(configuration: configuration::LLaMACPP) -> anyhow::Result<Self> {
        let api = ApiBuilder::new().with_progress(true).build()?;
        let name = configuration
            .model
            .name
            .as_ref()
            .context("Model `name` is required when using GGUF models")?;
        let repo = api.model(configuration.model.repository.to_owned());
        let model_path = repo.get(&name)?;
        let model = Model::new(model_path, &configuration.kwargs)?;
        Ok(Self {
            model,
            configuration,
        })
    }

    #[instrument(skip(self))]
    fn get_prompt_string(&self, prompt: &Prompt) -> anyhow::Result<String> {
        // We need to check that they not only set the `chat` key, but they set the `completion` sub key
        Ok(match &self.configuration.chat {
            Some(c) => {
                if let Some(completion_messages) = &c.completion {
                    let chat_messages = format_chat_messages(completion_messages, prompt);
                    if let Some(chat_template) = &c.chat_template {
                        let bos_token = self.model.get_bos_token()?;
                        let eos_token = self.model.get_eos_token()?;
                        apply_chat_template(&chat_template, chat_messages, &bos_token, &eos_token)?
                    } else {
                        self.model.apply_chat_template(chat_messages, None)?
                    }
                } else {
                    prompt.code.to_owned()
                }
            }
            None => prompt.code.to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl TransformerBackend for LlamaCPP {
    #[instrument(skip(self))]
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        // let prompt = self.get_prompt_string(prompt)?;
        let prompt = &prompt.code;
        debug!("Prompt string for LLM: {}", prompt);
        let max_new_tokens = self.configuration.max_tokens.completion;
        self.model
            .complete(&prompt, max_new_tokens)
            .map(|insert_text| DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse> {
        // let prompt = self.get_prompt_string(prompt)?;
        // debug!("Prompt string for LLM: {}", prompt);
        let prompt = &prompt.code;
        let max_new_tokens = self.configuration.max_tokens.completion;
        self.model
            .complete(&prompt, max_new_tokens)
            .map(|generated_text| DoGenerateResponse { generated_text })
    }

    #[instrument(skip(self))]
    async fn do_generate_stream(
        &self,
        _request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse> {
        unimplemented!()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use serde_json::json;

//     #[test]
//     fn test_gguf() {
//         let args = json!({
//             "initializationOptions": {
//                 "memory": {
//                     "file_store": {}
//                 },
//                 "model_gguf": {
//                     "repository": "stabilityai/stable-code-3b",
//                     "name": "stable-code-3b-Q5_K_M.gguf",
//                     "max_new_tokens": {
//                         "completion": 32,
//                         "generation": 256,
//                     },
//                     // "fim": {
//                     //     "start": "",
//                     //     "middle": "",
//                     //     "end": ""
//                     // },
//                     "chat": {
//                         "completion": [
//                             {
//                                 "role": "system",
//                                 "message": "You are a code completion chatbot. Use the following context to complete the next segement of code. Keep your response brief.\n\n{context}",
//                             },
//                             {
//                                 "role": "user",
//                                 "message": "Complete the following code: \n\n{code}"
//                             }
//                         ],
//                         "generation": [
//                             {
//                                 "role": "system",
//                                 "message": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{context}",
//                             },
//                             {
//                                 "role": "user",
//                                 "message": "Complete the following code: \n\n{code}"
//                             }
//                         ]
//                     },
//                     "n_ctx": 2048,
//                     "n_gpu_layers": 1000,
//                 }
//             },
//         });
//         let configuration = Configuration::new(args).unwrap();
//         let _model = LlamaCPP::new(configuration).unwrap();
//         // let output = model.do_completion("def fibon").unwrap();
//         // println!("{}", output.insert_text);
//     }
// }
