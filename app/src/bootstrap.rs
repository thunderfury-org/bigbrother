use sea_orm::DatabaseConnection;

use crate::{
    bot::{self, handler::TelegramDeliveryContext},
    cache::Cache,
    event_bus::EventBus,
    infrastructure::{
        cache::string_store::StringCacheStore, client::library_remote::Pan123LibraryRemote,
    },
    server::media::MediaServerContext,
    state::AppState,
};

pub struct AppRuntime {
    pub log_dir: String,
    pub db: DatabaseConnection,
    pub bot: teloxide::Bot,
    pub bot_runtime: bot::BotRuntime,
    pub media_server_addr: String,
    pub media_server_ctx: MediaServerContext,
    pub event_bus: EventBus,
    pub telegram_delivery: TelegramDeliveryContext,
    pub cache: Cache,
}

impl AppRuntime {
    pub fn from_state(state: AppState) -> Self {
        let bot = state.bot().clone();
        let cache = state.cache().clone();
        let event_bus = state.bus().clone();

        Self {
            log_dir: state.config().get_log_dir(),
            db: state.db().clone(),
            bot: bot.clone(),
            bot_runtime: bot::BotRuntime::from_state(state.clone()),
            media_server_addr: state.config().get_media_server_config().get_addr(),
            media_server_ctx: MediaServerContext {
                path_prefix: state
                    .config()
                    .get_media_server_config()
                    .get_strm_path_prefix()
                    .to_string(),
                cache: StringCacheStore::new(cache.clone()),
                remote: Pan123LibraryRemote::new(state.client().pan123.clone()),
            },
            event_bus: event_bus.clone(),
            telegram_delivery: TelegramDeliveryContext {
                bot,
                user_id: state.config().get_telegram_config().user_id,
            },
            cache,
        }
    }
}
