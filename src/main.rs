use clap::Parser;

use cli::{Cli, Commands};
use common::{config::Manager, state::AppState};
use futures::StreamExt;
use teloxide::{net::Download, prelude::*};

mod cli;
mod cmd;
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
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to create http client"),
        config: Manager::try_from(data_dir.trim()).unwrap(),
    }
}

async fn run_server(data_dir: &str) {
    logger::init(std::io::stdout);
    let state = init_state(data_dir);

    let bot = Bot::new(state.config.get_app_config().telegram.token.as_str());
    teloxide::repl(bot, |bot: Bot, msg: Message| async move {
        if let Some(text) = msg.text() {
            bot.send_message(msg.chat.id, text).await?;
        } else if let Some(doc) = msg.document() {
            let file = bot.get_file(doc.file.id.to_owned()).await?;
            let mut content = Vec::new();
            bot.download_file(&file.path, &mut content).await?;
            bot.send_message(msg.chat.id, format!("document: {}", String::from_utf8_lossy(&content))).await?;
        }
        Ok(())
    })
    .await;
}
