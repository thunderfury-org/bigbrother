use trace_id::TraceIdLayer;
use tracing::info;

use crate::{
    infrastructure::{
        cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
    },
    state::AppState,
    util::signal::shutdown_signal,
};

mod log;
mod media;

pub async fn run(state: AppState) {
    let addr = state.config().get_media_server_config().get_addr();
    info!("Starting media server at {}", addr);
    let media_ctx = media::MediaServerContext {
        path_prefix: state
            .config()
            .get_media_server_config()
            .get_strm_path_prefix()
            .to_string(),
        cache: StringCacheStore::new(state.cache().clone()),
        remote: Pan123LibraryRemote::new(state.client().pan123.clone()),
    };

    let app = media::new_router(media_ctx)
        .layer(log::LogLayer)
        .layer(TraceIdLayer::new_high_performance());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal("media server"))
        .await
        .unwrap();
    info!("Media server has shutdown gracefully.");
}
