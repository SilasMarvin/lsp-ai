use crate::config::ConfigError;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
