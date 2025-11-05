use clap::Parser;

use cli::{Cli, Commands};

use crate::state::AppState;

mod bot;
mod cli;
mod client;
mod config;
mod error;
mod library;
mod logger;
mod media;
mod server;
mod state;

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
    let state = AppState::try_from(data_dir).expect("Failed to initialize application state");
    logger::init(state.config.get_log_dir().as_str());
    tokio::join!(server::run(state.clone()), bot::run(state.clone()));
}
