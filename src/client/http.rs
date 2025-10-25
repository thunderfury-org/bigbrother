use std::sync::LazyLock;

use reqwest::{IntoUrl, StatusCode};
use serde::{Serialize, de::DeserializeOwned};

use super::{RequestError, RequestResult};

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to create http client")
});

pub async fn get<U: IntoUrl, T: DeserializeOwned>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<T> {
    let mut request = HTTP_CLIENT.get(url);
    if let Some(q) = query {
        request = request.query(&q);
    }
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }

    let result = request.send().await;
    match result {
        Err(e) => Err(RequestError::Error(format!("http get failed, {}", e))),
        Ok(response) => process_response(response).await,
    }
}

pub async fn post<U: IntoUrl, P: Serialize, T: DeserializeOwned>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
    payload: Option<&P>,
) -> RequestResult<T> {
    let mut request = HTTP_CLIENT.post(url);
    if let Some(q) = query {
        request = request.query(&q);
    }
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }
    if let Some(p) = payload {
        request = request.json(p);
    }

    let result = request.send().await;
    match result {
        Err(e) => Err(RequestError::Error(format!("http post failed, {}", e))),
        Ok(response) => process_response(response).await,
    }
}

async fn process_response<T: DeserializeOwned>(response: reqwest::Response) -> RequestResult<T> {
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.text().await?;

    println!("http request to {url} with status {status}, payload: {payload}");

    if status.is_success() {
        return match serde_json::from_str::<T>(&payload) {
            Ok(data) => Ok(data),
            Err(e) => Err(RequestError::Error(format!(
                "http request to {url} failed, decode payload failed, {e}, payload: {payload}",
            ))),
        };
    }

    match status {
        StatusCode::UNAUTHORIZED => Err(RequestError::Unauthorized),
        StatusCode::NOT_FOUND => Err(RequestError::NotFound),
        _ => Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        ))),
    }
}
