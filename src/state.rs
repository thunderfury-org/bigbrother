use std::sync::Arc;

use crate::{
    client::{pan123, pan189, tmdb},
    config,
    error::AppError,
};

#[derive(Clone, Default)]
pub struct AppState {
    pub config: config::Manager,
    pub pan123: Arc<pan123::Client>,
    pub pan189: Arc<pan189::Client>,
    pub tmdb: Arc<tmdb::Client>,
}

impl TryFrom<&str> for AppState {
    type Error = AppError;

    fn try_from(data_dir: &str) -> Result<Self, Self::Error> {
        let config = config::Manager::try_from(data_dir.trim())?;
        Ok(AppState {
            pan123: Arc::new(pan123::Client::new(
                &config.get_pan123_config().client_id,
                &config.get_pan123_config().client_secret,
                &format!("{}/pan123", config.get_cache_dir()),
            )),
            pan189: Arc::new(pan189::Client::new()),
            tmdb: Arc::new(tmdb::Client::new(&config.get_tmdb_config().api_key)),
            config,
        })
    }
}
