pub mod openai;
pub mod pan115;
pub mod pan123;
pub mod pan189;
pub mod tmdb;

mod http;

pub mod library_remote;

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("already exists")]
    AlreadyExists,

    #[error("share audit not pass")]
    ShareAuditNotPass,

    #[error("share cancelled, {0}")]
    ShareCancelled(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found, {0}")]
    NotFound(String),

    #[error("too many requests")]
    TooManyRequests,

    #[error("bad request, {0}")]
    BadRequest(String),

    #[error("connect error, {0}")]
    ConnectError(String),

    #[error("timeout, {0}")]
    Timeout(String),

    #[error("server error, {0}")]
    ServerError(String),

    #[error("error, {0}")]
    Other(String),
}

pub type RequestResult<T> = std::result::Result<T, RequestError>;

impl From<reqwest_middleware::Error> for RequestError {
    fn from(e: reqwest_middleware::Error) -> Self {
        match e {
            reqwest_middleware::Error::Reqwest(e) => Self::from(e),
            reqwest_middleware::Error::Middleware(e) => {
                Self::Other(format!("middleware error: {e}"))
            }
        }
    }
}

impl From<reqwest::Error> for RequestError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.to_string())
        } else if e.is_connect() {
            Self::ConnectError(e.to_string())
        } else {
            let url = e.url().map(|u| u.to_string()).unwrap_or_default();
            Self::Other(format!("http request to {url} error: {e}"))
        }
    }
}
