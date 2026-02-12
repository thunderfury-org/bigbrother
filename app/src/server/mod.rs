use trace_id::TraceIdLayer;
use tracing::info;

use crate::{state::AppState, util::signal::shutdown_signal};

mod log;
mod media;

pub async fn run(state: AppState) {
    let addr = state.config().get_media_server_config().get_addr();
    info!("Starting media server at {}", addr);

    let app = media::new_router(state.clone())
        .layer(log::LogLayer)
        .layer(TraceIdLayer::new_high_performance());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal("media server"))
        .await
        .unwrap();
    info!("Media server has shutdown gracefully.");
}
