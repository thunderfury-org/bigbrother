use clap::Parser;

mod application;
mod domain;
mod error;
mod infrastructure;
mod interface;
mod util;

#[tokio::main]
async fn main() {
    let cli = interface::cli::Cli::parse();

    if let Err(err) = interface::cli::run(cli).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
