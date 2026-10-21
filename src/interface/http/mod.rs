use axum::Router;
use trace_id::TraceIdLayer;
use tracing::info;

use crate::{error::AppResult, util::signal::shutdown_signal};

pub(crate) mod community;
pub(crate) mod console;
pub(crate) mod console_assets;
pub(crate) mod emby_proxy;
pub(crate) mod library;
pub(crate) mod log;
pub(crate) mod media;
pub(crate) mod media_dirs;
pub(crate) mod share;
pub(crate) mod subscription;

pub(crate) async fn run(
    name: &'static str,
    addr: String,
    app: Router,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> AppResult<()> {
    info!("Starting {name} at {}", addr);

    let app = app
        .layer(log::LogLayer)
        .layer(TraceIdLayer::new_high_performance());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_signal(name) => {}
                _ = shutdown_rx.wait_for(|&s| s) => {
                    info!("Shutdown signal received for {name}...");
                }
            }
        })
        .await?;
    info!("{name} has shutdown gracefully.");
    Ok(())
}
