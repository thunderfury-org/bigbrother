use sea_orm::DatabaseConnection;
use std::time::Duration;
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
            keyword::SeaOrmKeywordRepository, subscription::SeaOrmSubscriptionRepository,
            telegram_export_state::SeaOrmTelegramExportStateRepository,
        },
        services::{
            FileIndexRuntimeService, IdentifyService, ImportService, ParseRuntimeService,
            ShareResolverRuntimeService, SubscriptionService,
        },
        share::{
            pan115::Pan115ShareService, pan123::Pan123ShareService, pan189::Pan189ShareService,
        },
        title_extractor::TitleExtractorService,
    },
};

use super::{config, connect_db};

pub(super) struct CliContext {
    config: config::Manager,
    db: OnceCell<DatabaseConnection>,
    pan115: client::pan115::Client,
    pan123: client::pan123::Client,
    pan189: client::pan189::Client,
    tmdb: client::tmdb::Client,
}

impl CliContext {
    pub(super) fn new(data_dir: &str) -> AppResult<Self> {
        let config = config::Manager::try_from(data_dir.trim())?;
        let pan115 = client::pan115::Client::with_request_interval(Duration::from_millis(
            config.get_pan115_config().get_request_interval_ms(),
        ));
        let pan123 = client::pan123::Client::new(
            &config.get_pan123_config().api_address,
            &config.get_pan123_config().refresh_token,
            &format!("{}/pan123", config.get_cache_dir()),
        );
        let pan189 = client::pan189::Client::new(client::pan189::AuthConfig {
            username: config.get_pan189_config().username.clone(),
            password: config.get_pan189_config().password.clone(),
            cache_dir: format!("{}/pan189", config.get_cache_dir()),
        });
        let tmdb = client::tmdb::Client::new(&config.get_tmdb_config().api_key);

        Ok(Self {
            config,
            db: OnceCell::new(),
            pan115,
            pan123,
            pan189,
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

    pub(super) fn tmdb(&self) -> client::tmdb::Client {
        self.tmdb.clone()
    }

    pub(super) fn share_resolver(&self) -> ShareResolverRuntimeService {
        ShareResolverRuntimeService::new(
            Pan123ShareService::new(self.pan123()),
            Pan189ShareService::new(self.pan189()),
            Pan115ShareService::new(self.pan115()),
        )
    }

    pub(super) async fn import_service(&self) -> AppResult<ImportService> {
        Ok(ImportService::new(
            PanLibraryGateway::new(self.pan123()),
            FilesystemImportLocalStore::new(
                self.config.get_library_config().remote_path.clone(),
                self.config.get_library_config().local_path.clone(),
                self.config
                    .get_media_server_config()
                    .get_strm_download_url(),
            ),
        ))
    }

    pub(super) async fn identify_service(&self) -> AppResult<IdentifyService> {
        let openai_config = self.config.get_openai_config();
        let openai_client = if openai_config.is_configured() {
            Some(client::openai::Client::new(
                &openai_config.api_key,
                &openai_config.base_url,
                &openai_config.model,
            ))
        } else {
            None
        };
        let db = self.db().await?.clone();
        let title_extractor = TitleExtractorService::new(openai_client, db);

        Ok(IdentifyService::new(
            TmdbMetadataGateway::new(self.tmdb()),
            title_extractor,
        ))
    }

    pub(super) async fn parse_service(&self) -> AppResult<ParseRuntimeService> {
        let openai_config = self.config.get_openai_config();
        let openai_client = if openai_config.is_configured() {
            Some(client::openai::Client::new(
                &openai_config.api_key,
                &openai_config.base_url,
                &openai_config.model,
            ))
        } else {
            None
        };
        let db = self.db().await?.clone();
        let title_extractor = TitleExtractorService::new(openai_client, db);

        Ok(ParseRuntimeService::new(
            TmdbMetadataGateway::new(self.tmdb()),
            title_extractor,
        ))
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

    pub(super) async fn subscription_service(
        &self,
    ) -> AppResult<(SubscriptionService, SeaOrmSubscriptionRepository)> {
        let db = self.db().await?.clone();
        let repo = SeaOrmSubscriptionRepository::new(db);
        let service = SubscriptionService::new(repo.clone(), TmdbMetadataGateway::new(self.tmdb()));
        Ok((service, repo))
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
