use std::collections::HashMap;
use std::time::Duration;

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use reqwest::StatusCode;
use tracing::error;

use crate::{client::RequestError, state::AppState};

pub(super) fn new_router(state: AppState) -> Router {
    let path = format!(
        "{}/{{*path}}",
        state.config().get_media_server_config().get_strm_path_prefix()
    );
    Router::new().route(path.as_str(), get(redirect)).with_state(state)
}

async fn redirect(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let file_id = params.get("file_id");
    if file_id.is_none() {
        return (StatusCode::BAD_REQUEST, "file_id is required").into_response();
    }

    match file_id.unwrap().parse::<i64>() {
        Err(e) => (StatusCode::BAD_REQUEST, format!("file_id is invalid: {}", e)).into_response(),
        Ok(id) => {
            // Try to get URL from cache first
            let cache_key = format!("pan123:download_url:{}", id);

            if let Ok(Some(cached_url)) = state.cache().get::<String>(&cache_key).await {
                return Redirect::to(cached_url.as_str()).into_response();
            }

            // Cache miss, fetch from API
            match state.client().pan123.get_download_url(id).await {
                Ok(url) => {
                    if url.is_empty() {
                        error!("Failed to get download url of file {}, id: {}", path, id);
                        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get download url").into_response();
                    }

                    // Cache the URL for 30 minutes on success
                    let ttl = Duration::from_mins(30);
                    if let Err(e) = state.cache().set(&cache_key, &url, Some(ttl)).await {
                        error!("Failed to cache download url for file {}, id: {}, {}", path, id, e);
                        // Continue even if caching fails
                    }

                    Redirect::to(url.as_str()).into_response()
                }
                Err(e) => match e {
                    RequestError::Unauthorized => {
                        error!("Unauthorized to get download url of file {}, id: {}", path, id);
                        (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
                    }
                    RequestError::NotFound(_) => {
                        error!("File {} not found, id: {}", path, id);
                        (StatusCode::NOT_FOUND, "File not found").into_response()
                    }
                    _ => {
                        error!("Failed to get download url of file {}, id: {}, {}", path, id, e);
                        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get download url").into_response()
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode as HttpStatusCode;

    #[tokio::test]
    async fn test_redirect_missing_file_id() {
        let state = AppState::for_test().await;

        let params = HashMap::new();
        let response = redirect(State(state), Path("test.mp4".to_string()), Query(params)).await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_redirect_invalid_file_id() {
        let state = AppState::for_test().await;

        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "not_a_number".to_string());

        let response = redirect(State(state), Path("test.mp4".to_string()), Query(params)).await;

        let (parts, body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.contains("file_id is invalid"));
    }

    #[tokio::test]
    async fn test_redirect_from_cache() {
        let state = AppState::for_test().await;
        state.miggrate_db().await;

        // Pre-populate cache with a URL
        let cache_key = "pan123:download_url:12345";
        let test_url = "https://example.com/download/test.mp4";
        state.cache().set(cache_key, &test_url.to_string(), None).await.unwrap();

        let mut params = HashMap::new();
        params.insert("file_id".to_string(), "12345".to_string());

        let response = redirect(State(state), Path("test.mp4".to_string()), Query(params)).await;

        let (parts, _body) = response.into_parts();
        assert_eq!(parts.status, HttpStatusCode::SEE_OTHER);
        assert_eq!(parts.headers.get("location").unwrap(), test_url);
    }
}
