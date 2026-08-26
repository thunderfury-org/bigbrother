use crate::{
    application::ports::{LibraryMediaUpdate, LibraryMediaUpdateKind, LibraryUpdateNotifier},
    error::AppResult,
    infrastructure::client::emby::{self, Client},
};

#[derive(Clone)]
pub struct EmbyLibraryUpdateNotifier {
    client: Client,
    local_prefix: String,
    emby_prefix: String,
}

impl EmbyLibraryUpdateNotifier {
    pub fn new(
        server_url: impl Into<String>,
        api_key: impl Into<String>,
        local_prefix: impl Into<String>,
        emby_prefix: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(&server_url.into(), &api_key.into()),
            local_prefix: local_prefix.into(),
            emby_prefix: emby_prefix.into(),
        }
    }
}

#[async_trait::async_trait]
impl LibraryUpdateNotifier for EmbyLibraryUpdateNotifier {
    async fn notify(&self, updates: &[LibraryMediaUpdate]) -> AppResult<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let payload = updates
            .iter()
            .map(|update| emby::MediaUpdate {
                path: map_library_path(
                    update.path.as_str(),
                    self.local_prefix.as_str(),
                    self.emby_prefix.as_str(),
                ),
                update_type: update_type(update.kind).to_owned(),
            })
            .collect::<Vec<_>>();
        self.client.report_media_updated(&payload).await?;
        tracing::info!(
            count = updates.len(),
            "Notified Emby of library media updates"
        );
        Ok(())
    }
}

fn update_type(kind: LibraryMediaUpdateKind) -> &'static str {
    match kind {
        LibraryMediaUpdateKind::Created => "Created",
        LibraryMediaUpdateKind::Modified => "Modified",
        LibraryMediaUpdateKind::Deleted => "Deleted",
    }
}

fn map_library_path(local_path: &str, local_prefix: &str, emby_prefix: &str) -> String {
    let local_prefix = local_prefix.trim_end_matches('/');
    let emby_prefix = emby_prefix.trim_end_matches('/');
    if local_prefix.is_empty() {
        return local_path.to_owned();
    }

    match local_path.strip_prefix(local_prefix) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            format!("{emby_prefix}{rest}")
        }
        _ => local_path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::LibraryMediaUpdateKind;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    #[test]
    fn map_library_path_keeps_path_without_prefix() {
        assert_eq!(
            map_library_path("/local/show/ep01.strm", "", "/media"),
            "/local/show/ep01.strm"
        );
    }

    #[test]
    fn map_library_path_replaces_prefix_on_path_boundary() {
        assert_eq!(
            map_library_path("/data/media/show/ep01.strm", "/data/media", "/media"),
            "/media/show/ep01.strm"
        );
        assert_eq!(
            map_library_path("/data/media", "/data/media/", "/media/"),
            "/media"
        );
        assert_eq!(
            map_library_path("/data/media2/show.strm", "/data/media", "/media"),
            "/data/media2/show.strm"
        );
    }

    #[tokio::test]
    async fn notify_sends_mapped_paths() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .and(header("X-Emby-Token", "secret"))
            .and(body_json(serde_json::json!({
                "Updates": [
                    {"Path": "/media/Movie.strm", "UpdateType": "Created"}
                ]
            })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let notifier = EmbyLibraryUpdateNotifier::new(server.uri(), "secret", "/local", "/media");
        notifier
            .notify(&[LibraryMediaUpdate {
                path: "/local/Movie.strm".to_string(),
                kind: LibraryMediaUpdateKind::Created,
            }])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn notify_propagates_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/emby/Library/Media/Updated"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let notifier = EmbyLibraryUpdateNotifier::new(server.uri(), "secret", "", "");
        let error = notifier
            .notify(&[LibraryMediaUpdate {
                path: "/local/Movie.strm".to_string(),
                kind: LibraryMediaUpdateKind::Created,
            }])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("500") || error.to_string().contains("external"));
    }
}
