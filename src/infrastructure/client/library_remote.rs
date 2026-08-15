use crate::application::ports::{DownloadUrlError, DownloadUrlResult, DownloadUrlSource};

use super::{RequestError, pan123};

#[derive(Clone)]
pub struct Pan123LibraryRemote {
    client: pan123::Client,
}

impl Pan123LibraryRemote {
    pub fn new(client: pan123::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl DownloadUrlSource for Pan123LibraryRemote {
    async fn get_download_url(&self, file_id: i64) -> DownloadUrlResult<String> {
        self.client
            .get_download_url(file_id)
            .await
            .map_err(map_download_url_error)
    }
}

fn map_download_url_error(err: RequestError) -> DownloadUrlError {
    match err {
        RequestError::Unauthorized => DownloadUrlError::Unauthorized,
        RequestError::NotFound(message) => DownloadUrlError::NotFound(message),
        RequestError::AlreadyExists => DownloadUrlError::Error("already exists".to_string()),
        RequestError::ShareAuditNotPass => {
            DownloadUrlError::Error("share audit not pass".to_string())
        }
        RequestError::ShareCancelled(msg) => DownloadUrlError::NotFound(msg),
        RequestError::TooManyRequests => DownloadUrlError::Error("too many requests".to_string()),
        RequestError::BadRequest(message) => DownloadUrlError::Error(message),
        RequestError::ConnectError(message) => DownloadUrlError::Error(message),
        RequestError::Timeout(message) => DownloadUrlError::Error(message),
        RequestError::ServerError(message) => DownloadUrlError::Error(message),
        RequestError::Other(message) => DownloadUrlError::Error(message),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::application::ports::DownloadUrlSource;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn unique_cache_dir() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("bigbrother-library-remote-{nanos}"))
            .display()
            .to_string()
    }

    async fn remote(server: &MockServer) -> Pan123LibraryRemote {
        let client = pan123::Client::with_open_api_base(
            &format!("{}/refresh", server.uri()),
            "refresh-token",
            &unique_cache_dir(),
            server.uri().as_str(),
        );
        client
            .set_token_for_test(
                "test-token",
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            )
            .await;
        Pan123LibraryRemote::new(client)
    }

    #[test]
    fn map_download_url_error_preserves_expected_variants() {
        assert!(matches!(
            map_download_url_error(RequestError::Unauthorized),
            DownloadUrlError::Unauthorized
        ));
        assert!(matches!(
            map_download_url_error(RequestError::NotFound("missing".to_string())),
            DownloadUrlError::NotFound(message) if message == "missing"
        ));
        assert!(matches!(
            map_download_url_error(RequestError::TooManyRequests),
            DownloadUrlError::Error(message) if message == "too many requests"
        ));
        assert!(matches!(
            map_download_url_error(RequestError::ShareAuditNotPass),
            DownloadUrlError::Error(message) if message == "share audit not pass"
        ));
        assert!(matches!(
            map_download_url_error(RequestError::BadRequest("bad".to_string())),
            DownloadUrlError::Error(message) if message.contains("bad")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::ConnectError("conn".to_string())),
            DownloadUrlError::Error(message) if message.contains("conn")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::Timeout("timeout".to_string())),
            DownloadUrlError::Error(message) if message.contains("timeout")
        ));
    }

    #[tokio::test]
    async fn get_download_url_maps_empty_url_error() {
        let server = MockServer::start().await;
        let remote = remote(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/file/download_info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadUrl": ""
                }
            })))
            .mount(&server)
            .await;

        let error = DownloadUrlSource::get_download_url(&remote, 99)
            .await
            .unwrap_err();

        assert!(
            matches!(error, DownloadUrlError::Error(message) if message.contains("empty download url"))
        );
    }
}
