use tokio::signal::{
    self,
    unix::{SignalKind, signal},
};
use tower_http::{
    LatencyUnit,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::{Level, info};

use crate::state::AppState;

mod file;

pub async fn run(state: AppState) {
    let host = state
        .config
        .get_file_server_config()
        .host
        .as_ref()
        .map_or_else(|| "0.0.0.0", |h| h.as_str());
    let port = state.config.get_file_server_config().port.unwrap_or(3100);

    info!("Starting file server at http://{}:{}", host, port);

    let app = file::new_router(state.clone()).layer(
        TraceLayer::new_for_http().on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        ),
    );

    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    info!("File server has shutdown gracefully.");
}

async fn shutdown_signal() {
    let mut term = signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = term.recv() => {},
    }

    info!("Signal received, starting graceful shutdown...");
}
