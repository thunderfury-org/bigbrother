use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use std::time::Duration;
use teloxide::net::Download;
use teloxide::prelude::Requester;
use tracing::{error, info, warn};

pub mod app;
pub mod services;

pub use app::{AppContext, RuntimeBootstrapInputs};

use crate::{
    application::{
        delete_media::DeleteMediaService,
        file_index::{SeenFile, is_permanent_index_source_error},
        manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService,
        sync_strm::SyncStrmService,
    },
    bootstrap::services::{
        FileIndexRuntimeService, ImportService, KeywordService, MediaDownloadUrlService,
        NotifyService, build_file_index_service, build_import_service_from_clients,
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
        telegram::{
            self,
            delivery::TelegramDeliveryContext,
            file_index::{
                MediaSource, ProcessMediaSources, send_import_error, send_import_results,
            },
        },
    },
    logger,
    util::signal::shutdown_signal,
};

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
    pub media_handler: ProcessMediaSourcesHandler,
}

#[derive(Clone)]
pub struct ProcessMediaSourcesHandler {
    pub file_index_service: FileIndexRuntimeService,
    pub import_service: ImportService,
    pub notify_service: NotifyService,
    pub keyword_service: KeywordService,
    pub bot: teloxide::Bot,
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

        let keyword_service =
            ManageKeywordsService::new(SeaOrmKeywordRepository::new(inputs.db.clone()));
        let import_service = build_import_service_from_clients(
            &inputs.clients,
            inputs.import_remote_path.clone(),
            inputs.import_local_path.clone(),
            inputs.import_strm_download_url.clone(),
        );
        let notify_service =
            PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone()));

        Ok(Self {
            log_dir: inputs.log_dir.clone(),
            db: inputs.db.clone(),
            server: ServerRuntime {
                bot: bot.clone(),
                bot_runtime: telegram::BotRuntime::new(telegram::BotRuntimeArgs {
                    user_id,
                    keyword_service: keyword_service.clone(),
                    notify_service: notify_service.clone(),
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
                    event_bus: event_bus.clone(),
                }),
                media_server_addr: inputs.media_server_addr.clone(),
                media_server,
                emby_proxy_addr,
                emby_proxy_server,
            },
            event_delivery: EventDeliveryRuntime {
                event_bus,
                telegram_delivery: TelegramDeliveryContext {
                    bot: bot.clone(),
                    user_id: inputs.telegram_user_id,
                },
                media_handler: ProcessMediaSourcesHandler {
                    file_index_service: build_file_index_service(inputs.db.clone()),
                    import_service,
                    notify_service,
                    keyword_service,
                    bot,
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
        self.event_bus
            .subscribe(self.media_handler, on_process_media_sources)
            .await?;

        info!("Event bus is running");
        shutdown_signal("event bus").await;
        info!("Shutting down event bus...");
        Ok(())
    }
}

async fn on_process_media_sources(
    handler: ProcessMediaSourcesHandler,
    payload: ProcessMediaSources,
) -> AppResult<()> {
    let reply_to = payload.reply_to_message_id;
    let description = payload.description.clone();
    let error_prefix = match &payload.source {
        MediaSource::ShareUrl(_) => "分享处理失败",
        MediaSource::Fslink(_) => "秒传处理失败",
        MediaSource::TgDocument { .. } => "JSON/CAS 文件处理失败",
    };

    // Step 1: Fetch raw files (source-specific)
    let raw_files = fetch_raw_files(&handler, &payload.source, reply_to, error_prefix).await?;
    let Some(raw_files) = raw_files else {
        return Ok(());
    };

    // Step 2: Index
    let seen: Vec<SeenFile> = raw_files.iter().map(SeenFile::from_raw_file).collect();
    if let Err(err) = handler
        .file_index_service
        .record_seen_files(seen, description)
        .await
    {
        warn!(error = %err, "file index record failed (non-blocking)");
    }

    // Step 3: Import
    if should_import(
        &handler.keyword_service,
        payload.channel_post,
        &payload.description,
    )
    .await
    {
        match handler
            .import_service
            .import_with_raw_files(raw_files)
            .await
        {
            Ok(imported) => {
                send_import_results(&handler.notify_service, reply_to, &imported).await;
            }
            Err(err) if is_permanent_index_source_error(&err) => {
                send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

/// Fetch raw files from the source. Returns Ok(None) if the source should be skipped (permanent error).
async fn fetch_raw_files(
    handler: &ProcessMediaSourcesHandler,
    source: &MediaSource,
    reply_to: Option<i32>,
    error_prefix: &str,
) -> AppResult<Option<Vec<crate::domain::import::inner::RawFile>>> {
    let result = match source {
        MediaSource::ShareUrl(url) => {
            let parsed_url = url::Url::parse(url)
                .map_err(|e| AppError::InvalidParameter(format!("invalid share url: {e}")))?;
            let share_url =
                crate::application::import::ShareUrl::from(&parsed_url).ok_or_else(|| {
                    AppError::InvalidParameter(format!("unsupported share url: {url}"))
                })?;
            handler
                .import_service
                .raw_files_from_share_url(&share_url)
                .await
        }
        MediaSource::Fslink(fslink) => handler.import_service.raw_files_from_fslink(fslink),
        MediaSource::TgDocument { file_id, file_name } => {
            return fetch_tg_document(handler, file_id, file_name, reply_to, error_prefix).await;
        }
    };

    match result {
        Ok(files) => Ok(Some(files)),
        Err(err) if is_permanent_index_source_error(&err) => {
            warn!(error = %err, "skipping permanent error");
            send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn fetch_tg_document(
    handler: &ProcessMediaSourcesHandler,
    file_id: &str,
    file_name: &str,
    reply_to: Option<i32>,
    error_prefix: &str,
) -> AppResult<Option<Vec<crate::domain::import::inner::RawFile>>> {
    let file = handler
        .bot
        .get_file(teloxide::types::FileId(file_id.to_string()))
        .await
        .map_err(|e| AppError::Dependency(format!("failed to get document: {e}")))?;

    if file.meta.size > 10 * 1024 * 1024 {
        let err = AppError::InvalidParameter(format!(
            "Telegram document too large ({file_name}): {} bytes exceeds 10MB limit",
            file.meta.size
        ));
        warn!(file_name = %file_name, size = file.meta.size, "document too large");
        send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
        return Ok(None);
    }

    let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
    handler
        .bot
        .download_file(&file.path, &mut content)
        .await
        .map_err(|e| AppError::Dependency(format!("failed to download document: {e}")))?;

    match handler.import_service.raw_files_from_json(content) {
        Ok(files) => Ok(Some(files)),
        Err(err) => {
            warn!(file_name = %file_name, error = %err, "failed to parse document");
            send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
            Ok(None)
        }
    }
}

async fn should_import(
    keyword_service: &KeywordService,
    channel_post: bool,
    description: &Option<String>,
) -> bool {
    if !channel_post {
        return true;
    }

    let keywords = match keyword_service.list_values().await {
        Ok(keywords) if !keywords.is_empty() => keywords,
        _ => return false,
    };

    let text = description.as_deref().unwrap_or_default();
    keywords.iter().any(|kw| text.contains(kw))
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
