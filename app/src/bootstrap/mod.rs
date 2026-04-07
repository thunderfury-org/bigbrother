use sea_orm::DatabaseConnection;

pub mod app;

pub use app::AppContext;

use crate::{
    application::{
        import_media::ImportMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, resolve_download_url::ResolveDownloadUrlService,
        sync_strm::SyncStrmService,
    },
    bot::{self, handler::TelegramDeliveryContext},
    cache::Cache,
    event_bus::EventBus,
    infrastructure::{
        cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher, fs::tokio_file_store::TokioFileStore,
        import::gateway::ImportGateway, repo::keyword::SeaOrmKeywordRepository,
    },
    library::import::ImportContext,
    server::media::MediaServerContext,
};

pub struct AppRuntime {
    pub log_dir: String,
    pub db: DatabaseConnection,
    pub bot: teloxide::Bot,
    pub bot_runtime: bot::BotRuntime,
    pub media_server_addr: String,
    pub media_server: axum::Router,
    pub event_bus: EventBus,
    pub telegram_delivery: TelegramDeliveryContext,
    pub cache: Cache,
}

impl AppRuntime {
    pub fn from_app(app: AppContext) -> Self {
        let bot = app.bot().clone();
        let cache = app.cache().clone();
        let event_bus = app.bus().clone();
        let import_context = import_context(&app);
        let sync_config = sync_config(&app);

        Self {
            log_dir: app.config().get_log_dir(),
            db: app.db().clone(),
            bot: bot.clone(),
            bot_runtime: bot::BotRuntime::new(
                teloxide::types::UserId(
                    app.config()
                        .get_telegram_config()
                        .user_id
                        .try_into()
                        .unwrap(),
                ),
                ManageKeywordsService::new(SeaOrmKeywordRepository::new(app.db().clone())),
                ImportMediaService::new(ImportGateway::new(import_context)),
                PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone())),
                SyncStrmService::new(
                    Pan123LibraryRemote::new(app.client().pan123.clone()),
                    TokioFileStore,
                    sync_config,
                ),
            ),
            media_server_addr: app.config().get_media_server_config().get_addr(),
            media_server: crate::server::media::new_router(media_server_context(
                &app,
                cache.clone(),
            )),
            event_bus: event_bus.clone(),
            telegram_delivery: TelegramDeliveryContext {
                bot,
                user_id: app.config().get_telegram_config().user_id,
            },
            cache,
        }
    }
}

fn import_context(app: &AppContext) -> ImportContext {
    ImportContext::new(
        app.client().pan115.clone(),
        app.client().pan123.clone(),
        app.client().pan189.clone(),
        app.client().tmdb.clone(),
        app.config().get_library_config().remote_path.clone(),
        app.config().get_library_config().local_path.clone(),
        app.config()
            .get_media_server_config()
            .get_strm_download_url(),
    )
}

fn sync_config(app: &AppContext) -> crate::application::sync_strm::SyncStrmConfig {
    crate::application::sync_strm::SyncStrmConfig {
        remote_path: app.config().get_library_config().remote_path.clone(),
        local_path: app.config().get_library_config().local_path.clone(),
        strm_download_url: app
            .config()
            .get_media_server_config()
            .get_strm_download_url(),
    }
}

fn media_server_context(app: &AppContext, cache: Cache) -> MediaServerContext {
    MediaServerContext::new(
        app.config()
            .get_media_server_config()
            .get_strm_path_prefix()
            .to_string(),
        ResolveDownloadUrlService::new(
            StringCacheStore::new(cache),
            Pan123LibraryRemote::new(app.client().pan123.clone()),
        ),
    )
}
