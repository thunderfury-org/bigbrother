use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use std::time::Duration;

pub mod app;
pub mod services;

pub use app::{AppContext, RuntimeBootstrapInputs};

use crate::{
    application::{
        import_media::ImportMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, sync_strm::SyncStrmService,
    },
    bootstrap::services::MediaDownloadUrlService,
    error::{AppError, AppResult},
    infrastructure::{
        cache::{Cache, string_store::StringCacheStore},
        client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher,
        event_bus::EventBus,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{PanLibraryGateway, ShareImportGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::keyword::SeaOrmKeywordRepository,
    },
    interface::{
        http,
        http::media::{self, MediaServerContext},
        telegram::{self, delivery::TelegramDeliveryContext},
    },
    logger,
    util::signal::shutdown_signal,
};
use tracing::{error, info};

pub struct AppRuntime {
    pub log_dir: String,
    pub db: DatabaseConnection,
    pub server: ServerRuntime,
    pub event_delivery: EventDeliveryRuntime,
    pub cache_cleanup: CacheCleanupRuntime,
}

pub struct ServerRuntime {
    pub bot: teloxide::Bot,
    pub bot_runtime: telegram::BotRuntime,
    pub media_server_addr: String,
    pub media_server: axum::Router,
}

pub struct EventDeliveryRuntime {
    pub event_bus: EventBus,
    pub telegram_delivery: TelegramDeliveryContext,
}

pub struct CacheCleanupRuntime {
    pub cache: Cache,
    pub interval: Duration,
}

impl AppRuntime {
    pub fn from_app(app: AppContext) -> AppResult<Self> {
        let inputs = app.runtime_inputs();
        let bot = inputs.bot.clone();
        let cache = inputs.cache.clone();
        let event_bus = inputs.event_bus.clone();
        let media_server = media::new_router(media_server_context(&inputs));
        let user_id = inputs
            .telegram_user_id
            .try_into()
            .map(teloxide::types::UserId)
            .map_err(|_| AppError::InvalidParameter("invalid telegram user id".to_owned()))?;

        Ok(Self {
            log_dir: inputs.log_dir.clone(),
            db: inputs.db.clone(),
            server: ServerRuntime {
                bot,
                bot_runtime: telegram::BotRuntime::new(
                    user_id,
                    ManageKeywordsService::new(SeaOrmKeywordRepository::new(inputs.db.clone())),
                    ImportMediaService::new(
                        PanLibraryGateway::new(inputs.clients.pan123.clone()),
                        ShareImportGateway::new(
                            inputs.clients.pan115.clone(),
                            inputs.clients.pan123.clone(),
                            inputs.clients.pan189.clone(),
                        ),
                        TmdbMetadataGateway::new(inputs.clients.tmdb.clone()),
                        FilesystemImportLocalStore::new(
                            inputs.import_remote_path.clone(),
                            inputs.import_local_path.clone(),
                            inputs.import_strm_download_url.clone(),
                        ),
                    ),
                    PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone())),
                    SyncStrmService::new(
                        Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
                        TokioFileStore,
                        inputs.sync_config.clone(),
                    ),
                ),
                media_server_addr: inputs.media_server_addr.clone(),
                media_server,
            },
            event_delivery: EventDeliveryRuntime {
                event_bus,
                telegram_delivery: TelegramDeliveryContext {
                    bot: inputs.bot,
                    user_id: inputs.telegram_user_id,
                },
            },
            cache_cleanup: CacheCleanupRuntime {
                cache,
                interval: Duration::from_hours(12),
            },
        })
    }

    pub async fn run(self) -> AppResult<()> {
        logger::init(self.log_dir.as_str());

        Migrator::up(&self.db, None)
            .await
            .map_err(|err| AppError::Runtime(format!("failed to run migration: {err}")))?;
        let (server_result, event_result, _) = tokio::join!(
            self.server.run(),
            self.event_delivery.run(),
            self.cache_cleanup.run()
        );
        server_result?;
        event_result?;
        Ok(())
    }
}

impl ServerRuntime {
    async fn run(self) -> AppResult<()> {
        let mut http_task = tokio::spawn(http::run(self.media_server_addr, self.media_server));
        let mut bot_task = tokio::spawn(telegram::run(self.bot, self.bot_runtime));

        tokio::select! {
            http_result = &mut http_task => {
                bot_task.abort();
                match http_result {
                    Ok(result) => result,
                    Err(err) => Err(AppError::Runtime(format!("http task failed: {err}"))),
                }
            }
            bot_result = &mut bot_task => {
                match bot_result {
                    Ok(_) => {
                        http_task.abort();
                        Ok(())
                    }
                    Err(err) => Err(AppError::Runtime(format!("telegram task failed: {err}"))),
                }
            }
        }
    }
}

fn media_server_context(inputs: &RuntimeBootstrapInputs) -> MediaServerContext {
    MediaServerContext::new(
        inputs.media_server_strm_path_prefix.clone(),
        MediaDownloadUrlService::new(
            StringCacheStore::new(inputs.cache.clone()),
            Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
        ),
    )
}

impl EventDeliveryRuntime {
    async fn run(self) -> AppResult<()> {
        self.event_bus
            .subscribe(
                self.telegram_delivery,
                crate::interface::telegram::delivery::on_send_telegram_message,
            )
            .await?;

        info!("Event bus is running");
        shutdown_signal("event bus").await;
        info!("Shutting down event bus...");
        Ok(())
    }
}

impl CacheCleanupRuntime {
    async fn run(self) {
        info!(
            "Cache cleanup task started (interval: {} hours)",
            self.interval.as_secs() / 3600
        );

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {
                    match self.cache.clear_expired().await {
                        Ok(count) => info!("Cache cleanup completed: removed {} expired entries", count),
                        Err(e) => error!("Cache cleanup failed: {}", e),
                    }
                }
                _ = shutdown_signal("cache cleanup task") => {
                    info!("Shutting down cache cleanup task...");
                    break;
                }
            }
        }
    }
}
