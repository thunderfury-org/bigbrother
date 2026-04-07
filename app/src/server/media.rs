use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use reqwest::StatusCode;
use tracing::error;

use crate::{
    application::ports::{DownloadUrlCache, DownloadUrlSource},
    application::resolve_download_url::{ResolveDownloadUrlResult, ResolveDownloadUrlService},
    infrastructure::{
        cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
    },
};

type MediaDownloadUrlService = ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>;

#[derive(Clone)]
pub(crate) struct MediaServerContext {
    pub path_prefix: String,
    pub resolver: Arc<MediaDownloadUrlService>,
}

impl MediaServerContext {
    pub(crate) fn new(path_prefix: String, resolver: MediaDownloadUrlService) -> Self {
        Self {
            path_prefix,
            resolver: Arc::new(resolver),
        }
    }
}

pub(crate) fn new_router(ctx: MediaServerContext) -> Router {
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
    redirect_with_resolver(ctx.resolver.as_ref(), path, params).await
}

async fn redirect_with_resolver<C, S>(
    resolver: &ResolveDownloadUrlService<C, S>,
    path: String,
    params: HashMap<String, String>,
) -> Response
where
    C: DownloadUrlCache,
    S: DownloadUrlSource,
{
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
        Ok(id) => match resolver.resolve(id).await {
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
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::ports::{DownloadUrlCache, DownloadUrlSource},
        client::{RequestError, RequestResult},
        error::AppResult,
    };
    use axum::http::StatusCode as HttpStatusCode;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[derive(Clone, Default)]
    struct FakeCache {
        stored: Arc<Mutex<HashMap<String, String>>>,
    }

    impl DownloadUrlCache for FakeCache {
        async fn get_download_url(&self, key: &str) -> AppResult<Option<String>> {
            Ok(self.stored.lock().unwrap().get(key).cloned())
        }

        async fn set_download_url(&self, key: &str, url: &str, _ttl: Duration) -> AppResult<()> {
            self.stored
                .lock()
                .unwrap()
                .insert(key.to_string(), url.to_string());
            Ok(())
        }
    }

    #[derive(Clone)]
    enum FakeSourceResult {
        Url(String),
        NotFound,
    }

    #[derive(Clone)]
    struct FakeSource {
        result: FakeSourceResult,
    }

    impl DownloadUrlSource for FakeSource {
        async fn get_download_url(&self, _file_id: i64) -> RequestResult<String> {
            match &self.result {
                FakeSourceResult::Url(url) => Ok(url.clone()),
                FakeSourceResult::NotFound => Err(RequestError::NotFound("missing".to_string())),
            }
        }
    }

    #[tokio::test]
    async fn test_redirect_missing_file_id() {
        let response = redirect_with_resolver(
            &ResolveDownloadUrlService::new(
                FakeCache::default(),
                FakeSource {
                    result: FakeSourceResult::Url("https://example.com".to_string()),
                },
            ),
            "test.mp4".to_string(),
            HashMap::new(),
        )
        .await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_redirect_invalid_file_id() {
        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "not_a_number".to_string());

        let response = redirect_with_resolver(
            &ResolveDownloadUrlService::new(
                FakeCache::default(),
                FakeSource {
                    result: FakeSourceResult::Url("https://example.com".to_string()),
                },
            ),
            "test.mp4".to_string(),
            params,
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
        let cache = FakeCache::default();
        let cache_key = "pan123:download_url:12345";
        let test_url = "https://example.com/download/test.mp4";
        cache
            .set_download_url(cache_key, test_url, std::time::Duration::from_secs(60))
            .await
            .unwrap();
        let resolver = ResolveDownloadUrlService::new(
            cache,
            FakeSource {
                result: FakeSourceResult::NotFound,
            },
        );

        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "12345".to_string());

        let response = redirect_with_resolver(&resolver, "test.mp4".to_string(), params).await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::SEE_OTHER);
        assert_eq!(parts.headers.get("location").unwrap(), test_url);
    }

    #[tokio::test]
    async fn test_redirect_not_found() {
        let response = redirect_with_resolver(
            &ResolveDownloadUrlService::new(
                FakeCache::default(),
                FakeSource {
                    result: FakeSourceResult::NotFound,
                },
            ),
            "test.mp4".to_string(),
            HashMap::from([("file_id".to_string(), "12345".to_string())]),
        )
        .await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::NOT_FOUND);
    }
}
