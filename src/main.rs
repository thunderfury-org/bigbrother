use clap::Parser;

use cli::{Cli, Commands};
use common::{config::Manager, state::AppState};
use task::push;

mod cli;
mod common;
mod logger;
mod parser;
mod task;

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Server(args) => {
            run_server(args.data_dir.as_str());
        }
        Commands::Once(args) => {
            run_once(args.data_dir.as_str());
        }
        Commands::Push(push_args) => {
            push(push_args.data_dir.as_str(), &push_args.message);
        }
    }
}

fn init_state(data_dir: &str) -> AppState {
    AppState {
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to create http client"),
        config: Manager::try_from(data_dir.trim()).unwrap(),
    }
}

fn run_server(data_dir: &str) {
    let state = init_state(data_dir);

    // init logger
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("log")
        .max_log_files(3)
        .build(format!("{}/log", state.config.get_data_dir()))
        .expect("initializing rolling file appender failed");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    logger::init(non_blocking);

    new_runtime().block_on(async {
        loop {
            task::run_tasks(&state).await;
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        }
    })
}

fn run_once(data_dir: &str) {
    let state = init_state(data_dir);

    // init logger
    logger::init(std::io::stdout);

    // run
    new_runtime().block_on(task::run_tasks(&state));
}

fn new_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn push(data_dir: &str, message: &str) {
    let state = init_state(data_dir);

    // init logger
    logger::init(std::io::stdout);

    // run
    new_runtime().block_on(push::send(&state, message));
}
