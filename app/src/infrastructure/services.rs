use crate::{
    application::{
        delete_media::DeleteMediaService,
        file_index::FileIndexService,
        import::{ParseService, TransferWorkflow, identify::MediaIdentifyService},
        manage_keywords::ManageKeywordsService,
        resolve_download_url::ResolveDownloadUrlService,
        sync_strm::SyncStrmService,
    },
    infrastructure::{
        cache::string_store::StringCacheStore,
        client::library_remote::Pan123LibraryRemote,
        client::{pan115, pan123, pan189},
        event::publisher::EventBusPublisher,
        fs::tokio_file_store::TokioFileStore,
        import::{
            gateway::{Pan123MediaSearchGateway, PanLibraryGateway, TmdbMetadataGateway},
            local_store::FilesystemImportLocalStore,
        },
        repo::{file_index::SeaOrmFileIndexRepository, keyword::SeaOrmKeywordRepository},
        share::resolver::ShareResolverService,
        title_extractor::TitleExtractorService,
    },
};

pub type KeywordService = ManageKeywordsService<SeaOrmKeywordRepository>;
pub type ShareResolverRuntimeService =
    ShareResolverService<pan123::Client, pan189::Client, pan115::Client>;
pub type ImportService = TransferWorkflow<PanLibraryGateway, FilesystemImportLocalStore>;
pub type IdentifyService = MediaIdentifyService<TmdbMetadataGateway, TitleExtractorService>;
pub type NotifyService = EventBusPublisher;
pub type SyncService = SyncStrmService<Pan123LibraryRemote, TokioFileStore>;
pub type MediaDownloadUrlService = ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>;
pub type DeleteMediaServiceRuntime =
    DeleteMediaService<Pan123MediaSearchGateway, PanLibraryGateway, FilesystemImportLocalStore>;
pub type FileIndexRuntimeService = FileIndexService<SeaOrmFileIndexRepository>;
pub type ParseRuntimeService = ParseService<TmdbMetadataGateway, TitleExtractorService>;
