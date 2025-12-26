use clap::Parser;

use cli::{Cli, Commands};

use crate::state::AppState;
use migration::{Migrator, MigratorTrait};

mod bot;
mod cli;
mod client;
mod config;
mod entity;
mod error;
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
    logger::init(state.config.get_log_dir().as_str());

    Migrator::up(&state.db, None).await.expect("Migration failed");
    tokio::join!(server::run(state.clone()), bot::run(state.clone()));
}
