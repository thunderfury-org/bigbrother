pub mod import;
pub mod notify;
pub mod share;

pub use import::{
    LibraryGateway, LibraryGatewayHandle, MetadataCatalog, MetadataCatalogHandle, TitleExtractor,
    TitleExtractorHandle,
};
pub use notify::{Message, MessageSender};
pub use share::{ShareResolver, ShareResolverHandle};

use std::{sync::Arc, time::Duration};

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

#[async_trait::async_trait]
pub trait DownloadUrlCache: Send + Sync {
    async fn get_download_url(&self, key: &str) -> AppResult<Option<String>>;
    async fn set_download_url(&self, key: &str, url: &str, ttl: Duration) -> AppResult<()>;
}

pub type DownloadUrlCacheHandle = Arc<dyn DownloadUrlCache>;

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

#[async_trait::async_trait]
pub trait DownloadUrlSource: Send + Sync {
    async fn get_download_url(&self, file_id: i64) -> DownloadUrlResult<String>;
}

pub type DownloadUrlSourceHandle = Arc<dyn DownloadUrlSource>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub path: String,
    pub is_dir: bool,
}

#[async_trait::async_trait]
pub trait FileStore: Send + Sync {
    async fn read_to_string_if_exists(&self, path: &str) -> AppResult<Option<String>>;
    async fn metadata_len_if_exists(&self, path: &str) -> AppResult<Option<u64>>;
    async fn ensure_parent_dir(&self, path: &str) -> AppResult<()>;
    async fn write(&self, path: &str, content: &[u8]) -> AppResult<()>;
    async fn read_dir(&self, path: &str) -> AppResult<Vec<LocalEntry>>;
    async fn remove_file(&self, path: &str) -> AppResult<()>;
    async fn remove_dir_all(&self, path: &str) -> AppResult<()>;
    async fn remove_file_if_exists(&self, path: &str) -> AppResult<()>;
    async fn remove_dir_all_if_exists(&self, path: &str) -> AppResult<()>;
}

pub type FileStoreHandle = Arc<dyn FileStore>;

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

#[async_trait::async_trait]
pub trait FileIndexRepository: Send + Sync {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()>;
    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>;
    async fn get_records_by_ids(&self, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>>;
}

pub type FileIndexRepo = Arc<dyn FileIndexRepository>;

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

#[async_trait::async_trait]
pub trait TelegramExportStateRepository: Send + Sync {
    async fn get(
        &self,
        source_type: &str,
        source_value: &str,
    ) -> AppResult<Option<TelegramExportStateRecord>>;
    async fn upsert(&self, record: &TelegramExportStateRecord) -> AppResult<()>;
}

pub type TelegramExportStateRepo = Arc<dyn TelegramExportStateRepository>;

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

#[async_trait::async_trait]
pub trait ImportRecordRepository: Send + Sync {
    async fn create(&self, input: &ImportRecordCreate) -> AppResult<i64>;
    async fn finalize(&self, id: i64, update: &ImportRecordFinalize) -> AppResult<()>;
    async fn get(&self, id: i64) -> AppResult<Option<ImportRecordView>>;
    async fn list(
        &self,
        filter: &ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> AppResult<ImportRecordPage>;
}

pub type ImportRecordRepo = Arc<dyn ImportRecordRepository>;

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

#[async_trait::async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>>;
    async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>>;
    async fn find_by_tmdb_id(
        &self,
        tmdb_id: u32,
        media_type: &SubscriptionMediaType,
    ) -> AppResult<Option<SubscriptionRecord>>;
    async fn create(&self, input: &SubscriptionCreateInput) -> AppResult<i64>;
    async fn delete(&self, id: i64) -> AppResult<()>;
}

pub type SubscriptionRepo = Arc<dyn SubscriptionRepository>;
