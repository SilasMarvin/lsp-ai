use serde_json::Value;

use crate::{
    config::{self, ValidModel},
    memory_backends::Prompt,
    transformer_worker::{
        DoCompletionResponse, DoGenerationResponse, DoGenerationStreamResponse,
        GenerationStreamRequest,
    },
};

use self::{anthropic::AnthropicRunParams, llama_cpp::LLaMACPPRunParams, openai::OpenAIRunParams};

mod anthropic;
mod llama_cpp;
mod openai;

// impl RunParams {
//     pub fn from_completion(completion: &Completion) -> Self {
//         todo!()
//     }
// }

// macro_rules! impl_runparams_try_into {
//     ( $f:ident, $t:ident ) => {
//         impl TryInto<$f> for RunParams {
//             type Error = anyhow::Error;

//             fn try_into(self) -> Result<$f, Self::Error> {
//                 match self {
//                     Self::$t(a) => Ok(a),
//                     _ => anyhow::bail!("Cannot convert RunParams into {}", stringify!($f)),
//                 }
//             }
//         }
//     };
// }

// impl_runparams_try_into!(AnthropicRunParams, Anthropic);
// impl_runparams_try_into!(LLaMACPPRunParams, LLaMACPP);
// impl_runparams_try_into!(OpenAIRunParams, OpenAI);

#[async_trait::async_trait]
pub trait TransformerBackend {
    async fn do_completion(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoCompletionResponse>;
    async fn do_generate(
        &self,
        prompt: &Prompt,
        params: Value,
    ) -> anyhow::Result<DoGenerationResponse>;
    async fn do_generate_stream(
        &self,
        request: &GenerationStreamRequest,
        params: Value,
    ) -> anyhow::Result<DoGenerationStreamResponse>;
}

impl TryFrom<ValidModel> for Box<dyn TransformerBackend + Send + Sync> {
    type Error = anyhow::Error;

    fn try_from(valid_model: ValidModel) -> Result<Self, Self::Error> {
        match valid_model {
            ValidModel::LLaMACPP(model_gguf) => Ok(Box::new(llama_cpp::LLaMACPP::new(model_gguf)?)),
            ValidModel::OpenAI(openai_config) => Ok(Box::new(openai::OpenAI::new(openai_config))),
            ValidModel::Anthropic(anthropic_config) => {
                Ok(Box::new(anthropic::Anthropic::new(anthropic_config)))
            }
        }
    }
}
