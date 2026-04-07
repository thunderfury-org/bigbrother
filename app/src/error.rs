use std::io;

use crate::infrastructure::client::RequestError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = std::result::Result<T, AppError>;

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        Self::Internal(format!("io error, {e}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(format!("deserialize json error, {e}"))
    }
}

impl From<RequestError> for AppError {
    fn from(e: RequestError) -> Self {
        Self::Internal(format!("request error, {e}"))
    }
}

impl From<sea_orm::error::DbErr> for AppError {
    fn from(e: sea_orm::error::DbErr) -> Self {
        Self::Internal(format!("db error, {e}"))
    }
}
