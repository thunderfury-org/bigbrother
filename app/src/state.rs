use std::sync::Arc;

use sea_orm::{Database, DatabaseConnection};

use crate::{
    client::{pan123, pan189, tmdb},
    config,
    error::AppResult,
};

#[derive(Clone, Default)]
pub struct AppState {
    pub _db: DatabaseConnection,
    pub config: Arc<config::Manager>,
    pub pan123: Arc<pan123::Client>,
    pub pan189: Arc<pan189::Client>,
    pub tmdb: Arc<tmdb::Client>,
}

impl AppState {
    pub async fn new(data_dir: &str) -> AppResult<Self> {
        let config = config::Manager::try_from(data_dir.trim())?;

        let db_dir = config.get_db_dir();
        if !std::fs::exists(db_dir.as_str())? {
            std::fs::create_dir_all(db_dir.as_str())?;
        }
        let conn_str = format!("sqlite:{}/data.db?mode=rwc", db_dir);
        let db = Database::connect(conn_str.as_str()).await?;

        Ok(AppState {
            _db: db,
            pan123: Arc::new(pan123::Client::new(
                &config.get_pan123_config().client_id,
                &config.get_pan123_config().client_secret,
                &format!("{}/pan123", config.get_cache_dir()),
            )),
            pan189: Arc::new(pan189::Client::new()),
            tmdb: Arc::new(tmdb::Client::new(&config.get_tmdb_config().api_key)),
            config: Arc::new(config),
        })
    }
}
