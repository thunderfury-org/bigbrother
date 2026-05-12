use serde::Deserialize;

use crate::error::AppError;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
struct AppConfig {
    pub media_server: MediaServerConfig,
    pub emby_proxy: EmbyProxyConfig,
    pub pan123: Pan123Config,
    pub pan189: Pan189Config,
    pub tmdb: TmdbConfig,
    pub telegram: TelegramConfig,
    pub library: LibraryConfig,
    pub quark: QuarkConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct MediaServerConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub advertise_base_url: Option<String>,
    pub strm_path_prefix: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct EmbyProxyConfig {
    pub enable: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub upstream_base_url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Pan123Config {
    pub passport: String,
    pub password: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Pan189Config {
    pub username: String,
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

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct QuarkConfig {
    pub cookie: String,
}

#[derive(Default)]
pub(crate) struct Manager {
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

    pub fn get_emby_proxy_config(&self) -> &EmbyProxyConfig {
        &self.app_config.emby_proxy
    }

    pub fn get_pan123_config(&self) -> &Pan123Config {
        &self.app_config.pan123
    }

    pub fn get_pan189_config(&self) -> &Pan189Config {
        &self.app_config.pan189
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

    pub fn get_quark_config(&self) -> &QuarkConfig {
        &self.app_config.quark
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

impl EmbyProxyConfig {
    pub fn is_enabled(&self) -> bool {
        self.enable
    }

    fn get_host(&self) -> &str {
        self.host.as_deref().unwrap_or("0.0.0.0")
    }

    fn get_port(&self) -> u16 {
        self.port.unwrap_or(8097)
    }

    pub fn get_addr(&self) -> String {
        format!("{}:{}", self.get_host(), self.get_port())
    }

    pub fn get_upstream_base_url(&self) -> Option<String> {
        self.upstream_base_url
            .as_ref()
            .map(|url| url.trim_end_matches('/').to_owned())
            .filter(|url| !url.is_empty())
    }

    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempConfigDir {
        path: PathBuf,
    }

    impl TempConfigDir {
        fn new() -> Self {
            let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bigbrother-config-{}-{counter}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("config")).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_config(&self, config: &str) {
            fs::write(self.path.join("config/config.yaml"), config).unwrap();
        }
    }

    impl Drop for TempConfigDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn emby_proxy_defaults_to_disabled() {
        let data_dir = TempConfigDir::new();
        data_dir.write_config("");

        let config = Manager::try_from(data_dir.path().to_str().unwrap()).unwrap();

        assert!(!config.get_emby_proxy_config().is_enabled());
        assert_eq!(config.get_emby_proxy_config().get_addr(), "0.0.0.0:8097");
    }

    #[test]
    fn emby_proxy_empty_section_uses_defaults() {
        let data_dir = TempConfigDir::new();
        data_dir.write_config(
            r#"
emby_proxy: {}
"#,
        );

        let config = Manager::try_from(data_dir.path().to_str().unwrap()).unwrap();
        let emby_proxy = config.get_emby_proxy_config();

        assert!(!emby_proxy.is_enabled());
        assert_eq!(emby_proxy.get_addr(), "0.0.0.0:8097");
    }

    #[test]
    fn emby_proxy_parses_enabled_config() {
        let data_dir = TempConfigDir::new();
        data_dir.write_config(
            r#"
emby_proxy:
  enable: true
  host: 127.0.0.1
  port: 18097
  upstream_base_url: http://emby.example:8096/
  api_key: secret
"#,
        );

        let config = Manager::try_from(data_dir.path().to_str().unwrap()).unwrap();
        let emby_proxy = config.get_emby_proxy_config();

        assert!(emby_proxy.is_enabled());
        assert_eq!(emby_proxy.get_addr(), "127.0.0.1:18097");
        assert_eq!(
            emby_proxy.get_upstream_base_url().unwrap(),
            "http://emby.example:8096"
        );
        assert_eq!(emby_proxy.get_api_key(), Some("secret"));
    }
}
