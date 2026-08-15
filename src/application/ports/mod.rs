pub mod erase;
pub mod import;
pub mod notify;
pub mod share;

pub use erase::{
    DownloadUrlCacheHandle, DownloadUrlSourceHandle, FileIndexRepo, FileStoreHandle,
    ImportLocalStoreHandle, ImportRecordRepo, LibraryGatewayHandle, LibraryRemoteHandle,
    MediaSearchHandle, MetadataCatalogHandle, ShareResolverHandle, SubscriptionRepo,
    TelegramExportStateRepo, TitleExtractorHandle,
};
pub use import::{ImportLocalStore, LibraryGateway, MetadataCatalog, TitleExtractor};
pub use notify::{Message, MessageSender};
pub use share::ShareResolver;

use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{
    domain::{
        import_record::{ImportSourceKind, ImportStatus},
        subscription::SubscriptionMediaType,
    },
    error::AppResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDirectoryRecord {
    pub dir_id: i64,
    pub display_name: String,
    pub remote_path: String,
}

pub trait MediaSearchSource {
    fn search_media_dirs(
        &self,
        keyword: &str,
    ) -> impl std::future::Future<Output = AppResult<Vec<MediaDirectoryRecord>>> + Send;
}

pub trait DownloadUrlCache {
    fn get_download_url(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<String>>> + Send;
    fn set_download_url(
        &self,
        key: &str,
        url: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadUrlError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("not found, {0}")]
    NotFound(String),

    #[error("error, {0}")]
    Error(String),
}

pub type DownloadUrlResult<T> = std::result::Result<T, DownloadUrlError>;

pub trait DownloadUrlSource {
    fn get_download_url(
        &self,
        file_id: i64,
    ) -> impl std::future::Future<Output = DownloadUrlResult<String>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub file_id: i64,
    pub file_name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub trait LibraryRemote {
    fn get_file_id_by_path(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<i64>>> + Send;
    fn list_dir(
        &self,
        dir_id: i64,
    ) -> impl std::future::Future<Output = AppResult<Vec<RemoteEntry>>> + Send;
    fn download_file(
        &self,
        file_id: i64,
        local_path: &str,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub path: String,
    pub is_dir: bool,
}

pub trait FileStore {
    fn read_to_string_if_exists(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<String>>> + Send;
    fn metadata_len_if_exists(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<u64>>> + Send;
    fn ensure_parent_dir(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn write(
        &self,
        path: &str,
        content: &[u8],
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn read_dir(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<Vec<LocalEntry>>> + Send;
    fn remove_file(&self, path: &str) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn remove_dir_all(&self, path: &str)
    -> impl std::future::Future<Output = AppResult<()>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndexRecordInput {
    pub size: u64,
    pub hash_type: String,
    pub hash_value: String,
    pub file_name: String,
    pub file_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLocationRecord {
    pub file_name: String,
    pub file_path: String,
    pub descriptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchRecord {
    pub id: i64,
    pub size: u64,
    pub hash_type: String,
    pub hash_value: String,
    pub locations: Vec<FileLocationRecord>,
}

pub trait FileIndexRepository {
    fn record_files(
        &self,
        files: &[FileIndexRecordInput],
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn search_files(
        &self,
        keyword: &str,
        limit: u64,
    ) -> impl std::future::Future<Output = AppResult<Vec<FileSearchRecord>>> + Send;
    fn get_records_by_ids(
        &self,
        ids: &[i64],
    ) -> impl std::future::Future<Output = AppResult<Vec<FileSearchRecord>>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramExportStateRecord {
    pub source_type: String,
    pub source_value: String,
    pub description: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub attempt_count: i64,
    pub first_seen_at: String,
    pub last_attempt_at: String,
}

pub trait TelegramExportStateRepository {
    fn get(
        &self,
        source_type: &str,
        source_value: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<TelegramExportStateRecord>>> + Send;
    fn upsert(
        &self,
        record: &TelegramExportStateRecord,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecordCreate {
    pub source_kind: ImportSourceKind,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecordFinalize {
    pub status: ImportStatus,
    pub summary_json: String,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub finished_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecordView {
    pub id: i64,
    pub source_kind: ImportSourceKind,
    pub source: String,
    pub status: ImportStatus,
    pub summary_json: Option<String>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportRecordFilter {
    pub status: Option<ImportStatus>,
    pub source_kind: Option<ImportSourceKind>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportRecordPaging {
    pub cursor: Option<i64>,
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecordPage {
    pub items: Vec<ImportRecordView>,
    pub next_cursor: Option<i64>,
}

pub trait ImportRecordRepository {
    fn create(
        &self,
        input: &ImportRecordCreate,
    ) -> impl std::future::Future<Output = AppResult<i64>> + Send;
    fn finalize(
        &self,
        id: i64,
        update: &ImportRecordFinalize,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn get(
        &self,
        id: i64,
    ) -> impl std::future::Future<Output = AppResult<Option<ImportRecordView>>> + Send;
    fn list(
        &self,
        filter: &ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> impl std::future::Future<Output = AppResult<ImportRecordPage>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRecord {
    pub id: i64,
    pub tmdb_id: u32,
    pub media_type: SubscriptionMediaType,
    pub title_zh: Option<String>,
    pub title_en: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionCreateInput {
    pub tmdb_id: u32,
    pub media_type: SubscriptionMediaType,
    pub title_zh: Option<String>,
    pub title_en: Option<String>,
}

pub trait SubscriptionRepository {
    fn list_all(
        &self,
    ) -> impl std::future::Future<Output = AppResult<Vec<SubscriptionRecord>>> + Send;
    fn get_by_id(
        &self,
        id: i64,
    ) -> impl std::future::Future<Output = AppResult<Option<SubscriptionRecord>>> + Send;
    fn find_by_tmdb_id(
        &self,
        tmdb_id: u32,
        media_type: &SubscriptionMediaType,
    ) -> impl std::future::Future<Output = AppResult<Option<SubscriptionRecord>>> + Send;
    fn create(
        &self,
        input: &SubscriptionCreateInput,
    ) -> impl std::future::Future<Output = AppResult<i64>> + Send;
    fn delete(&self, id: i64) -> impl std::future::Future<Output = AppResult<()>> + Send;
}
