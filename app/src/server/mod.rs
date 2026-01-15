use tower_http::{
    LatencyUnit,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use trace_id::TraceIdLayer;
use tracing::{Level, info};

use crate::{state::AppState, util::signal::shutdown_signal};

mod media;

pub async fn run(state: AppState) {
    let addr = state.config().get_media_server_config().get_addr();
    info!("Starting media server at {}", addr);

    let app = media::new_router(state.clone()).layer(TraceIdLayer::new()).layer(
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
