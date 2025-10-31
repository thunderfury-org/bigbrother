use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use reqwest::StatusCode;
use tracing::info;

use crate::state::AppState;

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

    info!("Redirecting /d/{} to /download", path);
    axum::response::Redirect::to("/download").into_response()
}
