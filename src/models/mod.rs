use anyhow::Result;
use serde::Deserialize;

mod starcoder;

pub trait Model {
    fn run(&mut self, prompt: &str) -> Result<String>;
}

#[derive(Deserialize, Default)]
pub struct ModelParams {
    model: Option<String>,
    model_file: Option<String>,
    model_type: Option<String>,
    max_length: Option<usize>,
}

impl TryFrom<ModelParams> for Box<dyn Model> {
    type Error = anyhow::Error;

    fn try_from(value: ModelParams) -> Result<Self> {
        let model_type = value.model_type.unwrap_or("starcoder".to_string());
        let max_length = value.max_length.unwrap_or(12);
        Ok(Box::new(match model_type.as_str() {
            "starcoder" => starcoder::build_model(value.model, value.model_file, max_length)?,
            _ => anyhow::bail!(
                "Model type: {} not supported. Feel free to make a pr or create a github issue.",
                model_type
            ),
        }))
    }
}
