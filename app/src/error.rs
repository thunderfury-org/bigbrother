#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("database error: {0}")]
    Database(String, bool),
    #[error("external service error: {0}")]
    ExternalService(String, bool),
    #[error("network error: {0}")]
    Network(String, bool),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = std::result::Result<T, AppError>;

impl AppError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::InvalidParameter(_)
            | Self::NotFound(_)
            | Self::Unauthorized(_)
            | Self::Internal(_) => false,
            Self::Database(_, retryable)
            | Self::ExternalService(_, retryable)
            | Self::Network(_, retryable) => *retryable,
        }
    }
}
