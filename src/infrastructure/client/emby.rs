use std::time::Duration;

use reqwest::StatusCode;
use serde::Serialize;

use super::{RequestError, RequestResult};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MediaUpdate {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "UpdateType")]
    pub update_type: String,
}

#[derive(Serialize)]
struct MediaUpdatedRequest<'a> {
    #[serde(rename = "Updates")]
    updates: &'a [MediaUpdate],
}

#[derive(Clone)]
pub struct Client {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self::with_timeout(base_url, api_key, Duration::from_secs(5))
    }

    pub fn with_timeout(base_url: &str, api_key: &str, timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to create emby http client");
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key: api_key.to_owned(),
            http,
        }
    }

    pub async fn report_media_updated(&self, updates: &[MediaUpdate]) -> RequestResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let url = format!("{}/emby/Library/Media/Updated", self.base_url);
        let response = self
            .http
            .post(&url)
            .header("X-Emby-Token", self.api_key.as_str())
            .json(&MediaUpdatedRequest { updates })
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let payload = response.text().await.unwrap_or_default();
        Err(status_error(status, &url, &payload))
    }
}

fn status_error(status: StatusCode, url: &str, payload: &str) -> RequestError {
    match status {
        StatusCode::UNAUTHORIZED => RequestError::Unauthorized,
        StatusCode::NOT_FOUND => RequestError::NotFound(format!("resource not found, url: {url}")),
        StatusCode::TOO_MANY_REQUESTS => RequestError::TooManyRequests,
        s if s.is_client_error() => RequestError::BadRequest(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        )),
        _ => RequestError::ServerError(format!(
            "http request to {url} failed, status: {status}, payload: {payload}",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    fn updates() -> Vec<MediaUpdate> {
        vec![
            MediaUpdate {
                path: "/media/Movie.strm".to_string(),
                update_type: "Created".to_string(),
            },
            MediaUpdate {
                path: "/media/old".to_string(),
                update_type: "Deleted".to_string(),
            },
        ]
    }

    #[tokio::test]
    async fn report_media_updated_posts_pascal_case_payload_and_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .and(header("X-Emby-Token", "secret"))
            .and(body_json(serde_json::json!({
                "Updates": [
                    {"Path": "/media/Movie.strm", "UpdateType": "Created"},
                    {"Path": "/media/old", "UpdateType": "Deleted"}
                ]
            })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        Client::new(&server.uri(), "secret")
            .report_media_updated(&updates())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn report_media_updated_accepts_empty_success_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        Client::new(&server.uri(), "secret")
            .report_media_updated(&updates())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn report_media_updated_skips_empty_updates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        Client::new(&server.uri(), "secret")
            .report_media_updated(&[])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn report_media_updated_maps_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let error = Client::new(&server.uri(), "secret")
            .report_media_updated(&updates())
            .await
            .unwrap_err();

        assert!(
            matches!(error, RequestError::ServerError(message) if message.contains("status: 500"))
        );
    }

    #[tokio::test]
    async fn report_media_updated_times_out() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .respond_with(ResponseTemplate::new(204).set_delay(Duration::from_millis(200)))
            .mount(&server)
            .await;

        let error = Client::with_timeout(&server.uri(), "secret", Duration::from_millis(20))
            .report_media_updated(&updates())
            .await
            .unwrap_err();

        assert!(matches!(error, RequestError::Timeout(_)));
    }
}
