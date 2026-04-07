use std::time::Duration;

use bootstrap::{AppContext, AppRuntime};
use clap::Parser;
use infrastructure::event_bus::EventBus;
use interface::{
    cli::{Cli, Commands},
    http,
    telegram::{self, delivery::TelegramDeliveryContext},
};
use tracing::{error, info};

use migration::{Migrator, MigratorTrait};
use util::signal::shutdown_signal;

mod application;
mod bootstrap;
mod cache;
mod client;
mod config;
mod domain;
mod entity;
mod error;
mod event;
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
    let runtime = AppRuntime::from_app(app);
    logger::init(runtime.log_dir.as_str());

    Migrator::up(&runtime.db, None)
        .await
        .expect("Migration failed");
    tokio::join!(
        http::run(runtime.media_server_addr, runtime.media_server),
        telegram::run(runtime.bot, runtime.bot_runtime),
        run_event_bus(runtime.event_bus, runtime.telegram_delivery),
        run_cache_cleanup(runtime.cache)
    );
}

async fn run_event_bus(bus: EventBus, delivery_ctx: TelegramDeliveryContext) {
    bus.subscribe(
        delivery_ctx,
        interface::telegram::delivery::on_send_telegram_message,
    )
        .await
        .unwrap();

    info!("Event bus is running");
    // wait for shutdown signal
    shutdown_signal("event bus").await;
    info!("Shutting down event bus...");
}

async fn run_cache_cleanup(cache: cache::Cache) {
    let interval = Duration::from_hours(12);

    info!("Cache cleanup task started (interval: 12 hours)");

    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                match cache.clear_expired().await {
                    Ok(count) => {
                        info!("Cache cleanup completed: removed {} expired entries", count);
                    }
                    Err(e) => {
                        error!("Cache cleanup failed: {}", e);
                    }
                }
            }
            _ = shutdown_signal("cache cleanup task") => {
                info!("Shutting down cache cleanup task...");
                break;
            }
        }
    }
}
