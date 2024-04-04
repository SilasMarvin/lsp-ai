use anyhow::Context;
use hf_hub::api::sync::ApiBuilder;
use tracing::instrument;

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

pub struct LLaMACPP {
    model: Model,
    configuration: configuration::LLaMACPP,
}

impl LLaMACPP {
    #[instrument]
    pub fn new(configuration: configuration::LLaMACPP) -> anyhow::Result<Self> {
        let api = ApiBuilder::new().with_progress(true).build()?;
        let name = configuration
            .model
            .name
            .as_ref()
            .context("Please set `transformer->llamacpp->name` to use LLaMA.cpp")?;
        let repo = api.model(configuration.model.repository.to_owned());
        let model_path = repo.get(name)?;
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
                        apply_chat_template(chat_template, chat_messages, &bos_token, &eos_token)?
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
impl TransformerBackend for LLaMACPP {
    #[instrument(skip(self))]
    async fn do_completion(&self, prompt: &Prompt) -> anyhow::Result<DoCompletionResponse> {
        let prompt = self.get_prompt_string(prompt)?;
        let max_new_tokens = self.configuration.max_tokens.completion;
        self.model
            .complete(&prompt, max_new_tokens)
            .map(|insert_text| DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    async fn do_generate(&self, prompt: &Prompt) -> anyhow::Result<DoGenerateResponse> {
        let prompt = self.get_prompt_string(prompt)?;
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

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn llama_cpp_do_completion() -> anyhow::Result<()> {
        let configuration: configuration::LLaMACPP = serde_json::from_value(json!({
            "repository": "TheBloke/deepseek-coder-6.7B-instruct-GGUF",
            "name": "deepseek-coder-6.7b-instruct.Q5_K_S.gguf",
            "max_new_tokens": {
                "completion": 32,
                "generation": 256,
            },
            "fim": {
                "start": "<fim_prefix>",
                "middle": "<fim_suffix>",
                "end": "<fim_middle>"
            },
            "chat": {
                // "completion": [
                //     {
                //         "role": "system",
                //         "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. Keep your response brief. Do not produce any text besides code. \n\n{context}",
                //     },
                //     {
                //         "role": "user",
                //         "content": "Complete the following code: \n\n{code}"
                //     }
                // ],
                // "generation": [
                //     {
                //         "role": "system",
                //         "content": "You are a code completion chatbot. Use the following context to complete the next segement of code. \n\n{context}",
                //     },
                //     {
                //         "role": "user",
                //         "content": "Complete the following code: \n\n{code}"
                //     }
                // ],
                "chat_template": "{% if not add_generation_prompt is defined %}\n{% set add_generation_prompt = false %}\n{% endif %}\n{%- set ns = namespace(found=false) -%}\n{%- for message in messages -%}\n    {%- if message['role'] == 'system' -%}\n        {%- set ns.found = true -%}\n    {%- endif -%}\n{%- endfor -%}\n{{bos_token}}{%- if not ns.found -%}\n{{'You are an AI programming assistant, utilizing the Deepseek Coder model, developed by Deepseek Company, and you only answer questions related to computer science. For politically sensitive questions, security and privacy issues, and other non-computer science questions, you will refuse to answer\\n'}}\n{%- endif %}\n{%- for message in messages %}\n    {%- if message['role'] == 'system' %}\n{{ message['content'] }}\n    {%- else %}\n        {%- if message['role'] == 'user' %}\n{{'### Instruction:\\n' + message['content'] + '\\n'}}\n        {%- else %}\n{{'### Response:\\n' + message['content'] + '\\n<|EOT|>\\n'}}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{% if add_generation_prompt %}\n{{'### Response:'}}\n{% endif %}"
            },
            "n_ctx": 2048,
            "n_gpu_layers": 35,
        }))?;
        let llama_cpp = LLaMACPP::new(configuration).unwrap();
        let prompt = Prompt::default_with_cursor();
        let response = llama_cpp.do_completion(&prompt).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }
}
