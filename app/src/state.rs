use std::sync::Arc;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::{
    client::{pan123, pan189, tmdb},
    config,
    error::AppResult,
    event_bus::EventBus,
};

#[derive(Clone)]
struct InnerAppState {
    pub db: DatabaseConnection,
    pub config: Arc<config::Manager>,
    pub pan123: Arc<pan123::Client>,
    pub pan189: Arc<pan189::Client>,
    pub tmdb: Arc<tmdb::Client>,
    pub bus: Arc<EventBus>,
    pub bot: Arc<teloxide::Bot>,
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
                pan123: Arc::new(pan123::Client::new(
                    &config.get_pan123_config().client_id,
                    &config.get_pan123_config().client_secret,
                    &format!("{}/pan123", config.get_cache_dir()),
                )),
                pan189: Arc::new(pan189::Client::new()),
                tmdb: Arc::new(tmdb::Client::new(&config.get_tmdb_config().api_key)),
                bus: Arc::new(EventBus::new(db.clone())),
                bot: Arc::new(teloxide::Bot::new(config.get_telegram_config().bot_token.as_str())),
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

    pub fn pan123(&self) -> &pan123::Client {
        &self.inner.pan123
    }

    pub fn pan189(&self) -> &pan189::Client {
        &self.inner.pan189
    }

    pub fn tmdb(&self) -> &tmdb::Client {
        &self.inner.tmdb
    }

    pub fn bus(&self) -> &EventBus {
        &self.inner.bus
    }

    pub fn bot(&self) -> &teloxide::Bot {
        &self.inner.bot
    }
}
