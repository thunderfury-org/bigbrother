use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use reqwest::StatusCode;
use tracing::error;

use crate::{
    application::resolve_download_url::{ResolveDownloadUrlResult, ResolveDownloadUrlService},
    infrastructure::{
        cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
    },
};

#[derive(Clone)]
pub(crate) struct MediaServerContext {
    pub path_prefix: String,
    pub cache: StringCacheStore,
    pub remote: Pan123LibraryRemote,
}

impl MediaServerContext {
    pub(crate) fn new(
        path_prefix: String,
        cache: StringCacheStore,
        remote: Pan123LibraryRemote,
    ) -> Self {
        Self {
            path_prefix,
            cache,
            remote,
        }
    }
}

pub(super) fn new_router(ctx: MediaServerContext) -> Router {
    let path = format!("{}/{{*path}}", ctx.path_prefix);
    Router::new()
        .route(path.as_str(), get(redirect))
        .with_state(ctx)
}

async fn redirect(
    State(ctx): State<MediaServerContext>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let file_id = params.get("file_id");
    if file_id.is_none() {
        return (StatusCode::BAD_REQUEST, "file_id is required").into_response();
    }

    match file_id.unwrap().parse::<i64>() {
        Err(e) => (
            StatusCode::BAD_REQUEST,
            format!("file_id is invalid: {}", e),
        )
            .into_response(),
        Ok(id) => {
            match ResolveDownloadUrlService::new(ctx.cache.clone(), ctx.remote.clone())
                .resolve(id)
                .await
            {
                Ok(ResolveDownloadUrlResult::Redirect(url)) => {
                    Redirect::to(url.as_str()).into_response()
                }
                Ok(ResolveDownloadUrlResult::Unauthorized) => {
                    error!(
                        "Unauthorized to get download url of file {}, id: {}",
                        path, id
                    );
                    (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
                }
                Ok(ResolveDownloadUrlResult::NotFound) => {
                    error!("File {} not found, id: {}", path, id);
                    (StatusCode::NOT_FOUND, "File not found").into_response()
                }
                Err(e) => {
                    error!(
                        "Failed to get download url of file {}, id: {}, {}",
                        path, id, e
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to get download url",
                    )
                        .into_response()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::ports::DownloadUrlCache,
        cache::Cache,
        client::pan123,
        infrastructure::{
            cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
        },
    };
    use axum::http::StatusCode as HttpStatusCode;
    use migration::MigratorTrait;
    use sea_orm::Database;

    async fn media_server_context() -> MediaServerContext {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        migration::Migrator::up(&db, None).await.unwrap();
        MediaServerContext::new(
            "/d".to_string(),
            StringCacheStore::new(Cache::new(db)),
            Pan123LibraryRemote::new(pan123::Client::new("", "", "/tmp/pan123-test")),
        )
    }

    #[tokio::test]
    async fn test_redirect_missing_file_id() {
        let params = HashMap::new();
        let response = redirect(
            State(media_server_context().await),
            Path("test.mp4".to_string()),
            Query(params),
        )
        .await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_redirect_invalid_file_id() {
        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "not_a_number".to_string());

        let response = redirect(
            State(media_server_context().await),
            Path("test.mp4".to_string()),
            Query(params),
        )
        .await;

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.contains("file_id is invalid"));
    }

    #[tokio::test]
    async fn test_redirect_from_cache() {
        let ctx = media_server_context().await;

        // Pre-populate cache with a URL
        let cache_key = "pan123:download_url:12345";
        let test_url = "https://example.com/download/test.mp4";
        ctx.cache
            .set_download_url(cache_key, test_url, std::time::Duration::from_secs(60))
            .await
            .unwrap();

        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "12345".to_string());

        let response = redirect(State(ctx), Path("test.mp4".to_string()), Query(params)).await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::SEE_OTHER);
        assert_eq!(parts.headers.get("location").unwrap(), test_url);
    }
}
