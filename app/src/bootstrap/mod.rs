use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use std::time::Duration;

pub mod app;
pub mod services;

pub use app::{AppContext, RuntimeBootstrapInputs};

use crate::{
    application::{
        delete_media::DeleteMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, sync_strm::SyncStrmService,
    },
    bootstrap::services::{MediaDownloadUrlService, build_import_service_from_clients},
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
        let emby_proxy_server = inputs
            .emby_proxy_config
            .as_ref()
            .map(|config| {
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
                .map(http::emby_proxy::new_router)
            })
            .transpose()?;
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
                bot_runtime: telegram::BotRuntime::new(
                    user_id,
                    ManageKeywordsService::new(SeaOrmKeywordRepository::new(inputs.db.clone())),
                    build_import_service_from_clients(
                        &inputs.clients,
                        inputs.import_remote_path.clone(),
                        inputs.import_local_path.clone(),
                        inputs.import_strm_download_url.clone(),
                    ),
                    PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone())),
                    SyncStrmService::new(
                        Pan123LibraryRemote::new(inputs.clients.pan123.clone()),
                        TokioFileStore,
                        inputs.sync_config.clone(),
                    ),
                    DeleteMediaService::new(
                        Pan123MediaSearchGateway::new(inputs.clients.pan123.clone()),
                        PanLibraryGateway::new(inputs.clients.pan123.clone()),
                        FilesystemImportLocalStore::new(
                            inputs.import_remote_path.clone(),
                            inputs.import_local_path.clone(),
                            inputs.import_strm_download_url.clone(),
                        ),
                        inputs.import_remote_path.clone(),
                    ),
                ),
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::error::AppError;

    fn unique_temp_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("bigbrother-runtime-{suffix}"))
    }

    #[tokio::test]
    async fn from_app_returns_error_for_invalid_emby_proxy_upstream_url() {
        let data_dir = unique_temp_dir();
        let config_dir = data_dir.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.yaml"),
            r#"
emby_proxy:
  enable: true
  upstream_base_url: http://
"#,
        )
        .unwrap();

        let app = AppContext::new(data_dir.to_str().unwrap()).await.unwrap();
        let result = AppRuntime::from_app(app);

        assert!(matches!(
            result,
            Err(AppError::InvalidParameter(message))
                if message.contains("invalid emby upstream url")
        ));

        let _ = fs::remove_dir_all(data_dir);
    }
}
