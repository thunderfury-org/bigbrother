use std::io;

use crate::infrastructure::client::RequestError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppErrorKind {
    InvalidParameter,
    NotFound,
    Dependency,
    RuleRejected,
    Runtime,
    Internal,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("dependency error: {0}")]
    Dependency(String),
    #[error("rule rejected: {0}")]
    RuleRejected(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn kind(&self) -> AppErrorKind {
        match self {
            Self::InvalidParameter(_) => AppErrorKind::InvalidParameter,
            Self::NotFound(_) => AppErrorKind::NotFound,
            Self::Dependency(_) => AppErrorKind::Dependency,
            Self::RuleRejected(_) => AppErrorKind::RuleRejected,
            Self::Runtime(_) => AppErrorKind::Runtime,
            Self::Internal(_) => AppErrorKind::Internal,
        }
    }
}

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
        Self::Dependency(format!("request error, {e}"))
    }
}

impl From<sea_orm::error::DbErr> for AppError {
    fn from(e: sea_orm::error::DbErr) -> Self {
        Self::Dependency(format!("db error, {e}"))
    }
}
