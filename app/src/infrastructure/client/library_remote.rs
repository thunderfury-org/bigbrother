use crate::{
    application::ports::{
        DownloadUrlError, DownloadUrlResult, DownloadUrlSource, LibraryRemote, RemoteEntry,
    },
    error::AppResult,
};

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

impl LibraryRemote for Pan123LibraryRemote {
    async fn get_file_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        Ok(self.client.get_file_id_by_path(path).await?)
    }

    async fn list_dir(&self, dir_id: i64) -> AppResult<Vec<RemoteEntry>> {
        Ok(self
            .client
            .list(dir_id)
            .await?
            .into_iter()
            .map(|file| {
                let is_dir = file.is_dir();
                RemoteEntry {
                    file_id: file.file_id,
                    file_name: file.file_name,
                    is_dir,
                    size: file.size,
                }
            })
            .collect())
    }

    async fn download_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
        self.client.download_file(file_id, local_path).await?;
        Ok(())
    }
}

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
        RequestError::Error(message) => DownloadUrlError::Error(message),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::application::ports::{DownloadUrlSource, LibraryRemote};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path, query_param},
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

    fn file_json(file_id: i64, file_name: &str, file_type: i32) -> serde_json::Value {
        serde_json::json!({
            "FileId": file_id,
            "FileName": file_name,
            "Type": file_type,
            "Size": 1234,
            "CreateAt": "2024-01-01T00:00:00Z",
            "UpdateAt": "2024-01-01T00:00:00Z",
            "Etag": format!("etag-{file_id}"),
            "AbsPath": format!("/{file_id}"),
        })
    }

    async fn remote(server: &MockServer) -> Pan123LibraryRemote {
        let client =
            pan123::Client::with_host("user", "pass", &unique_cache_dir(), server.uri().as_str());
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
    }

    #[tokio::test]
    async fn list_dir_maps_pan123_entries_to_remote_entries() {
        let server = MockServer::start().await;
        let remote = remote(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/file/list/new"))
            .and(query_param("parentFileId", "42"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "Next": "0",
                    "Len": 2,
                    "IsFirst": true,
                    "InfoList": [
                        file_json(10, "Season 1", 1),
                        file_json(11, "episode.mkv", 0)
                    ]
                }
            })))
            .mount(&server)
            .await;

        let entries = remote.list_dir(42).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file_id, 10);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].file_name, "episode.mkv");
        assert!(!entries[1].is_dir);
    }

    #[tokio::test]
    async fn get_download_url_maps_not_found_error() {
        let server = MockServer::start().await;
        let remote = remote(&server).await;

        Mock::given(method("POST"))
            .and(path("/api/file/info"))
            .and(body_json(serde_json::json!({
                "fileIdList": [{"FileId": 99}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "infoList": []
                }
            })))
            .mount(&server)
            .await;

        let error = DownloadUrlSource::get_download_url(&remote, 99)
            .await
            .unwrap_err();

        assert!(
            matches!(error, DownloadUrlError::NotFound(message) if message.contains("file 99 not found"))
        );
    }
}
