use std::sync::Arc;

use clap::Parser;

use cli::{Cli, Commands};

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

fn init_state(data_dir: &str) -> state::AppState {
    let config = config::Manager::try_from(data_dir.trim()).unwrap();
    state::AppState {
        pan123: Arc::new(client::pan123::Client::new(
            &config.get_pan123_config().client_id,
            &config.get_pan123_config().client_secret,
            &format!("{}/pan123", config.get_cache_dir()),
        )),
        config,
    }
}

async fn run_server(data_dir: &str) {
    logger::init(std::io::stdout);
    let state = init_state(data_dir);

    bot::run_bot(state).await;
}
