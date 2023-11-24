use anyhow::{Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::bigcode::{Config, GPTBigCode};
use hf_hub::{api::sync::ApiBuilder, Repo, RepoType};
use tokenizers::Tokenizer;

pub struct TextGeneration {
    model: GPTBigCode,
    device: Device,
    tokenizer: Tokenizer,
    logits_processor: LogitsProcessor,
}

impl TextGeneration {
    fn new(
        model: GPTBigCode,
        tokenizer: Tokenizer,
        seed: u64,
        temp: Option<f64>,
        top_p: Option<f64>,
        device: &Device,
    ) -> Self {
        let logits_processor = LogitsProcessor::new(seed, temp, top_p);
        Self {
            model,
            tokenizer,
            logits_processor,
            device: device.clone(),
        }
    }

    pub fn run(&mut self, prompt: &str, sample_len: usize) -> Result<String> {
        eprintln!("Starting to generate tokens");
        let mut tokens = self
            .tokenizer
            .encode(prompt, true)
            .map_err(E::msg)?
            .get_ids()
            .to_vec();
        let mut new_tokens = vec![];
        let mut outputs = vec![];
        let start_gen = std::time::Instant::now();
        for index in 0..sample_len {
            let (context_size, past_len) = if self.model.config().use_cache && index > 0 {
                (1, tokens.len().saturating_sub(1))
            } else {
                (tokens.len(), 0)
            };
            let ctxt = &tokens[tokens.len().saturating_sub(context_size)..];
            let input = Tensor::new(ctxt, &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&input, past_len)?;
            let logits = logits.squeeze(0)?.to_dtype(DType::F32)?;

            let next_token = self.logits_processor.sample(&logits)?;
            tokens.push(next_token);
            new_tokens.push(next_token);
            let token = self.tokenizer.decode(&[next_token], true).map_err(E::msg)?;
            outputs.push(token);
        }
        let dt = start_gen.elapsed();
        self.model.clear_cache();
        eprintln!(
            "GENERATED {} tokens in  {} seconds",
            outputs.len(),
            dt.as_secs()
        );
        Ok(outputs.join(""))
    }
}

pub fn build() -> Result<TextGeneration> {
    let start = std::time::Instant::now();
    eprintln!("Loading in model");
    let api = ApiBuilder::new()
        .with_token(Some(std::env::var("HF_TOKEN")?.to_string()))
        .build()?;
    let repo = api.repo(Repo::with_revision(
        "bigcode/starcoderbase-1b".to_string(),
        RepoType::Model,
        "main".to_string(),
    ));
    let tokenizer_filename = repo.get("tokenizer.json")?;
    let filenames = ["model.safetensors"]
        .iter()
        .map(|f| repo.get(f))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;
    let device = Device::Cpu;
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&filenames, DType::F32, &device)? };
    let config = Config::starcoder_1b();
    let model = GPTBigCode::load(vb, config)?;
    eprintln!("loaded the model in {:?}", start.elapsed());
    Ok(TextGeneration::new(
        model,
        tokenizer,
        0,
        Some(0.85),
        None,
        &device,
    ))
}
