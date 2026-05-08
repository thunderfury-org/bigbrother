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

#[derive(Debug, Clone, thiserror::Error)]
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
        match e {
            RequestError::ShareAuditNotPass => {
                Self::RuleRejected("request error, share audit not pass".to_owned())
            }
            RequestError::ShareCancelled(msg) => Self::NotFound(format!("分享已取消: {msg}")),
            other => Self::Dependency(format!("request error, {other}")),
        }
    }
}

impl From<sea_orm::error::DbErr> for AppError {
    fn from(e: sea_orm::error::DbErr) -> Self {
        Self::Dependency(format!("db error, {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_audit_not_pass_maps_to_rule_rejected() {
        let error = AppError::from(RequestError::ShareAuditNotPass);

        assert!(
            matches!(error, AppError::RuleRejected(message) if message.contains("share audit not pass"))
        );
    }

    #[test]
    fn share_cancelled_maps_to_not_found() {
        let error =
            AppError::from(RequestError::ShareCancelled("该分享已被取消".to_owned()));

        assert!(matches!(error.kind(), AppErrorKind::NotFound));
        assert!(error.to_string().contains("该分享已被取消"));
    }
}
