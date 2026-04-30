use axum::Router;
use trace_id::TraceIdLayer;
use tracing::info;

use crate::{error::AppResult, util::signal::shutdown_signal};

pub(crate) mod emby_proxy;
pub(crate) mod log;
pub(crate) mod media;

pub(crate) async fn run(addr: String, app: Router) -> AppResult<()> {
    info!("Starting media server at {}", addr);

    let app = app
        .layer(log::LogLayer)
        .layer(TraceIdLayer::new_high_performance());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal("media server"))
        .await?;
    info!("Media server has shutdown gracefully.");
    Ok(())
}
