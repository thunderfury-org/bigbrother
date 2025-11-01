use tokio::signal::{
    self,
    unix::{SignalKind, signal},
};
use tower_http::{
    LatencyUnit,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::{Level, info};

use crate::state::AppState;

mod media;

pub async fn run(state: AppState) {
    let addr = state.config.get_media_server_config().get_addr();
    info!("Starting media server at {}", addr);

    let app = media::new_router(state.clone()).layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(
                DefaultOnResponse::new()
                    .level(Level::INFO)
                    .latency_unit(LatencyUnit::Millis),
            ),
    );

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    info!("Media server has shutdown gracefully.");
}

async fn shutdown_signal() {
    let mut term = signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = term.recv() => {},
    }

    info!("Signal received, starting graceful shutdown...");
}
