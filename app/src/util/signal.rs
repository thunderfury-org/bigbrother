use tokio::signal::{
    self,
    unix::{SignalKind, signal},
};
use tracing::info;

pub async fn shutdown_signal() {
    let mut term = signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = term.recv() => {},
    }

    info!("Signal received, starting graceful shutdown...");
}
