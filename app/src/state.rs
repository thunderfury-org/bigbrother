use std::sync::Arc;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::{cache, client, config, error::AppResult, event_bus::EventBus};

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
struct InnerAppState {
    pub db: DatabaseConnection,
    pub config: Arc<config::Manager>,
    pub client: Arc<Client>,
    pub bus: Arc<EventBus>,
    pub bot: Arc<teloxide::Bot>,
    pub cache: cache::DatabaseCache,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<InnerAppState>,
}

impl AppState {
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

        Ok(AppState {
            inner: Arc::new(InnerAppState {
                client: Arc::new(Client::new(&config)),
                bus: Arc::new(EventBus::new(db.clone())),
                bot: Arc::new(teloxide::Bot::new(config.get_telegram_config().bot_token.as_str())),
                cache: cache::DatabaseCache::new(db.clone()),
                db,
                config: Arc::new(config),
            }),
        })
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.inner.db
    }

    pub fn config(&self) -> &config::Manager {
        &self.inner.config
    }

    pub fn client(&self) -> &Client {
        &self.inner.client
    }

    pub fn bus(&self) -> &EventBus {
        &self.inner.bus
    }

    pub fn bot(&self) -> &teloxide::Bot {
        &self.inner.bot
    }

    pub fn cache(&self) -> &cache::DatabaseCache {
        &self.inner.cache
    }
}
