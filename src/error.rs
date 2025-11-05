use std::io;

use crate::client::RequestError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Error(String),
}

pub type AppResult<T> = std::result::Result<T, AppError>;

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        Self::Error(format!("io error, {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Error(format!("deserialize json error, {e}"))
    }
}

impl From<RequestError> for AppError {
    fn from(e: RequestError) -> Self {
        Self::Error(format!("request error, {e}"))
    }
}
