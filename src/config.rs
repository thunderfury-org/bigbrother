use std::sync::Arc;

use serde::Deserialize;

use super::error::AppError;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
struct AppConfig {
    pub pan123: Pan123Config,
    pub tmdb: TmdbConfig,
    pub telegram: TelegramConfig,
    pub library: LibraryConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Pan123Config {
    pub client_id: String,
    pub client_secret: String,
    pub file_id: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LibraryConfig {
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct TelegramConfig {
    pub bot_token: String,
    pub user_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct TmdbConfig {
    pub api_key: String,
}

#[derive(Clone)]
pub struct Manager {
    data_dir: Arc<String>,
    app_config: Arc<AppConfig>,
}

impl Manager {
    pub fn get_log_dir(&self) -> String {
        format!("{}/log", self.data_dir.as_str())
    }

    pub fn get_cache_dir(&self) -> String {
        format!("{}/cache", self.data_dir.as_str())
    }

    pub fn get_db_dir(&self) -> String {
        format!("{}/db", self.data_dir.as_str())
    }

    pub fn get_pan123_config(&self) -> &Pan123Config {
        &self.app_config.pan123
    }

    pub fn get_tmdb_config(&self) -> &TmdbConfig {
        &self.app_config.tmdb
    }

    pub fn get_telegram_config(&self) -> &TelegramConfig {
        &self.app_config.telegram
    }

    pub fn get_library_config(&self) -> &LibraryConfig {
        &self.app_config.library
    }
}

impl TryFrom<&str> for Manager {
    type Error = AppError;

    fn try_from(data_dir: &str) -> Result<Self, Self::Error> {
        if data_dir.is_empty() {
            return Err(AppError::Error("config dir is empty".to_string()));
        }

        let config_file = format!("{data_dir}/config/config.yaml");
        if !std::fs::exists(config_file.as_str())? {
            return Ok(Self {
                data_dir: Arc::new(data_dir.to_string()),
                app_config: Arc::new(AppConfig::default()),
            });
        }

        match serde_yaml::from_str(std::fs::read_to_string(config_file.as_str())?.as_str()) {
            Ok(config) => {
                return Ok(Self {
                    data_dir: Arc::new(data_dir.to_string()),
                    app_config: Arc::new(config),
                });
            }
            Err(e) => Err(AppError::Error(format!("parse config file error, {}", e))),
        }
    }
}
