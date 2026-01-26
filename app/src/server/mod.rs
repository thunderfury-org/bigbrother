use trace_id::TraceIdLayer;
use tracing::info;

use crate::{state::AppState, util::signal::shutdown_signal};

mod access_log;
mod media;

pub async fn run(state: AppState) {
    let addr = state.config().get_media_server_config().get_addr();
    info!(target = "media_server", "Starting media server at {}", addr);

    let app = media::new_router(state.clone())
        .layer(TraceIdLayer::new())
        .layer(access_log::AccessLogLayer);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    info!(target = "media_server", "Media server has shutdown gracefully.");
}
