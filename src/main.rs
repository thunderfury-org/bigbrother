use clap::Parser;

use cli::{Cli, Commands};
use common::{config::Manager, state::AppState};

mod bot;
mod cli;
mod client;
mod common;
mod logger;
mod media;
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

fn init_state(data_dir: &str) -> AppState {
    AppState {
        config: Manager::try_from(data_dir.trim()).unwrap(),
    }
}

async fn run_server(data_dir: &str) {
    logger::init(std::io::stdout);
    let state = init_state(data_dir);

    bot::run_bot(state.config.get_telegram_config().bot_token.as_str()).await;
}
