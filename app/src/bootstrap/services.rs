pub use crate::infrastructure::services::*;

use crate::{
    application::{import::MetadataLookup, share_crawler::ShareCrawler},
    bootstrap::app::Client,
    config,
    infrastructure::import::{
        gateway::{PanLibraryGateway, ShareImportGateway, TmdbMetadataGateway},
        local_store::FilesystemImportLocalStore,
    },
};

pub(crate) fn build_share_crawler(config: &config::Manager) -> ShareCrawler<ShareSourceService> {
    let clients = Client::new(config);
    ShareCrawler::new(ShareImportGateway::new(
        clients.pan115.clone(),
        clients.pan123.clone(),
        clients.pan189.clone(),
        clients.quark.clone(),
    ))
}

pub(crate) fn build_import_service(config: &config::Manager) -> (ImportService, MetadataLookup) {
    let clients = Client::new(config);
    build_import_service_from_clients(
        &clients,
        config.get_library_config().remote_path.clone(),
        config.get_library_config().local_path.clone(),
        config.get_media_server_config().get_strm_download_url(),
    )
}

pub(crate) fn build_import_service_from_clients(
    clients: &Client,
    remote_path: String,
    local_path: String,
    strm_download_url: String,
) -> (ImportService, MetadataLookup) {
    (
        ImportService::new(
            PanLibraryGateway::new(clients.pan123.clone()),
            TmdbMetadataGateway::new(clients.tmdb.clone()),
            FilesystemImportLocalStore::new(remote_path, local_path, strm_download_url),
        ),
        MetadataLookup::default(),
    )
}

pub(crate) fn build_file_index_service(db: sea_orm::DatabaseConnection) -> FileIndexRuntimeService {
    crate::application::file_index::FileIndexService::new(
        crate::infrastructure::repo::file_index::SeaOrmFileIndexRepository::new(db),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::build_import_service;
    use crate::config;

    fn unique_temp_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("bigbrother-build-import-service-{suffix}"))
    }

    #[test]
    fn build_import_service_from_config_without_server_runtime() {
        let data_dir = unique_temp_dir();
        let config_dir = data_dir.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.yaml"),
            r#"
pan123:
  passport: test-user
  password: test-pass
quark:
  cookie: test-cookie
tmdb:
  api_key: test-tmdb-key
library:
  remote_path: /remote/library
  local_path: /local/library
media_server:
  advertise_base_url: http://localhost:3100
  strm_path_prefix: /d
"#,
        )
        .unwrap();

        let config = config::Manager::try_from(data_dir.to_str().unwrap()).unwrap();
        let _ = build_import_service(&config);

        fs::remove_dir_all(data_dir).unwrap();
    }
}
