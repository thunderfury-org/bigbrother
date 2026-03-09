use serde::Deserialize;

use super::error::AppError;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
struct AppConfig {
    pub media_server: MediaServerConfig,
    pub pan123: Pan123Config,
    pub tmdb: TmdbConfig,
    pub telegram: TelegramConfig,
    pub library: LibraryConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct MediaServerConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub advertise_base_url: Option<String>,
    pub strm_path_prefix: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Pan123Config {
    pub passport: String,
    pub password: String,
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
    pub user_id: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct TmdbConfig {
    pub api_key: String,
}

#[derive(Default)]
pub struct Manager {
    data_dir: String,
    app_config: Box<AppConfig>,
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

    pub fn get_media_server_config(&self) -> &MediaServerConfig {
        &self.app_config.media_server
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
            return Err(AppError::InvalidParameter(
                "config dir is empty".to_string(),
            ));
        }

        let config_file = format!("{data_dir}/config/config.yaml");
        if !std::fs::exists(config_file.as_str())? {
            return Ok(Self {
                data_dir: data_dir.to_owned(),
                app_config: Box::new(AppConfig::default()),
            });
        }

        match serde_yaml::from_str(std::fs::read_to_string(config_file.as_str())?.as_str()) {
            Ok(config) => Ok(Self {
                data_dir: data_dir.to_owned(),
                app_config: Box::new(config),
            }),
            Err(e) => Err(AppError::InvalidParameter(format!(
                "parse config file error, {}",
                e
            ))),
        }
    }
}

impl MediaServerConfig {
    #[inline]
    fn get_host(&self) -> &str {
        self.host.as_deref().unwrap_or("0.0.0.0")
    }

    #[inline]
    fn get_port(&self) -> u16 {
        self.port.unwrap_or(3100)
    }

    pub fn get_addr(&self) -> String {
        format!("{}:{}", self.get_host(), self.get_port())
    }

    pub fn get_advertise_base_url(&self) -> String {
        self.advertise_base_url.as_ref().map_or_else(
            || format!("http://{}", self.get_addr()),
            |u| u.trim_end_matches('/').to_owned(),
        )
    }

    pub fn get_strm_path_prefix(&self) -> &str {
        self.strm_path_prefix.as_deref().unwrap_or("/d")
    }

    pub fn get_strm_download_url(&self) -> String {
        format!(
            "{}{}",
            self.get_advertise_base_url(),
            self.get_strm_path_prefix()
        )
    }
}
