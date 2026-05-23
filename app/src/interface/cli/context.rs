use sea_orm::DatabaseConnection;
use tokio::sync::OnceCell;

use crate::{
    application::manage_keywords::ManageKeywordsService,
    error::AppResult,
    infrastructure::{
        client,
        import::{
            gateway::{PanLibraryGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::{
            file_index::SeaOrmFileIndexRepository, import_record::SeaOrmImportRecordRepository,
            keyword::SeaOrmKeywordRepository,
            telegram_export_state::SeaOrmTelegramExportStateRepository,
        },
        services::{FileIndexRuntimeService, ImportService, ShareResolverRuntimeService},
        share::{
            pan115::Pan115ShareService, pan123::Pan123ShareService, pan189::Pan189ShareService,
            quark::QuarkShareService,
        },
    },
};

use super::{config, connect_db};

pub(super) struct CliContext {
    config: config::Manager,
    db: OnceCell<DatabaseConnection>,
    pan115: client::pan115::Client,
    pan123: client::pan123::Client,
    pan189: client::pan189::Client,
    quark: client::quark::Client,
    tmdb: client::tmdb::Client,
}

impl CliContext {
    pub(super) fn new(data_dir: &str) -> AppResult<Self> {
        let config = config::Manager::try_from(data_dir.trim())?;
        let pan115 = client::pan115::Client::new();
        let pan123 = client::pan123::Client::new(
            &config.get_pan123_config().passport,
            &config.get_pan123_config().password,
            &format!("{}/pan123", config.get_cache_dir()),
        );
        let pan189 = client::pan189::Client::new(client::pan189::AuthConfig {
            username: config.get_pan189_config().username.clone(),
            password: config.get_pan189_config().password.clone(),
            cache_dir: format!("{}/pan189", config.get_cache_dir()),
        });
        let quark = client::quark::Client::new(&config.get_quark_config().cookie);
        let tmdb = client::tmdb::Client::new(&config.get_tmdb_config().api_key);

        Ok(Self {
            config,
            db: OnceCell::new(),
            pan115,
            pan123,
            pan189,
            quark,
            tmdb,
        })
    }

    pub(super) fn config(&self) -> &config::Manager {
        &self.config
    }

    pub(super) async fn db(&self) -> AppResult<&DatabaseConnection> {
        self.db
            .get_or_try_init(|| async { connect_db(&self.config.get_db_dir()).await })
            .await
    }

    pub(super) fn pan115(&self) -> client::pan115::Client {
        self.pan115.clone()
    }

    pub(super) fn pan123(&self) -> client::pan123::Client {
        self.pan123.clone()
    }

    pub(super) fn pan189(&self) -> client::pan189::Client {
        self.pan189.clone()
    }

    pub(super) fn quark(&self) -> client::quark::Client {
        self.quark.clone()
    }

    pub(super) fn tmdb(&self) -> client::tmdb::Client {
        self.tmdb.clone()
    }

    pub(super) fn share_resolver(&self) -> ShareResolverRuntimeService {
        ShareResolverRuntimeService::new(
            Pan123ShareService::new(self.pan123()),
            Pan189ShareService::new(self.pan189()),
            Pan115ShareService::new(self.pan115()),
            QuarkShareService::new(self.quark()),
        )
    }

    pub(super) fn import_service(&self) -> ImportService {
        ImportService::new(
            PanLibraryGateway::new(self.pan123()),
            TmdbMetadataGateway::new(self.tmdb()),
            FilesystemImportLocalStore::new(
                self.config.get_library_config().remote_path.clone(),
                self.config.get_library_config().local_path.clone(),
                self.config
                    .get_media_server_config()
                    .get_strm_download_url(),
            ),
        )
    }

    pub(super) async fn file_index_service(&self) -> AppResult<FileIndexRuntimeService> {
        let db = self.db().await?.clone();
        Ok(FileIndexRuntimeService::new(
            SeaOrmFileIndexRepository::new(db),
        ))
    }

    pub(super) async fn keyword_service(
        &self,
    ) -> AppResult<ManageKeywordsService<SeaOrmKeywordRepository>> {
        let db = self.db().await?.clone();
        Ok(ManageKeywordsService::new(SeaOrmKeywordRepository::new(db)))
    }

    pub(super) async fn telegram_export_index_services(
        &self,
    ) -> AppResult<(
        ShareResolverRuntimeService,
        SeaOrmFileIndexRepository,
        SeaOrmTelegramExportStateRepository,
    )> {
        let db = self.db().await?.clone();
        Ok((
            self.share_resolver(),
            SeaOrmFileIndexRepository::new(db.clone()),
            SeaOrmTelegramExportStateRepository::new(db),
        ))
    }

    pub(super) async fn import_record_repository(&self) -> AppResult<SeaOrmImportRecordRepository> {
        let db = self.db().await?.clone();
        Ok(SeaOrmImportRecordRepository::new(db))
    }
}
