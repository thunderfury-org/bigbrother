use clap::Parser;

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
    let cli = interface::cli::Cli::parse();

    if let interface::cli::Commands::Server(args) = &cli.command {
        if let Err(err) = run_server(&args.data_dir).await {
            eprintln!("Failed to start server: {err}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(err) = interface::cli::run(cli).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run_server(data_dir: &str) -> error::AppResult<()> {
    let app = bootstrap::AppContext::new(data_dir).await?;
    bootstrap::AppRuntime::from_app(app)?.run().await
}
