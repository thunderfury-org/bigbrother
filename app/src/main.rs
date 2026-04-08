use bootstrap::{AppContext, AppRuntime};
use clap::Parser;
use interface::cli::{Cli, Commands};

mod application;
mod bootstrap;
mod config;
mod domain;
mod error;
mod infrastructure;
mod interface;
mod library;
mod logger;
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
    let app = AppContext::new(data_dir)
        .await
        .expect("Failed to initialize application state");
    AppRuntime::from_app(app).run().await;
}
