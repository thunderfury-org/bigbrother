use sea_orm::DatabaseConnection;

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
    state::AppState,
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
    pub fn from_state(state: AppState) -> Self {
        let bot = state.bot().clone();
        let cache = state.cache().clone();
        let event_bus = state.bus().clone();
        let import_context = import_context(&state);
        let sync_config = sync_config(&state);

        Self {
            log_dir: state.config().get_log_dir(),
            db: state.db().clone(),
            bot: bot.clone(),
            bot_runtime: bot::BotRuntime::new(
                teloxide::types::UserId(
                    state
                        .config()
                        .get_telegram_config()
                        .user_id
                        .try_into()
                        .unwrap(),
                ),
                ManageKeywordsService::new(SeaOrmKeywordRepository::new(state.db().clone())),
                ImportMediaService::new(ImportGateway::new(import_context)),
                PublishTelegramMessageService::new(EventBusPublisher::new(event_bus.clone())),
                SyncStrmService::new(
                    Pan123LibraryRemote::new(state.client().pan123.clone()),
                    TokioFileStore,
                    sync_config,
                ),
            ),
            media_server_addr: state.config().get_media_server_config().get_addr(),
            media_server: crate::server::media::new_router(media_server_context(
                &state,
                cache.clone(),
            )),
            event_bus: event_bus.clone(),
            telegram_delivery: TelegramDeliveryContext {
                bot,
                user_id: state.config().get_telegram_config().user_id,
            },
            cache,
        }
    }
}

fn import_context(state: &AppState) -> ImportContext {
    ImportContext::new(
        state.client().pan115.clone(),
        state.client().pan123.clone(),
        state.client().pan189.clone(),
        state.client().tmdb.clone(),
        state.config().get_library_config().remote_path.clone(),
        state.config().get_library_config().local_path.clone(),
        state
            .config()
            .get_media_server_config()
            .get_strm_download_url(),
    )
}

fn sync_config(state: &AppState) -> crate::application::sync_strm::SyncStrmConfig {
    crate::application::sync_strm::SyncStrmConfig {
        remote_path: state.config().get_library_config().remote_path.clone(),
        local_path: state.config().get_library_config().local_path.clone(),
        strm_download_url: state
            .config()
            .get_media_server_config()
            .get_strm_download_url(),
    }
}

fn media_server_context(state: &AppState, cache: Cache) -> MediaServerContext {
    MediaServerContext::new(
        state
            .config()
            .get_media_server_config()
            .get_strm_path_prefix()
            .to_string(),
        ResolveDownloadUrlService::new(
            StringCacheStore::new(cache),
            Pan123LibraryRemote::new(state.client().pan123.clone()),
        ),
    )
}
