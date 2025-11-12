use std::{path::Path, sync::LazyLock};

use reqwest::{IntoUrl, StatusCode};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Serialize, de::DeserializeOwned};

use super::{RequestError, RequestResult};

const UA_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
pub const AUTH_KEY: &str = "Authorization";

static HTTP_CLIENT: LazyLock<ClientWithMiddleware> = LazyLock::new(|| {
    let req_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to create http client");

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    ClientBuilder::new(req_client)
        // Retry failed requests.
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
});

pub async fn download_file(url: &str, path: &str) -> RequestResult<()> {
    let response = HTTP_CLIENT.get(url).send().await?;
    let status = response.status();
    let url = response.url().to_string();
    let payload = response.bytes().await?;

    if status.is_success() {
        // write payload to file
        tokio::fs::create_dir_all(Path::new(path).parent().unwrap())
            .await
            .map_err(|e| RequestError::Error(format!("create dir failed, {}", e)))?;
        tokio::fs::write(path, payload)
            .await
            .map_err(|e| RequestError::Error(format!("write file failed, {}", e)))?;
        return Ok(());
    }

    match status {
        StatusCode::NOT_FOUND => Err(RequestError::NotFound(url)),
        _ => Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload:?}",
        ))),
    }
}

pub async fn get<U: IntoUrl, T: DeserializeOwned>(
    url: U,
    query: Option<Vec<(&str, &str)>>,
    headers: Option<Vec<(&str, &str)>>,
) -> RequestResult<T> {
    let mut request = HTTP_CLIENT.get(url);
    if let Some(q) = query {
        request = request.query(&q);
    }
    for (k, v) in default_headers() {
        request = request.header(k, v);
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
    for (k, v) in default_headers() {
        request = request.header(k, v);
    }
    if let Some(h) = headers {
        for (k, v) in h {
            request = request.header(k, v);
        }
    }
    if let Some(p) = payload {
        match serde_json::to_vec(p) {
            Ok(body) => {
                request = request.header("content-type", "application/json");
                request = request.body(body);
            }
            Err(err) => return Err(RequestError::Error(format!("serialize http payload failed, {}", err))),
        }
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
        StatusCode::NOT_FOUND => Err(RequestError::NotFound("resource not found".to_owned())),
        _ => Err(RequestError::Error(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        ))),
    }
}

fn default_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("user-agent", UA_VALUE),
        ("accept", "application/json;charset=UTF-8"),
        ("accept-encoding", "gzip, deflate, br"),
    ]
}
