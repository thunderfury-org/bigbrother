use std::time::Duration;

use tracing::{error, info};

use crate::{
    application::{
        delete_media::DeleteMediaService,
        media_source_observation::ProcessObservationService,
        recorded_import::RecordedImportService,
        sync_strm::{SyncStrmConfig, SyncStrmService},
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
        services::MediaDownloadUrlService,
    },
    interface::{
        http,
        http::console::{self, ConsoleContext},
        http::media::{self, MediaServerContext},
        telegram::{
            self,
            delivery::TelegramDeliveryContext,
            handler::{ProcessMediaSourcesHandler, on_process_media_sources},
        },
    },
    util::signal::shutdown_signal,
};

use super::{context::CliContext, logger};

pub(super) async fn run(data_dir: &str) -> AppResult<()> {
    let ctx = CliContext::new(data_dir)?;
    let config = ctx.config();

    logger::init(config.get_log_dir().as_str());

    let db = ctx.db().await?.clone();

    // Infrastructure
    let event_bus = EventBus::new(db.clone());
    let bot = teloxide::Bot::new(config.get_telegram_config().bot_token.as_str());
    let cache = Cache::new(db.clone());
    let pan123 = ctx.pan123();

    // Telegram
    let user_id = config
        .get_telegram_config()
        .user_id
        .try_into()
        .map(teloxide::types::UserId)
        .map_err(|_| AppError::InvalidParameter("invalid telegram user id".to_owned()))?;

    // Emby proxy
    let emby_proxy_config = if config.get_emby_proxy_config().is_enabled() {
        let upstream_base_url = config
            .get_emby_proxy_config()
            .get_upstream_base_url()
            .ok_or_else(|| {
                AppError::InvalidParameter(
                    "emby_proxy.upstream_base_url is required when emby_proxy.enable is true"
                        .to_string(),
                )
            })?;
        Some(EmbyProxyConfig {
            addr: config.get_emby_proxy_config().get_addr(),
            upstream_base_url,
            api_key: config
                .get_emby_proxy_config()
                .get_api_key()
                .map(str::to_owned),
            advertise_base_url: config.get_media_server_config().get_advertise_base_url(),
            strm_path_prefix: config
                .get_media_server_config()
                .get_strm_path_prefix()
                .to_string(),
        })
    } else {
        None
    };

    // Library
    let library = SyncStrmConfig {
        remote_path: config.get_library_config().remote_path.clone(),
        local_path: config.get_library_config().local_path.clone(),
        strm_download_url: config.get_media_server_config().get_strm_download_url(),
    };
    let media_server_addr = config.get_media_server_config().get_addr();
    let media_server_strm_path_prefix = config
        .get_media_server_config()
        .get_strm_path_prefix()
        .to_string();

    // Media server
    let media_server = media::new_router(MediaServerContext::new(
        media_server_strm_path_prefix,
        MediaDownloadUrlService::new(
            StringCacheStore::new(cache.clone()),
            Pan123LibraryRemote::new(pan123.clone()),
        ),
    ));

    // Telegram bot runtime
    let bot_runtime = telegram::BotRuntime::new(telegram::BotRuntimeArgs {
        user_id,
        notify_service: EventBusPublisher::new(event_bus.clone()),
        sync_service: SyncStrmService::new(
            Pan123LibraryRemote::new(pan123.clone()),
            TokioFileStore,
            library.clone(),
        ),
        delete_media_service: DeleteMediaService::new(
            Pan123MediaSearchGateway::new(pan123.clone()),
            PanLibraryGateway::new(pan123.clone()),
            FilesystemImportLocalStore::new(
                library.remote_path.clone(),
                library.local_path.clone(),
                library.strm_download_url.clone(),
            ),
            library.remote_path.clone(),
        ),
        event_bus: event_bus.clone(),
    });

    // Event delivery
    let event_delivery_telegram = TelegramDeliveryContext {
        bot: bot.clone(),
        user_id: config.get_telegram_config().user_id,
    };
    let subscription_repo = ctx.subscription_repo().await?;
    let notify_service = EventBusPublisher::new(event_bus.clone());
    let event_delivery_media_handler = ProcessMediaSourcesHandler {
        processor: ProcessObservationService::new(
            ctx.file_index_service().await?,
            RecordedImportService::new(ctx.import_record_repository().await?),
            ctx.identify_service().await?,
            ctx.import_service().await?,
            subscription_repo,
        ),
        notify_service,
        share_resolver: ctx.share_resolver(),
        bot: bot.clone(),
    };

    // Emby proxy server
    let emby_proxy_router = emby_proxy_config.as_ref().map(|config| {
        http::emby_proxy::new_router(
            http::emby_proxy::EmbyProxyContext::new(
                config.upstream_base_url.clone(),
                config.api_key.clone(),
                config.advertise_base_url.clone(),
                config.strm_path_prefix.clone(),
                MediaDownloadUrlService::new(
                    StringCacheStore::new(cache.clone()),
                    Pan123LibraryRemote::new(pan123),
                ),
            )
            .expect("validated emby proxy config"),
        )
    });
    let emby_proxy_addr = emby_proxy_config.map(|config| config.addr);

    // Console
    let (console_addr, console_router) = if config.get_console_config().is_enabled() {
        let repo = ctx.import_record_repository().await?;
        let file_index_service = ctx.file_index_service().await?;
        let import_service = ctx.import_service().await?;
        let identify_service = ctx.identify_service().await?;
        let (subscription_service, subscription_repo) = ctx.subscription_service().await?;
        (
            Some(config.get_console_config().get_addr()),
            Some(console::new_router(ConsoleContext::new(
                repo,
                file_index_service,
                import_service,
                identify_service,
                subscription_service,
                subscription_repo,
            ))),
        )
    } else {
        (None, None)
    };

    // Concurrent run
    let event_bus_for_delivery = event_bus.clone();
    let (server_result, event_result, _) = tokio::join!(
        // Server: HTTP + Telegram bot + optional Emby proxy + optional Console
        async move {
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(http::run("media server", media_server_addr, media_server));
            tasks.spawn(async move {
                telegram::run(bot, bot_runtime).await;
                Ok(())
            });
            if let (Some(addr), Some(router)) = (emby_proxy_addr, emby_proxy_router) {
                tasks.spawn(http::run("emby proxy", addr, router));
            }
            if let (Some(addr), Some(router)) = (console_addr, console_router) {
                tasks.spawn(http::run("console", addr, router));
            }
            match tasks.join_next().await {
                Some(Ok(result)) => {
                    tasks.abort_all();
                    result
                }
                Some(Err(err)) => {
                    tasks.abort_all();
                    Err(AppError::Internal(format!("server task failed: {err}")))
                }
                None => Ok(()),
            }
        },
        // Event delivery
        async move {
            event_bus_for_delivery
                .subscribe(
                    event_delivery_telegram,
                    crate::interface::telegram::delivery::on_send_telegram_message,
                )
                .await?;
            event_bus_for_delivery
                .subscribe(event_delivery_media_handler, on_process_media_sources)
                .await?;
            info!("Event bus is running");
            shutdown_signal("event bus").await;
            info!("Shutting down event bus...");
            Ok::<(), AppError>(())
        },
        // Cache cleanup
        async {
            info!("Cache cleanup task started (interval: 12 hours)");
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_hours(12)) => {
                        match cache.clear_expired().await {
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
    );
    server_result?;
    event_result?;
    Ok(())
}

#[derive(Clone)]
struct EmbyProxyConfig {
    addr: String,
    upstream_base_url: String,
    api_key: Option<String>,
    advertise_base_url: String,
    strm_path_prefix: String,
}
