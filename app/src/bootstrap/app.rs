use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::{
    application::sync_strm::SyncStrmConfig,
    config,
    error::AppResult,
    infrastructure::{cache::Cache, client, event_bus::EventBus},
    library::import::ImportPathConfig,
};

/// Unified client struct containing all API clients
#[derive(Clone)]
pub struct Client {
    pub pan115: client::pan115::Client,
    pub pan123: client::pan123::Client,
    pub pan189: client::pan189::Client,
    pub tmdb: client::tmdb::Client,
}

impl Client {
    pub fn new(config: &config::Manager) -> Self {
        Self {
            pan115: client::pan115::Client::new(),
            pan123: client::pan123::Client::new(
                &config.get_pan123_config().passport,
                &config.get_pan123_config().password,
                &format!("{}/pan123", config.get_cache_dir()),
            ),
            pan189: client::pan189::Client::new(),
            tmdb: client::tmdb::Client::new(&config.get_tmdb_config().api_key),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeBootstrapInputs {
    pub db: DatabaseConnection,
    pub bot: teloxide::Bot,
    pub cache: Cache,
    pub event_bus: EventBus,
    pub clients: Client,
    pub log_dir: String,
    pub media_server_addr: String,
    pub media_server_strm_path_prefix: String,
    pub telegram_user_id: i64,
    pub import_paths: ImportPathConfig,
    pub sync_config: SyncStrmConfig,
}

#[derive(Clone)]
pub struct AppContext {
    inputs: RuntimeBootstrapInputs,
}

impl AppContext {
    pub async fn new(data_dir: &str) -> AppResult<Self> {
        let config = config::Manager::try_from(data_dir.trim())?;

        let db_dir = config.get_db_dir();
        if !std::fs::exists(db_dir.as_str())? {
            std::fs::create_dir_all(db_dir.as_str())?;
        }

        let conn_str = format!("sqlite:{}/data.db?mode=rwc", db_dir);
        let mut opt = ConnectOptions::new(conn_str);
        opt.sqlx_logging(false);
        let db = Database::connect(opt).await?;
        let clients = Client::new(&config);
        let event_bus = EventBus::new(db.clone());
        let bot = teloxide::Bot::new(config.get_telegram_config().bot_token.as_str());
        let cache = Cache::new(db.clone());

        Ok(AppContext {
            inputs: RuntimeBootstrapInputs {
                db,
                bot,
                cache,
                event_bus,
                clients,
                log_dir: config.get_log_dir(),
                media_server_addr: config.get_media_server_config().get_addr(),
                media_server_strm_path_prefix: config
                    .get_media_server_config()
                    .get_strm_path_prefix()
                    .to_string(),
                telegram_user_id: config.get_telegram_config().user_id,
                import_paths: ImportPathConfig::new(
                    config.get_library_config().remote_path.clone(),
                    config.get_library_config().local_path.clone(),
                    config.get_media_server_config().get_strm_download_url(),
                ),
                sync_config: SyncStrmConfig {
                    remote_path: config.get_library_config().remote_path.clone(),
                    local_path: config.get_library_config().local_path.clone(),
                    strm_download_url: config.get_media_server_config().get_strm_download_url(),
                },
            },
        })
    }

    pub fn runtime_inputs(&self) -> RuntimeBootstrapInputs {
        self.inputs.clone()
    }
}
