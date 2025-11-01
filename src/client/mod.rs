pub mod pan123;
pub mod tmdb;

mod http;

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("already exists")]
    AlreadyExists,

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found")]
    NotFound,

    #[error("too many requests")]
    TooManyRequests,

    #[error("error, {0}")]
    Error(String),
}

pub type RequestResult<T> = std::result::Result<T, RequestError>;

impl From<reqwest::Error> for RequestError {
    fn from(e: reqwest::Error) -> Self {
        let url = e.url().map(|u| u.to_string()).unwrap_or_else(|| "".to_string());
        Self::Error(format!("http request to {url} error: {e}"))
    }
}
