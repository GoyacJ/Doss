use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("crypto error")]
    Crypto,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid stage transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

pub(crate) type AppResult<T> = Result<T, AppError>;
