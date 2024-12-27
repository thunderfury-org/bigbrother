use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found, message: {0}")]
    NotFound(String),

    #[error("internal error, {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Internal(format!("reqwest error: {e}"))
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Internal(format!("io error: {e}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(format!("serde_json error: {e}"))
    }
}
