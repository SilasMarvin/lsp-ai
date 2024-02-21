use anyhow::Context;
use llama_cpp_2::{
    context::params::LlamaContextParams,
    ggml_time_us,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    token::data_array::LlamaTokenDataArray,
};
use std::{num::NonZeroU32, path::PathBuf, time::Duration};

use crate::configuration::Kwargs;

pub struct Model {
    backend: LlamaBackend,
    model: LlamaModel,
    n_ctx: NonZeroU32,
}

impl Model {
    pub fn new(model_path: PathBuf, kwargs: &Kwargs) -> anyhow::Result<Self> {
        // Init the backend
        let backend = LlamaBackend::init()?;

        // Get n_gpu_layers if set in kwargs
        // As a default we set it to 1000, which should put all layers on the GPU
        let n_gpu_layers = kwargs
            .get("n_gpu_layers")
            .map(|u| anyhow::Ok(u.as_u64().context("n_gpu_layers must be a number")? as u32))
            .unwrap_or_else(|| Ok(1000))?;

        // Initialize the model_params
        let model_params = {
            #[cfg(feature = "cublas")]
            if !params.disable_gpu {
                LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers)
            } else {
                LlamaModelParams::default()
            }
            #[cfg(not(feature = "cublas"))]
            LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers)
        };

        // Load the model
        eprintln!("SETTING MODEL AT PATH: {:?}", model_path);
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)?;
        eprintln!("\nMODEL SET\n");

        // Get n_ctx if set in kwargs
        // As a default we set it to 2048
        let n_ctx = kwargs
            .get("n_ctx")
            .map(|u| {
                anyhow::Ok(NonZeroU32::new(
                    u.as_u64().context("n_ctx must be a number")? as u32,
                ))
            })
            .unwrap_or_else(|| Ok(NonZeroU32::new(2048)))?
            .context("n_ctx must not be zero")?;

        Ok(Model {
            backend,
            model,
            n_ctx,
        })
    }

    pub fn complete(&self, prompt: &str, max_new_tokens: usize) -> anyhow::Result<String> {
        // initialize the context
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(self.n_ctx.clone()));

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .with_context(|| "unable to create the llama_context")?;

        let tokens_list = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .with_context(|| format!("failed to tokenize {}", prompt))?;

        let n_cxt = ctx.n_ctx() as usize;
        let n_kv_req = tokens_list.len() + max_new_tokens;

        eprintln!(
            "n_len / max_new_tokens = {max_new_tokens}, n_ctx = {n_cxt}, k_kv_req = {n_kv_req}"
        );

        // make sure the KV cache is big enough to hold all the prompt and generated tokens
        if n_kv_req > n_cxt {
            anyhow::bail!(
                "n_kv_req > n_ctx, the required kv cache size is not big enough
        either reduce max_new_tokens or increase n_ctx"
            )
        }

        let mut batch = LlamaBatch::new(512, 1);

        let last_index: i32 = (tokens_list.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens_list.into_iter()) {
            // llama_decode will output logits only for the last token of the prompt
            let is_last = i == last_index;
            batch.add(token, i, &[0], is_last)?;
        }

        ctx.decode(&mut batch)
            .with_context(|| "llama_decode() failed")?;

        // main loop
        let mut output: Vec<String> = vec![];
        let mut n_cur = batch.n_tokens();
        let mut n_decode = 0;
        let t_main_start = ggml_time_us();
        while (n_cur as usize) <= max_new_tokens {
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
        eprintln!(
            "decoded {} tokens in {:.2} s, speed {:.2} t/s\n",
            n_decode,
            duration.as_secs_f32(),
            n_decode as f32 / duration.as_secs_f32()
        );
        eprintln!("{}", ctx.timings());

        Ok(output.join(""))
    }
}
