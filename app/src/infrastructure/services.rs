use crate::{
    application::{
        delete_media::DeleteMediaService, file_index::FileIndexService, import::TransferWorkflow,
        manage_keywords::ManageKeywordsService, notify::PublishTelegramMessageService,
        resolve_download_url::ResolveDownloadUrlService, sync_strm::SyncStrmService,
    },
    infrastructure::{
        cache::string_store::StringCacheStore,
        client::{pan115, pan123, pan189, quark},
        client::library_remote::Pan123LibraryRemote,
        event::publisher::EventBusPublisher,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{Pan123MediaSearchGateway, PanLibraryGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::{file_index::SeaOrmFileIndexRepository, keyword::SeaOrmKeywordRepository},
        share::{
            resolver::ShareResolverService,
        },
    },
};

pub type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
pub type ShareResolverRuntimeService = ShareResolverService<
    pan123::Client,
    pan189::Client,
    pan115::Client,
    quark::Client,
>;
pub type ImportService =
    TransferWorkflow<PanLibraryGateway, TmdbMetadataGateway, FilesystemImportLocalStore>;
pub type NotifyService = PublishTelegramMessageService<EventBusPublisher>;
pub type SyncService = SyncStrmService<Pan123LibraryRemote, TokioFileStore>;
pub type MediaDownloadUrlService = ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>;
pub type DeleteMediaServiceRuntime =
    DeleteMediaService<Pan123MediaSearchGateway, PanLibraryGateway, FilesystemImportLocalStore>;
pub type FileIndexRuntimeService = FileIndexService<SeaOrmFileIndexRepository>;
