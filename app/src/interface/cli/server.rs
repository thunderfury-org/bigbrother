use std::time::Duration;

use migration::{Migrator, MigratorTrait};
use tracing::{error, info};

use crate::{
    application::{
        delete_media::DeleteMediaService,
        import::MetadataLookup,
        manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService,
        share_crawler::ShareCrawler,
        sync_strm::{SyncStrmConfig, SyncStrmService},
    },
    error::{AppError, AppResult},
    infrastructure::{
        cache::{Cache, string_store::StringCacheStore},
        client::{self, library_remote::Pan123LibraryRemote},
        event::publisher::EventBusPublisher,
        event_bus::EventBus,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{
                Pan123MediaSearchGateway, PanLibraryGateway, ShareImportGateway,
                TmdbMetadataGateway,
            },
            local_store::FilesystemImportLocalStore,
        },
        repo::{file_index::SeaOrmFileIndexRepository, keyword::SeaOrmKeywordRepository},
        services::{FileIndexRuntimeService, ImportService, MediaDownloadUrlService},
    },
    interface::{
        http,
        http::media::{self, MediaServerContext},
        telegram::{
            self,
            delivery::TelegramDeliveryContext,
            handler::{ProcessMediaSourcesHandler, on_process_media_sources},
        },
    },
    util::signal::shutdown_signal,
};

use super::{config, connect_db, logger};

pub(super) async fn run(data_dir: &str) -> AppResult<()> {
    let config = config::Manager::try_from(data_dir.trim())?;

    logger::init(config.get_log_dir().as_str());

    let db = connect_db(&config.get_db_dir()).await?;

    // Infrastructure
    let event_bus = EventBus::new(db.clone());
    let bot = teloxide::Bot::new(config.get_telegram_config().bot_token.as_str());
    let cache = Cache::new(db.clone());

    // API clients
    let pan115 = client::pan115::Client::new();
    let pan123 = client::pan123::Client::new(
        &config.get_pan123_config().passport,
        &config.get_pan123_config().password,
        &format!("{}/pan123", config.get_cache_dir()),
    );
    let pan189 = client::pan189::Client::new(client::pan189::AuthConfig {
        username: config.get_pan189_config().username.clone(),
        password: config.get_pan189_config().password.clone(),
        cache_dir: format!("{}/pan189", config.get_cache_dir()),
    });
    let quark = client::quark::Client::new(&config.get_quark_config().cookie);
    let tmdb = client::tmdb::Client::new(&config.get_tmdb_config().api_key);

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

    // Database migration
    Migrator::up(&db, None)
        .await
        .map_err(|err| AppError::Database(format!("failed to run migration: {err}"), false))?;

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
        keyword_service: ManageKeywordsService::new(SeaOrmKeywordRepository::new(db.clone())),
        notify_service: PublishTelegramMessageService::new(EventBusPublisher::new(
            event_bus.clone(),
        )),
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
    let keyword_service = ManageKeywordsService::new(SeaOrmKeywordRepository::new(db.clone()));
    let notify_service =
        PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone()));
    let event_delivery_media_handler = ProcessMediaSourcesHandler {
        file_index_service: FileIndexRuntimeService::new(SeaOrmFileIndexRepository::new(
            db.clone(),
        )),
        share_crawler: ShareCrawler::new(ShareImportGateway::new(
            pan115,
            pan123.clone(),
            pan189,
            quark,
        )),
        import_service: ImportService::new(
            PanLibraryGateway::new(pan123.clone()),
            TmdbMetadataGateway::new(tmdb),
            FilesystemImportLocalStore::new(
                library.remote_path,
                library.local_path,
                library.strm_download_url,
            ),
        ),
        metadata_lookup: MetadataLookup::default(),
        notify_service,
        keyword_service,
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

    // Concurrent run
    let event_bus_for_delivery = event_bus.clone();
    let (server_result, event_result, _) = tokio::join!(
        // Server: HTTP + Telegram bot + optional Emby proxy
        async move {
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(http::run(media_server_addr, media_server));
            tasks.spawn(async move {
                telegram::run(bot, bot_runtime).await;
                Ok(())
            });
            if let (Some(addr), Some(router)) = (emby_proxy_addr, emby_proxy_router) {
                tasks.spawn(http::run(addr, router));
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
