use axum::{Router, routing::get};

use crate::state::AppState;

pub(super) fn new_router(state: AppState) -> Router {
    Router::new().route("/", get(|| async { "Hello, World!" }))
}
