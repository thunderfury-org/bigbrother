use crate::{
    application::{
        import_media::ImportMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, resolve_download_url::ResolveDownloadUrlService,
        sync_strm::SyncStrmService,
    },
    bootstrap::app::Client,
    config,
    infrastructure::{
        cache::string_store::StringCacheStore,
        client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{PanLibraryGateway, ShareImportGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::keyword::SeaOrmKeywordRepository,
    },
};

pub(crate) type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
pub(crate) type ImportService = ImportMediaService<
    PanLibraryGateway,
    ShareImportGateway,
    TmdbMetadataGateway,
    FilesystemImportLocalStore,
>;
pub(crate) type NotifyService = PublishTelegramMessageService<EventBusPublisher>;
pub(crate) type SyncService = SyncStrmService<Pan123LibraryRemote, TokioFileStore>;
pub(crate) type MediaDownloadUrlService =
    ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>;

pub(crate) fn build_import_service(config: &config::Manager) -> ImportService {
    let clients = Client::new(config);

    ImportMediaService::new(
        PanLibraryGateway::new(clients.pan123.clone()),
        ShareImportGateway::new(clients.pan115, clients.pan123, clients.pan189),
        TmdbMetadataGateway::new(clients.tmdb),
        FilesystemImportLocalStore::new(
            config.get_library_config().remote_path.clone(),
            config.get_library_config().local_path.clone(),
            config.get_media_server_config().get_strm_download_url(),
        ),
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
        let _service = build_import_service(&config);

        fs::remove_dir_all(data_dir).unwrap();
    }
}
