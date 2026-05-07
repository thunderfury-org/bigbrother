use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use std::time::Duration;

pub mod app;
pub mod services;

pub use app::{AppContext, RuntimeBootstrapInputs};

use crate::{
    application::{
        delete_media::DeleteMediaService, file_index::FileIndexIngestService,
        manage_keywords::ManageKeywordsService, notify::PublishTelegramMessageService,
        sync_strm::SyncStrmService,
    },
    bootstrap::services::{
        FileIndexIngestRuntimeService, MediaDownloadUrlService, build_file_index_service,
        build_import_service_from_clients,
    },
    error::{AppError, AppResult},
    infrastructure::{
        cache::{Cache, string_store::StringCacheStore},
        client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher,
        event_bus::EventBus,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{Pan123MediaSearchGateway, PanLibraryGateway},
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
    pub emby_proxy_addr: Option<String>,
    pub emby_proxy_server: Option<axum::Router>,
}

pub struct EventDeliveryRuntime {
    pub event_bus: EventBus,
    pub telegram_delivery: TelegramDeliveryContext,
    pub file_index_ingest: FileIndexIngestRuntimeService,
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
        let emby_proxy_server = inputs.emby_proxy_config.as_ref().map(|config| {
            http::emby_proxy::new_router(
                http::emby_proxy::EmbyProxyContext::new(
                    config.upstream_base_url.clone(),
                    config.api_key.clone(),
                    config.advertise_base_url.clone(),
                    config.strm_path_prefix.clone(),
                    MediaDownloadUrlService::new(
                        StringCacheStore::new(inputs.cache.clone()),
                        Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
                    ),
                )
                .expect("validated emby proxy config"),
            )
        });
        let emby_proxy_addr = inputs
            .emby_proxy_config
            .as_ref()
            .map(|config| config.addr.clone());
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
                bot_runtime: telegram::BotRuntime::new(telegram::BotRuntimeArgs {
                    user_id,
                    keyword_service: ManageKeywordsService::new(SeaOrmKeywordRepository::new(
                        inputs.db.clone(),
                    )),
                    import_service: build_import_service_from_clients(
                        &inputs.clients,
                        inputs.import_remote_path.clone(),
                        inputs.import_local_path.clone(),
                        inputs.import_strm_download_url.clone(),
                    ),
                    notify_service: PublishTelegramMessageService::new(EventBusPublisher::new(
                        event_bus.clone(),
                    )),
                    sync_service: SyncStrmService::new(
                        Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
                        TokioFileStore,
                        inputs.sync_config.clone(),
                    ),
                    delete_media_service: DeleteMediaService::new(
                        Pan123MediaSearchGateway::new(inputs.clients.pan123.clone()),
                        PanLibraryGateway::new(inputs.clients.pan123.clone()),
                        FilesystemImportLocalStore::new(
                            inputs.import_remote_path.clone(),
                            inputs.import_local_path.clone(),
                            inputs.import_strm_download_url.clone(),
                        ),
                        inputs.import_remote_path.clone(),
                    ),
                    file_index_events: event_bus.clone(),
                    file_index_ingest_dir: inputs.file_index_ingest_dir.clone(),
                }),
                media_server_addr: inputs.media_server_addr.clone(),
                media_server,
                emby_proxy_addr,
                emby_proxy_server,
            },
            event_delivery: EventDeliveryRuntime {
                event_bus,
                telegram_delivery: TelegramDeliveryContext {
                    bot: inputs.bot,
                    user_id: inputs.telegram_user_id,
                },
                file_index_ingest: FileIndexIngestService::new(
                    build_import_service_from_clients(
                        &inputs.clients,
                        inputs.import_remote_path.clone(),
                        inputs.import_local_path.clone(),
                        inputs.import_strm_download_url.clone(),
                    ),
                    build_file_index_service(inputs.db.clone()),
                ),
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
        let mut tasks = tokio::task::JoinSet::new();

        tasks.spawn(http::run(self.media_server_addr, self.media_server));
        tasks.spawn(async move {
            telegram::run(self.bot, self.bot_runtime).await;
            Ok(())
        });

        if let (Some(addr), Some(router)) = (self.emby_proxy_addr, self.emby_proxy_server) {
            tasks.spawn(http::run(addr, router));
        }

        match tasks.join_next().await {
            Some(Ok(result)) => {
                tasks.abort_all();
                result
            }
            Some(Err(err)) => {
                tasks.abort_all();
                Err(AppError::Runtime(format!("server task failed: {err}")))
            }
            None => Ok(()),
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
        self.event_bus
            .subscribe(self.file_index_ingest, on_index_files_from_source)
            .await?;

        info!("Event bus is running");
        shutdown_signal("event bus").await;
        info!("Shutting down event bus...");
        Ok(())
    }
}

async fn on_index_files_from_source(
    service: FileIndexIngestRuntimeService,
    payload: crate::interface::telegram::file_index::IndexFilesFromSource,
) -> AppResult<()> {
    service
        .ingest_sources_from_event(payload.sources, payload.description)
        .await?;
    Ok(())
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
