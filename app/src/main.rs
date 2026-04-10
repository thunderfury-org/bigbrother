use bootstrap::{AppContext, AppRuntime};
use clap::Parser;
use error::AppResult;
use interface::cli::{Cli, Commands};

mod application;
mod bootstrap;
mod config;
mod domain;
mod error;
mod infrastructure;
mod interface;
mod logger;
mod util;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Server(args) => {
            if let Err(err) = run_server(args.data_dir.as_str()).await {
                eprintln!("Failed to start server: {err}");
                std::process::exit(1);
            }
        }
    }
}

async fn run_server(data_dir: &str) -> AppResult<()> {
    let app = AppContext::new(data_dir).await?;
    AppRuntime::from_app(app)?.run().await
}
