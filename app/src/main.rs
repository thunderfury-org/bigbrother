use clap::Parser;

use cli::{Cli, Commands};
use tracing::info;

use crate::{state::AppState, util::signal::shutdown_signal};
use migration::{Migrator, MigratorTrait};

mod bot;
mod cli;
mod client;
mod config;
mod entity;
mod error;
mod event;
mod event_bus;
mod library;
mod logger;
mod media;
mod server;
mod state;
mod util;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Server(args) => {
            run_server(args.data_dir.as_str()).await;
        }
    }
}

async fn run_server(data_dir: &str) {
    let state = AppState::new(data_dir)
        .await
        .expect("Failed to initialize application state");
    logger::init(state.config().get_log_dir().as_str());

    Migrator::up(state.db(), None).await.expect("Migration failed");
    tokio::join!(
        server::run(state.clone()),
        bot::run(state.clone()),
        run_event_bus(state.clone())
    );
}

async fn run_event_bus(state: AppState) {
    let bus = state.bus();

    bus.subscribe(state.clone(), bot::handler::on_send_telegram_message)
        .await
        .unwrap();

    info!("Event bus is running");
    // wait for shutdown signal
    shutdown_signal().await;
    info!("Shutting down event bus...");
}
