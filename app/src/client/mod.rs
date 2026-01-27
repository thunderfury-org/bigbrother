pub mod pan115;
pub mod pan123;
pub mod pan189;
pub mod tmdb;

mod http;

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("already exists")]
    AlreadyExists,

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found, {0}")]
    NotFound(String),

    #[error("too many requests")]
    TooManyRequests,

    #[error("error, {0}")]
    Error(String),
}

pub type RequestResult<T> = std::result::Result<T, RequestError>;

impl From<reqwest_middleware::Error> for RequestError {
    fn from(e: reqwest_middleware::Error) -> Self {
        let url = e.url().map(|u| u.to_string()).unwrap_or_default();
        Self::Error(format!("http request to {url} error: {e}"))
    }
}

impl From<reqwest::Error> for RequestError {
    fn from(e: reqwest::Error) -> Self {
        let url = e.url().map(|u| u.to_string()).unwrap_or_default();
        Self::Error(format!("http request to {url} error: {e}"))
    }
}
