use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use reqwest::StatusCode;
use tracing::{error, info};

use crate::{client::RequestError, state::AppState};

pub(super) fn new_router(state: AppState) -> Router {
    Router::new().route("/d/{*path}", get(redirect)).with_state(state)
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
        Err(e) => return (StatusCode::BAD_REQUEST, format!("file_id is invalid: {}", e)).into_response(),
        Ok(id) => match state.pan123.get_download_url(id).await {
            Ok(url) => {
                if url.is_empty() {
                    error!("Failed to get download url of file {}, id: {}", path, id);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get download url").into_response();
                }
                info!("Redirecting /d/{} to {}", path, url);
                axum::response::Redirect::to(url.as_str()).into_response()
            }
            Err(e) => match e {
                RequestError::Unauthorized => {
                    error!("Unauthorized to get download url of file {}, id: {}", path, id);
                    return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
                }
                RequestError::NotFound(_) => {
                    error!("File {} not found, id: {}", path, id);
                    return (StatusCode::NOT_FOUND, "File not found").into_response();
                }
                _ => {
                    error!("Failed to get download url of file {}, id: {}, {}", path, id, e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get download url").into_response();
                }
            },
        },
    }
}
