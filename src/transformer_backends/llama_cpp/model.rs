use anyhow::Context;
use llama_cpp_2::{
    context::params::LlamaContextParams,
    ggml_time_us,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaChatMessage, LlamaModel},
    token::data_array::LlamaTokenDataArray,
};
use once_cell::sync::Lazy;
use std::{num::NonZeroU32, path::PathBuf, time::Duration};
use tracing::{debug, info, instrument};

use crate::config::{self, ChatMessage};

use super::LLaMACPPRunParams;

static BACKEND: Lazy<LlamaBackend> = Lazy::new(|| LlamaBackend::init().unwrap());

pub struct Model {
    model: LlamaModel,
    n_ctx: NonZeroU32,
}

impl Model {
    #[instrument]
    pub fn new(model_path: PathBuf, config: &config::LLaMACPP) -> anyhow::Result<Self> {
        // Initialize the model_params
        let model_params = LlamaModelParams::default().with_n_gpu_layers(config.n_gpu_layers);

        // Load the model
        debug!("Loading model at path: {:?}", model_path);
        let model = LlamaModel::load_from_file(&BACKEND, model_path, &model_params)?;

        Ok(Model {
            model,
            n_ctx: NonZeroU32::new(config.n_ctx).context("`n_ctx` must be non zero")?,
        })
    }

    #[instrument(skip(self))]
    pub fn complete(&self, prompt: &str, params: LLaMACPPRunParams) -> anyhow::Result<String> {
        // initialize the context
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(self.n_ctx))
            .with_n_batch(self.n_ctx.get());

        let mut ctx = self
            .model
            .new_context(&BACKEND, ctx_params)
            .with_context(|| "unable to create the llama_context")?;

        let tokens_list = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .with_context(|| format!("failed to tokenize {}", prompt))?;

        let n_cxt = ctx.n_ctx() as usize;
        let n_kv_req = tokens_list.len() + params.max_new_tokens;

        info!(
            "n_len / max_new_tokens = {}, n_ctx = {n_cxt}, k_kv_req = {n_kv_req}",
            params.max_new_tokens
        );

        // make sure the KV cache is big enough to hold all the prompt and generated tokens
        if n_kv_req > n_cxt {
            anyhow::bail!(
                "n_kv_req > n_ctx, the required kv cache size is not big enough
        either reduce max_new_tokens or increase n_ctx"
            )
        }

        let mut batch = LlamaBatch::new(n_cxt, 1);

        let last_index: i32 = (tokens_list.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens_list.into_iter()) {
            // llama_decode will output logits only for the last token of the prompt
            let is_last = i == last_index;
            batch.add(token, i, &[0], is_last)?;
        }

        ctx.decode(&mut batch)
            .with_context(|| "llama_decode() failed")?;

        // main loop
        let n_start = batch.n_tokens();
        let mut output: Vec<String> = vec![];
        let mut n_cur = n_start;
        let mut n_decode = 0;
        let t_main_start = ggml_time_us();
        while (n_cur as usize) <= (n_start as usize + params.max_new_tokens) {
            // sample the next token
            {
                let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
                let candidates_p = LlamaTokenDataArray::from_iter(candidates, false);

                // sample the most likely token
                let new_token_id = ctx.sample_token_greedy(candidates_p);

                // is it an end of stream?
                if new_token_id == self.model.token_eos() {
                    break;
                }

                output.push(self.model.token_to_str(new_token_id)?);
                batch.clear();
                batch.add(new_token_id, n_cur, &[0], true)?;
            }
            n_cur += 1;
            ctx.decode(&mut batch).with_context(|| "failed to eval")?;
            n_decode += 1;
        }

        let t_main_end = ggml_time_us();
        let duration = Duration::from_micros((t_main_end - t_main_start) as u64);
        info!(
            "decoded {} tokens in {:.2} s, speed {:.2} t/s\n",
            n_decode,
            duration.as_secs_f32(),
            n_decode as f32 / duration.as_secs_f32()
        );
        info!("{}", ctx.timings());

        Ok(output.join(""))
    }

    #[instrument(skip(self))]
    pub fn apply_chat_template(
        &self,
        messages: Vec<ChatMessage>,
        template: Option<String>,
    ) -> anyhow::Result<String> {
        let llama_chat_messages = messages
            .into_iter()
            .map(|c| LlamaChatMessage::new(c.role, c.content))
            .collect::<Result<Vec<LlamaChatMessage>, _>>()?;
        Ok(self
            .model
            .apply_chat_template(template, llama_chat_messages, true)?)
    }

    #[instrument(skip(self))]
    pub fn get_eos_token(&self) -> anyhow::Result<String> {
        let token = self.model.token_eos();
        Ok(self.model.token_to_str(token)?)
    }

    #[instrument(skip(self))]
    pub fn get_bos_token(&self) -> anyhow::Result<String> {
        let token = self.model.token_bos();
        Ok(self.model.token_to_str(token)?)
    }
}
