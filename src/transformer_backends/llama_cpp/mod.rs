use super::TransformerBackend;
use crate::{
    config::{self, ChatMessage, FIM},
    memory_backends::Prompt,
    template::apply_chat_template,
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
    utils::format_chat_messages,
};
use anyhow::Context;
use hf_hub::api::sync::ApiBuilder;
use serde::Deserialize;
use serde_json::Value;
use tracing::instrument;

mod model;
use model::Model;

const fn max_new_tokens_default() -> usize {
    32
}

// NOTE: We cannot deny unknown fields as the provided parameters may contain other fields relevant to other processes
#[derive(Debug, Deserialize)]
pub struct LLaMACPPRunParams {
    pub fim: Option<FIM>,
    messages: Option<Vec<ChatMessage>>,
    chat_template: Option<String>, // A Jinja template
    chat_format: Option<String>,   // The name of a template in llamacpp
    #[serde(default = "max_new_tokens_default")]
    pub max_tokens: usize,
    // TODO: Explore other arguments
}

pub struct LLaMACPP {
    model: Model,
}

impl LLaMACPP {
    #[instrument]
    pub fn new(configuration: config::LLaMACPP) -> anyhow::Result<Self> {
        let api = ApiBuilder::new().with_progress(true).build()?;
        let name = configuration
            .model
            .name
            .as_ref()
            .context("Please set `name` to use LLaMA.cpp")?;
        let repo = api.model(configuration.model.repository.to_owned());
        let model_path = repo.get(name)?;
        let model = Model::new(model_path, &configuration)?;
        Ok(Self { model })
    }

    #[instrument(skip(self))]
    fn get_prompt_string(
        &self,
        prompt: &Prompt,
        params: &LLaMACPPRunParams,
    ) -> anyhow::Result<String> {
        match prompt {
            Prompt::ContextAndCode(context_and_code) => Ok(match &params.messages {
                Some(completion_messages) => {
                    let chat_messages = format_chat_messages(completion_messages, context_and_code);
                    if let Some(chat_template) = &params.chat_template {
                        let bos_token = self.model.get_bos_token()?;
                        let eos_token = self.model.get_eos_token()?;
                        apply_chat_template(chat_template, chat_messages, &bos_token, &eos_token)?
                    } else {
                        self.model
                            .apply_chat_template(chat_messages, params.chat_format.clone())?
                    }
                }
                None => context_and_code.code.clone(),
            }),
            Prompt::FIM(fim) => Ok(match &params.fim {
                Some(fim_params) => {
                    format!(
                        "{}{}{}{}{}",
                        fim_params.start, fim.prompt, fim_params.middle, fim.suffix, fim_params.end
                    )
                }
                None => anyhow::bail!("Prompt type is FIM but no FIM parameters provided"),
            }),
        }
    }
}

#[async_trait::async_trait]
impl TransformerBackend for LLaMACPP {
    #[instrument(skip(self))]
    async fn do_completion(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoCompletionResponse> {
        let params: LLaMACPPRunParams = serde_json::from_value(params)?;
        let prompt = self.get_prompt_string(prompt, &params)?;
        self.model
            .complete(&prompt, params)
            .map(|insert_text| DoCompletionResponse { insert_text })
    }

    #[instrument(skip(self))]
    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse> {
        let params: LLaMACPPRunParams = serde_json::from_value(params)?;
        let prompt = self.get_prompt_string(prompt, &params)?;
        self.model
            .complete(&prompt, params)
            .map(|generated_text| DoGenerationResponse { generated_text })
    }

    #[instrument(skip(self))]
    async fn do_generate_stream(
        &self,
        _request: &GenerationStreamRequest,
        _params: Value,
    ) -> anyhow::Result<DoGenerationStreamResponse> {
        anyhow::bail!("GenerationStream is not yet implemented")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn llama_cpp_do_completion_chat() -> anyhow::Result<()> {
        let configuration: config::LLaMACPP = serde_json::from_value(json!({
            "repository": "QuantFactory/Meta-Llama-3-8B-GGUF",
            "name": "Meta-Llama-3-8B.Q5_K_M.gguf",
            "n_ctx": 2048,
            "n_gpu_layers": 1000,
        }))?;
        let llama_cpp = LLaMACPP::new(configuration).unwrap();
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "messages": [
                {
                    "role": "system",
                    "content": "Test"
                },
                {
                    "role": "user",
                    "content": "Test {CONTEXT} - {CODE}"
                }
            ],
            "chat_format": "llama2",
            "max_tokens": 4
        });
        let response = llama_cpp.do_completion(&prompt, run_params).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn llama_cpp_do_completion_fim() -> anyhow::Result<()> {
        let configuration: config::LLaMACPP = serde_json::from_value(json!({
            "repository": "stabilityai/stable-code-3b",
            "name": "stable-code-3b-Q5_K_M.gguf",
            "n_ctx": 2048,
            "n_gpu_layers": 35,
        }))?;
        let llama_cpp = LLaMACPP::new(configuration).unwrap();
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "fim": {
                "start": "<fim_prefix>",
                "middle": "<fim_suffix>",
                "end": "<fim_middle>"
            },
            "max_tokens": 4
        });
        let response = llama_cpp.do_completion(&prompt, run_params).await?;
        assert!(!response.insert_text.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn llama_cpp_do_generate_fim() -> anyhow::Result<()> {
        let configuration: config::LLaMACPP = serde_json::from_value(json!({
            "repository": "stabilityai/stable-code-3b",
            "name": "stable-code-3b-Q5_K_M.gguf",
            "n_ctx": 2048,
            "n_gpu_layers": 35,
        }))?;
        let llama_cpp = LLaMACPP::new(configuration).unwrap();
        let prompt = Prompt::default_with_cursor();
        let run_params = json!({
            "fim": {
                "start": "<fim_prefix>",
                "middle": "<fim_suffix>",
                "end": "<fim_middle>"
            },
            "max_tokens": 4
        });
        let response = llama_cpp.do_generate(&prompt, run_params).await?;
        assert!(!response.generated_text.is_empty());
        Ok(())
    }
}
