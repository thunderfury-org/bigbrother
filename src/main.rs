use clap::Parser;

use cli::{Cli, Commands};

use crate::state::AppState;

mod bot;
mod cli;
mod client;
mod config;
mod error;
mod logger;
mod media;
mod state;
mod task;

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
    logger::init(std::io::stdout);
    let state = AppState::try_from(data_dir).expect("Failed to initialize application state");

    bot::run_bot(state).await;
}
