#![allow(dead_code)]

//! Object-safe twins of application ports.
//!
//! Adapters keep implementing the original RPITIT traits. Application services
//! store `Arc<dyn Dyn...>` so use-case types stay concrete.

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::future::BoxFuture;

use crate::{
    domain::{
        import::{LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
        media::Title,
        subscription::SubscriptionMediaType,
    },
    error::AppResult,
};

use super::{
    DownloadUrlCache, DownloadUrlResult, DownloadUrlSource, FileIndexRecordInput,
    FileIndexRepository, FileStore, ImportLocalStore, ImportRecordCreate, ImportRecordFilter,
    ImportRecordFinalize, ImportRecordPage, ImportRecordPaging, ImportRecordRepository,
    ImportRecordView, LibraryGateway, LibraryRemote, LocalEntry, MediaDirectoryRecord,
    MediaSearchSource, MetadataCatalog, RemoteEntry, ShareResolver, SubscriptionCreateInput,
    SubscriptionRecord, SubscriptionRepository, TelegramExportStateRecord,
    TelegramExportStateRepository, TitleExtractor,
};

pub type FileIndexRepo = Arc<dyn DynFileIndexRepository>;
pub type ImportRecordRepo = Arc<dyn DynImportRecordRepository>;
pub type SubscriptionRepo = Arc<dyn DynSubscriptionRepository>;
pub type LibraryGatewayHandle = Arc<dyn DynLibraryGateway>;
pub type MetadataCatalogHandle = Arc<dyn DynMetadataCatalog>;
pub type TitleExtractorHandle = Arc<dyn DynTitleExtractor>;
pub type ImportLocalStoreHandle = Arc<dyn DynImportLocalStore>;
pub type MediaSearchHandle = Arc<dyn DynMediaSearchSource>;
pub type DownloadUrlCacheHandle = Arc<dyn DynDownloadUrlCache>;
pub type DownloadUrlSourceHandle = Arc<dyn DynDownloadUrlSource>;
pub type LibraryRemoteHandle = Arc<dyn DynLibraryRemote>;
pub type FileStoreHandle = Arc<dyn DynFileStore>;
pub type ShareResolverHandle = Arc<dyn DynShareResolver>;
pub type TelegramExportStateRepo = Arc<dyn DynTelegramExportStateRepository>;

pub trait DynFileIndexRepository: Send + Sync {
    fn record_files<'a>(
        &'a self,
        files: &'a [FileIndexRecordInput],
    ) -> BoxFuture<'a, AppResult<()>>;
    fn search_files<'a>(
        &'a self,
        keyword: &'a str,
        limit: u64,
    ) -> BoxFuture<'a, AppResult<Vec<super::FileSearchRecord>>>;
    fn get_records_by_ids<'a>(
        &'a self,
        ids: &'a [i64],
    ) -> BoxFuture<'a, AppResult<Vec<super::FileSearchRecord>>>;
}

impl<T: FileIndexRepository + Send + Sync> DynFileIndexRepository for T {
    fn record_files<'a>(
        &'a self,
        files: &'a [FileIndexRecordInput],
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(FileIndexRepository::record_files(self, files))
    }

    fn search_files<'a>(
        &'a self,
        keyword: &'a str,
        limit: u64,
    ) -> BoxFuture<'a, AppResult<Vec<super::FileSearchRecord>>> {
        Box::pin(FileIndexRepository::search_files(self, keyword, limit))
    }

    fn get_records_by_ids<'a>(
        &'a self,
        ids: &'a [i64],
    ) -> BoxFuture<'a, AppResult<Vec<super::FileSearchRecord>>> {
        Box::pin(FileIndexRepository::get_records_by_ids(self, ids))
    }
}

pub trait DynImportRecordRepository: Send + Sync {
    fn create<'a>(&'a self, input: &'a ImportRecordCreate) -> BoxFuture<'a, AppResult<i64>>;
    fn finalize<'a>(
        &'a self,
        id: i64,
        update: &'a ImportRecordFinalize,
    ) -> BoxFuture<'a, AppResult<()>>;
    fn get(&self, id: i64) -> BoxFuture<'_, AppResult<Option<ImportRecordView>>>;
    fn list<'a>(
        &'a self,
        filter: &'a ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> BoxFuture<'a, AppResult<ImportRecordPage>>;
}

impl<T: ImportRecordRepository + Send + Sync> DynImportRecordRepository for T {
    fn create<'a>(&'a self, input: &'a ImportRecordCreate) -> BoxFuture<'a, AppResult<i64>> {
        Box::pin(ImportRecordRepository::create(self, input))
    }

    fn finalize<'a>(
        &'a self,
        id: i64,
        update: &'a ImportRecordFinalize,
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(ImportRecordRepository::finalize(self, id, update))
    }

    fn get(&self, id: i64) -> BoxFuture<'_, AppResult<Option<ImportRecordView>>> {
        Box::pin(ImportRecordRepository::get(self, id))
    }

    fn list<'a>(
        &'a self,
        filter: &'a ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> BoxFuture<'a, AppResult<ImportRecordPage>> {
        Box::pin(ImportRecordRepository::list(self, filter, paging))
    }
}

pub trait DynSubscriptionRepository: Send + Sync {
    fn list_all(&self) -> BoxFuture<'_, AppResult<Vec<SubscriptionRecord>>>;
    fn get_by_id(&self, id: i64) -> BoxFuture<'_, AppResult<Option<SubscriptionRecord>>>;
    fn find_by_tmdb_id<'a>(
        &'a self,
        tmdb_id: u32,
        media_type: &'a SubscriptionMediaType,
    ) -> BoxFuture<'a, AppResult<Option<SubscriptionRecord>>>;
    fn create<'a>(&'a self, input: &'a SubscriptionCreateInput) -> BoxFuture<'a, AppResult<i64>>;
    fn delete(&self, id: i64) -> BoxFuture<'_, AppResult<()>>;
}

impl<T: SubscriptionRepository + Send + Sync> DynSubscriptionRepository for T {
    fn list_all(&self) -> BoxFuture<'_, AppResult<Vec<SubscriptionRecord>>> {
        Box::pin(SubscriptionRepository::list_all(self))
    }

    fn get_by_id(&self, id: i64) -> BoxFuture<'_, AppResult<Option<SubscriptionRecord>>> {
        Box::pin(SubscriptionRepository::get_by_id(self, id))
    }

    fn find_by_tmdb_id<'a>(
        &'a self,
        tmdb_id: u32,
        media_type: &'a SubscriptionMediaType,
    ) -> BoxFuture<'a, AppResult<Option<SubscriptionRecord>>> {
        Box::pin(SubscriptionRepository::find_by_tmdb_id(
            self, tmdb_id, media_type,
        ))
    }

    fn create<'a>(&'a self, input: &'a SubscriptionCreateInput) -> BoxFuture<'a, AppResult<i64>> {
        Box::pin(SubscriptionRepository::create(self, input))
    }

    fn delete(&self, id: i64) -> BoxFuture<'_, AppResult<()>> {
        Box::pin(SubscriptionRepository::delete(self, id))
    }
}

pub trait DynLibraryGateway: Send + Sync {
    fn list_library_files(&self, dir_id: i64) -> BoxFuture<'_, AppResult<Vec<LibraryFile>>>;
    fn get_library_dir_id_by_path<'a>(
        &'a self,
        path: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<i64>>>;
    fn mkdir_library_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<i64>>;
    fn list_library_dir_ids(&self, dir_id: i64) -> BoxFuture<'_, AppResult<HashMap<String, i64>>>;
    fn mkdir_library_dir<'a>(
        &'a self,
        parent_dir_id: i64,
        folder_name: &'a str,
    ) -> BoxFuture<'a, AppResult<i64>>;
    fn trash_library_files<'a>(&'a self, file_ids: &'a [i64]) -> BoxFuture<'a, AppResult<()>>;
    fn fast_upload_md5<'a>(
        &'a self,
        parent_dir_id: i64,
        file_name: &'a str,
        hash: &'a str,
        size: u64,
    ) -> BoxFuture<'a, AppResult<Option<i64>>>;
    fn fast_upload_sha1<'a>(
        &'a self,
        parent_dir_id: i64,
        file_name: &'a str,
        sha1: &'a str,
        size: u64,
    ) -> BoxFuture<'a, AppResult<Option<i64>>>;
    fn download_library_file<'a>(
        &'a self,
        file_id: i64,
        local_path: &'a str,
    ) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: LibraryGateway + Send + Sync> DynLibraryGateway for T {
    fn list_library_files(&self, dir_id: i64) -> BoxFuture<'_, AppResult<Vec<LibraryFile>>> {
        Box::pin(LibraryGateway::list_library_files(self, dir_id))
    }

    fn get_library_dir_id_by_path<'a>(
        &'a self,
        path: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<i64>>> {
        Box::pin(LibraryGateway::get_library_dir_id_by_path(self, path))
    }

    fn mkdir_library_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<i64>> {
        Box::pin(LibraryGateway::mkdir_library_path(self, path))
    }

    fn list_library_dir_ids(&self, dir_id: i64) -> BoxFuture<'_, AppResult<HashMap<String, i64>>> {
        Box::pin(LibraryGateway::list_library_dir_ids(self, dir_id))
    }

    fn mkdir_library_dir<'a>(
        &'a self,
        parent_dir_id: i64,
        folder_name: &'a str,
    ) -> BoxFuture<'a, AppResult<i64>> {
        Box::pin(LibraryGateway::mkdir_library_dir(
            self,
            parent_dir_id,
            folder_name,
        ))
    }

    fn trash_library_files<'a>(&'a self, file_ids: &'a [i64]) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(LibraryGateway::trash_library_files(self, file_ids))
    }

    fn fast_upload_md5<'a>(
        &'a self,
        parent_dir_id: i64,
        file_name: &'a str,
        hash: &'a str,
        size: u64,
    ) -> BoxFuture<'a, AppResult<Option<i64>>> {
        Box::pin(LibraryGateway::fast_upload_md5(
            self,
            parent_dir_id,
            file_name,
            hash,
            size,
        ))
    }

    fn fast_upload_sha1<'a>(
        &'a self,
        parent_dir_id: i64,
        file_name: &'a str,
        sha1: &'a str,
        size: u64,
    ) -> BoxFuture<'a, AppResult<Option<i64>>> {
        Box::pin(LibraryGateway::fast_upload_sha1(
            self,
            parent_dir_id,
            file_name,
            sha1,
            size,
        ))
    }

    fn download_library_file<'a>(
        &'a self,
        file_id: i64,
        local_path: &'a str,
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(LibraryGateway::download_library_file(
            self, file_id, local_path,
        ))
    }
}

pub trait DynMetadataCatalog: Send + Sync {
    fn search_movie<'a>(
        &'a self,
        title: &'a str,
        year: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<SearchMovieResult>>>;
    fn get_movie_detail(&self, id: u32) -> BoxFuture<'_, AppResult<Option<MovieDetail>>>;
    fn search_tv<'a>(
        &'a self,
        title: &'a str,
        year: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<SearchTvResult>>>;
    fn get_tv_detail(&self, id: u32) -> BoxFuture<'_, AppResult<Option<TvDetail>>>;
}

impl<T: MetadataCatalog + Send + Sync> DynMetadataCatalog for T {
    fn search_movie<'a>(
        &'a self,
        title: &'a str,
        year: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<SearchMovieResult>>> {
        Box::pin(MetadataCatalog::search_movie(self, title, year))
    }

    fn get_movie_detail(&self, id: u32) -> BoxFuture<'_, AppResult<Option<MovieDetail>>> {
        Box::pin(MetadataCatalog::get_movie_detail(self, id))
    }

    fn search_tv<'a>(
        &'a self,
        title: &'a str,
        year: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<SearchTvResult>>> {
        Box::pin(MetadataCatalog::search_tv(self, title, year))
    }

    fn get_tv_detail(&self, id: u32) -> BoxFuture<'_, AppResult<Option<TvDetail>>> {
        Box::pin(MetadataCatalog::get_tv_detail(self, id))
    }
}

pub trait DynTitleExtractor: Send + Sync {
    fn extract_title<'a>(&'a self, description: &'a str)
    -> BoxFuture<'a, AppResult<Option<Title>>>;
}

impl<T: TitleExtractor + Send + Sync> DynTitleExtractor for T {
    fn extract_title<'a>(
        &'a self,
        description: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<Title>>> {
        Box::pin(TitleExtractor::extract_title(self, description))
    }
}

pub trait DynImportLocalStore: Send + Sync {
    fn remote_library_path(&self) -> &str;
    fn local_path_for_remote(&self, remote_path: &str) -> String;
    fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String;
    fn write_strm_file<'a>(
        &'a self,
        remote_file_path: &'a str,
        extension: &'a str,
        file_id: i64,
    ) -> BoxFuture<'a, AppResult<()>>;
    fn remove_local_file_if_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>>;
    fn remove_local_dir_if_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: ImportLocalStore + Send + Sync> DynImportLocalStore for T {
    fn remote_library_path(&self) -> &str {
        ImportLocalStore::remote_library_path(self)
    }

    fn local_path_for_remote(&self, remote_path: &str) -> String {
        ImportLocalStore::local_path_for_remote(self, remote_path)
    }

    fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        ImportLocalStore::local_strm_path(self, remote_file_path, extension)
    }

    fn write_strm_file<'a>(
        &'a self,
        remote_file_path: &'a str,
        extension: &'a str,
        file_id: i64,
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(ImportLocalStore::write_strm_file(
            self,
            remote_file_path,
            extension,
            file_id,
        ))
    }

    fn remove_local_file_if_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(ImportLocalStore::remove_local_file_if_exists(self, path))
    }

    fn remove_local_dir_if_exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(ImportLocalStore::remove_local_dir_if_exists(self, path))
    }
}

pub trait DynMediaSearchSource: Send + Sync {
    fn search_media_dirs<'a>(
        &'a self,
        keyword: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<MediaDirectoryRecord>>>;
}

impl<T: MediaSearchSource + Send + Sync> DynMediaSearchSource for T {
    fn search_media_dirs<'a>(
        &'a self,
        keyword: &'a str,
    ) -> BoxFuture<'a, AppResult<Vec<MediaDirectoryRecord>>> {
        Box::pin(MediaSearchSource::search_media_dirs(self, keyword))
    }
}

pub trait DynDownloadUrlCache: Send + Sync {
    fn get_download_url<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Option<String>>>;
    fn set_download_url<'a>(
        &'a self,
        key: &'a str,
        url: &'a str,
        ttl: Duration,
    ) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: DownloadUrlCache + Send + Sync> DynDownloadUrlCache for T {
    fn get_download_url<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Option<String>>> {
        Box::pin(DownloadUrlCache::get_download_url(self, key))
    }

    fn set_download_url<'a>(
        &'a self,
        key: &'a str,
        url: &'a str,
        ttl: Duration,
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(DownloadUrlCache::set_download_url(self, key, url, ttl))
    }
}

pub trait DynDownloadUrlSource: Send + Sync {
    fn get_download_url(&self, file_id: i64) -> BoxFuture<'_, DownloadUrlResult<String>>;
}

impl<T: DownloadUrlSource + Send + Sync> DynDownloadUrlSource for T {
    fn get_download_url(&self, file_id: i64) -> BoxFuture<'_, DownloadUrlResult<String>> {
        Box::pin(DownloadUrlSource::get_download_url(self, file_id))
    }
}

pub trait DynLibraryRemote: Send + Sync {
    fn get_file_id_by_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<Option<i64>>>;
    fn list_dir(&self, dir_id: i64) -> BoxFuture<'_, AppResult<Vec<RemoteEntry>>>;
    fn download_file<'a>(
        &'a self,
        file_id: i64,
        local_path: &'a str,
    ) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: LibraryRemote + Send + Sync> DynLibraryRemote for T {
    fn get_file_id_by_path<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<Option<i64>>> {
        Box::pin(LibraryRemote::get_file_id_by_path(self, path))
    }

    fn list_dir(&self, dir_id: i64) -> BoxFuture<'_, AppResult<Vec<RemoteEntry>>> {
        Box::pin(LibraryRemote::list_dir(self, dir_id))
    }

    fn download_file<'a>(
        &'a self,
        file_id: i64,
        local_path: &'a str,
    ) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(LibraryRemote::download_file(self, file_id, local_path))
    }
}

pub trait DynFileStore: Send + Sync {
    fn read_to_string_if_exists<'a>(
        &'a self,
        path: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<String>>>;
    fn metadata_len_if_exists<'a>(&'a self, path: &'a str)
    -> BoxFuture<'a, AppResult<Option<u64>>>;
    fn ensure_parent_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>>;
    fn write<'a>(&'a self, path: &'a str, content: &'a [u8]) -> BoxFuture<'a, AppResult<()>>;
    fn read_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<Vec<LocalEntry>>>;
    fn remove_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>>;
    fn remove_dir_all<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: FileStore + Send + Sync> DynFileStore for T {
    fn read_to_string_if_exists<'a>(
        &'a self,
        path: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<String>>> {
        Box::pin(FileStore::read_to_string_if_exists(self, path))
    }

    fn metadata_len_if_exists<'a>(
        &'a self,
        path: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<u64>>> {
        Box::pin(FileStore::metadata_len_if_exists(self, path))
    }

    fn ensure_parent_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(FileStore::ensure_parent_dir(self, path))
    }

    fn write<'a>(&'a self, path: &'a str, content: &'a [u8]) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(FileStore::write(self, path, content))
    }

    fn read_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<Vec<LocalEntry>>> {
        Box::pin(FileStore::read_dir(self, path))
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(FileStore::remove_file(self, path))
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(FileStore::remove_dir_all(self, path))
    }
}

pub trait DynShareResolver: Send + Sync {
    fn raw_files_from_url<'a>(
        &'a self,
        url: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<Vec<crate::domain::share::RawFile>>>>;
}

impl<T: ShareResolver + Send + Sync> DynShareResolver for T {
    fn raw_files_from_url<'a>(
        &'a self,
        url: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<Vec<crate::domain::share::RawFile>>>> {
        Box::pin(ShareResolver::raw_files_from_url(self, url))
    }
}

pub trait DynTelegramExportStateRepository: Send + Sync {
    fn get<'a>(
        &'a self,
        source_type: &'a str,
        source_value: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<TelegramExportStateRecord>>>;
    fn upsert<'a>(&'a self, record: &'a TelegramExportStateRecord) -> BoxFuture<'a, AppResult<()>>;
}

impl<T: TelegramExportStateRepository + Send + Sync> DynTelegramExportStateRepository for T {
    fn get<'a>(
        &'a self,
        source_type: &'a str,
        source_value: &'a str,
    ) -> BoxFuture<'a, AppResult<Option<TelegramExportStateRecord>>> {
        Box::pin(TelegramExportStateRepository::get(
            self,
            source_type,
            source_value,
        ))
    }

    fn upsert<'a>(&'a self, record: &'a TelegramExportStateRecord) -> BoxFuture<'a, AppResult<()>> {
        Box::pin(TelegramExportStateRepository::upsert(self, record))
    }
}
