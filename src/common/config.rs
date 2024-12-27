use std::sync::Arc;

use serde::Deserialize;

use crate::common::error::Error;

#[derive(Debug, Default, Deserialize)]
pub struct AppConfig {
    pub alist_host: String,
    pub alist_api_token: String,

    pub tmdb_api_key: String,

    #[serde(default)]
    pub telegram_bot_token: String,
    #[serde(default)]
    pub telegram_chat_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct TaskConfig {
    pub src_dir: String,
    pub dest_dir: String,
}

#[derive(Clone)]
pub struct Manager {
    data_dir: Arc<String>,
    app_config: Arc<AppConfig>,
    tasks: Arc<Vec<TaskConfig>>,
}

impl Manager {
    pub fn get_data_dir(&self) -> &str {
        self.data_dir.as_str()
    }

    pub fn get_app_config(&self) -> &AppConfig {
        self.app_config.as_ref()
    }

    pub fn get_tasks(&self) -> &Vec<TaskConfig> {
        self.tasks.as_ref()
    }
}

fn read_string(file: &str) -> Result<String, Error> {
    match std::fs::read_to_string(file) {
        Ok(s) => Ok(s),
        Err(e) => Err(Error::Internal(format!("read file {} error, {}", file, e))),
    }
}

impl TryFrom<&str> for Manager {
    type Error = Error;

    fn try_from(data_dir: &str) -> Result<Self, Self::Error> {
        if data_dir.is_empty() {
            return Err(Error::Internal("config dir is empty".to_string()));
        }

        let config_file = format!("{data_dir}/config.yaml");

        match serde_yaml::from_str(read_string(config_file.as_str())?.as_str()) {
            Ok(config) => {
                let m = Self {
                    data_dir: Arc::new(data_dir.to_string()),
                    app_config: Arc::new(config),
                    tasks: Arc::new(vec![]),
                };

                let task_file = format!("{data_dir}/tasks.yaml");
                if !std::fs::exists(task_file.as_str())? {
                    return Ok(m);
                }

                match serde_yaml::from_str(read_string(task_file.as_str())?.as_str()) {
                    Ok(tasks) => Ok(Self {
                        tasks: Arc::new(tasks),
                        ..m
                    }),
                    Err(e) => Err(Error::Internal(format!("parse tasks file error, {}", e))),
                }
            }
            Err(e) => Err(Error::Internal(format!("parse config file error, {}", e))),
        }
    }
}

impl From<AppConfig> for Manager {
    // used for test

    fn from(value: AppConfig) -> Self {
        Self {
            data_dir: Arc::new("".to_string()),
            app_config: Arc::new(value),
            tasks: Arc::new(vec![]),
        }
    }
}
