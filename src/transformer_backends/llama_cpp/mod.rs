use crate::{
    configuration::Configuration,
    worker::{
        DoCompletionResponse, DoGenerateResponse, DoGenerateStreamResponse, GenerateStreamRequest,
    },
};

use super::TransformerBackend;
use once_cell::sync::Lazy;
use pyo3::prelude::*;

pub static PY_MODULE: Lazy<anyhow::Result<Py<PyAny>>> = Lazy::new(|| {
    pyo3::Python::with_gil(|py| -> anyhow::Result<Py<PyAny>> {
        let src = include_str!("python/transformers.py");
        Ok(pyo3::types::PyModule::from_code(py, src, "transformers.py", "transformers")?.into())
    })
});

pub struct LlamaCPP {
    configuration: Configuration,
}

impl LlamaCPP {
    pub fn new(configuration: Configuration) -> Self {
        Self { configuration }
    }
}

impl TransformerBackend for LlamaCPP {
    fn init(&self) -> anyhow::Result<()> {
        // Activate the python venv
        Python::with_gil(|py| -> anyhow::Result<()> {
            let activate: Py<PyAny> = PY_MODULE
                .as_ref()
                .map_err(anyhow::Error::msg)?
                .getattr(py, "activate_venv")?;
            activate.call1(py, ("/Users/silas/Projects/lsp-ai/venv",))?;
            Ok(())
        })?;
        // Set the model
        Python::with_gil(|py| -> anyhow::Result<()> {
            let activate: Py<PyAny> = PY_MODULE
                .as_ref()
                .map_err(anyhow::Error::msg)?
                .getattr(py, "set_model")?;
            activate.call1(py, ("",))?;
            Ok(())
        })?;
        Ok(())
    }

    fn do_completion(&self, prompt: &str) -> anyhow::Result<DoCompletionResponse> {
        let max_new_tokens = self.configuration.get_max_new_tokens().completion;
        Python::with_gil(|py| -> anyhow::Result<String> {
            let transform: Py<PyAny> = PY_MODULE
                .as_ref()
                .map_err(anyhow::Error::msg)?
                .getattr(py, "transform")?;

            let out: String = transform.call1(py, (prompt, max_new_tokens))?.extract(py)?;
            Ok(out)
        })
        .map(|insert_text| DoCompletionResponse { insert_text })
    }

    fn do_generate(&self, prompt: &str) -> anyhow::Result<DoGenerateResponse> {
        let max_new_tokens = self.configuration.get_max_new_tokens().generation;
        Python::with_gil(|py| -> anyhow::Result<String> {
            let transform: Py<PyAny> = PY_MODULE
                .as_ref()
                .map_err(anyhow::Error::msg)?
                .getattr(py, "transform")?;

            let out: String = transform.call1(py, (prompt, max_new_tokens))?.extract(py)?;
            Ok(out)
        })
        .map(|generated_text| DoGenerateResponse { generated_text })
    }

    fn do_generate_stream(
        &self,
        request: &GenerateStreamRequest,
    ) -> anyhow::Result<DoGenerateStreamResponse> {
        Ok(DoGenerateStreamResponse {
            generated_text: "".to_string(),
        })
    }
}
