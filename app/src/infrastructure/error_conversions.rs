use std::io;

use sea_orm::error::DbErr;

use crate::error::AppError;

use super::client::RequestError;

impl From<teloxide::RequestError> for AppError {
    fn from(e: teloxide::RequestError) -> Self {
        match e {
            teloxide::RequestError::Network(_) => Self::Network(e.to_string(), true),
            teloxide::RequestError::RetryAfter(_) => Self::ExternalService(e.to_string(), true),
            teloxide::RequestError::Api(_)
            | teloxide::RequestError::InvalidJson { .. }
            | teloxide::RequestError::Io(_)
            | teloxide::RequestError::MigrateToChatId(_) => {
                Self::ExternalService(e.to_string(), false)
            }
        }
    }
}

impl From<teloxide::DownloadError> for AppError {
    fn from(e: teloxide::DownloadError) -> Self {
        match e {
            teloxide::DownloadError::Network(_) => Self::Network(e.to_string(), true),
            teloxide::DownloadError::Io(_) => Self::Internal(e.to_string()),
        }
    }
}

impl From<DbErr> for AppError {
    fn from(e: DbErr) -> Self {
        let retryable = matches!(
            e,
            DbErr::ConnectionAcquire(_) | DbErr::Conn(_) | DbErr::Exec(_) | DbErr::Query(_)
        );
        Self::Database(e.to_string(), retryable)
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<RequestError> for AppError {
    fn from(e: RequestError) -> Self {
        match e {
            RequestError::ShareAuditNotPass | RequestError::ShareCancelled(_) => {
                Self::ExternalService(e.to_string(), false)
            }
            RequestError::Unauthorized | RequestError::NotFound(_) => {
                Self::ExternalService(e.to_string(), false)
            }
            RequestError::TooManyRequests => Self::ExternalService(e.to_string(), true),
            RequestError::BadRequest(_) => Self::InvalidParameter(e.to_string()),
            RequestError::ServerError(_) => Self::ExternalService(e.to_string(), true),
            RequestError::ConnectError(_) | RequestError::Timeout(_) => {
                Self::Network(e.to_string(), true)
            }
            RequestError::AlreadyExists | RequestError::Other(_) => Self::Internal(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_audit_not_pass_maps_to_external_service_not_retryable() {
        let error = AppError::from(RequestError::ShareAuditNotPass);

        assert!(matches!(error, AppError::ExternalService(_, false)));
        assert!(!error.is_retryable());
    }

    #[test]
    fn share_cancelled_maps_to_external_service_not_retryable() {
        let error = AppError::from(RequestError::ShareCancelled("该分享已被取消".to_owned()));

        assert!(matches!(error, AppError::ExternalService(_, false)));
        assert!(!error.is_retryable());
        assert!(error.to_string().contains("该分享已被取消"));
    }

    #[test]
    fn bad_request_maps_to_invalid_parameter() {
        let error = AppError::from(RequestError::BadRequest("status: 400".to_owned()));

        assert!(matches!(error, AppError::InvalidParameter(_)));
        assert!(!error.is_retryable());
    }

    #[test]
    fn connect_error_maps_to_network_retryable() {
        let error = AppError::from(RequestError::ConnectError("dns failed".to_owned()));

        assert!(matches!(error, AppError::Network(_, true)));
        assert!(error.is_retryable());
    }

    #[test]
    fn timeout_maps_to_network_retryable() {
        let error = AppError::from(RequestError::Timeout("request timed out".to_owned()));

        assert!(matches!(error, AppError::Network(_, true)));
        assert!(error.is_retryable());
    }

    #[test]
    fn server_error_maps_to_external_service_retryable() {
        let error = AppError::from(RequestError::ServerError("status: 500".to_owned()));

        assert!(matches!(error, AppError::ExternalService(_, true)));
        assert!(error.is_retryable());
    }

    #[test]
    fn too_many_requests_maps_to_external_service_retryable() {
        let error = AppError::from(RequestError::TooManyRequests);

        assert!(matches!(error, AppError::ExternalService(_, true)));
        assert!(error.is_retryable());
    }

    #[test]
    fn other_maps_to_internal() {
        let error = AppError::from(RequestError::Other("decode failed".to_owned()));

        assert!(matches!(error, AppError::Internal(_)));
        assert!(!error.is_retryable());
    }
}
