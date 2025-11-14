use clap::Parser;

use cli::{Cli, Commands};
use migration::Migrator;
use sea_orm_migration::MigratorTrait;

use crate::state::AppState;

mod bot;
mod cli;
mod client;
mod config;
mod error;
mod event_bus;
mod library;
mod logger;
mod media;
mod migration;
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
        Commands::Migrate(args) => {
            run_migrations(args.data_dir.as_str()).await;
        }
    }
}

async fn run_migrations(data_dir: &str) {
    let state = AppState::new(data_dir)
        .await
        .expect("Failed to initialize application state");
    let db = state.db;
    Migrator::up(&db, None).await.unwrap();
}

async fn run_server(data_dir: &str) {
    let state = AppState::new(data_dir)
        .await
        .expect("Failed to initialize application state");
    logger::init(state.config.get_log_dir().as_str());
    tokio::join!(server::run(state.clone()), bot::run(state.clone()));
}
