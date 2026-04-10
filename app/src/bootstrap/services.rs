use crate::{
    application::{
        import_media::ImportMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, resolve_download_url::ResolveDownloadUrlService,
        sync_strm::SyncStrmService,
    },
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
