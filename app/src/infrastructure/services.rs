use crate::{
    application::{
        delete_media::DeleteMediaService, file_index::FileIndexService, import::TransferWorkflow,
        manage_keywords::ManageKeywordsService, notify::PublishTelegramMessageService,
        resolve_download_url::ResolveDownloadUrlService, sync_strm::SyncStrmService,
    },
    infrastructure::{
        cache::string_store::StringCacheStore,
        client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{Pan123MediaSearchGateway, PanLibraryGateway, ShareClientGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::{file_index::SeaOrmFileIndexRepository, keyword::SeaOrmKeywordRepository},
        share::resolver::ShareResolverService,
    },
};

pub type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
#[allow(dead_code)]
pub type ShareResolverRuntimeService = ShareResolverService<ShareClientGateway>;
pub type ShareSourceService = ShareClientGateway;
pub type ImportService =
    TransferWorkflow<PanLibraryGateway, TmdbMetadataGateway, FilesystemImportLocalStore>;
pub type NotifyService = PublishTelegramMessageService<EventBusPublisher>;
pub type SyncService = SyncStrmService<Pan123LibraryRemote, TokioFileStore>;
pub type MediaDownloadUrlService = ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>;
pub type DeleteMediaServiceRuntime =
    DeleteMediaService<Pan123MediaSearchGateway, PanLibraryGateway, FilesystemImportLocalStore>;
pub type FileIndexRuntimeService = FileIndexService<SeaOrmFileIndexRepository>;
