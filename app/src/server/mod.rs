use axum::Router;
use trace_id::TraceIdLayer;
use tracing::info;

use crate::util::signal::shutdown_signal;

pub(crate) mod log;
pub(crate) mod media;

pub async fn run(addr: String, app: Router) {
    info!("Starting media server at {}", addr);

    let app = app
        .layer(log::LogLayer)
        .layer(TraceIdLayer::new_high_performance());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal("media server"))
        .await
        .unwrap();
    info!("Media server has shutdown gracefully.");
}
