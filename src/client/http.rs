use std::sync::LazyLock;

use reqwest::{IntoUrl, StatusCode};
use serde::de::DeserializeOwned;

use super::{RequestError, RequestResult};

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to create http client")
});

pub async fn get<U: IntoUrl, T: DeserializeOwned>(url: U, query: Option<Vec<(&str, &str)>>) -> RequestResult<T> {
    let result = HTTP_CLIENT.get(url).query(&query.unwrap_or_default()).send().await;
    match result {
        Err(e) => Err(RequestError::Error(format!("http get failed, {}", e))),
        Ok(response) => process_response(response).await,
    }
}

async fn process_response<T: DeserializeOwned>(response: reqwest::Response) -> RequestResult<T> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;

    if status.is_success() {
        return match serde_json::from_str::<T>(&payload) {
            Ok(data) => Ok(data),
            Err(e) => Err(RequestError::Error(format!(
                "http request to {url} failed, decode payload failed, {e}, payload: {payload}",
            ))),
        };
    }

    match status {
        StatusCode::NOT_FOUND => Err(RequestError::NotFound),
        _ => Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        ))),
    }
}
