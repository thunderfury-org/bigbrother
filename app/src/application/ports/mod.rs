use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{
    domain::import_record::{ImportSourceKind, ImportStatus},
    error::AppResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordRecord {
    pub id: i64,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDirectoryRecord {
    pub dir_id: i64,
    pub display_name: String,
    pub remote_path: String,
}

pub trait KeywordRepository {
    async fn list_all_keywords(&self) -> AppResult<Vec<KeywordRecord>>;
    async fn add_keyword(&self, value: &str) -> AppResult<()>;
    async fn delete_keyword(&self, id: i64) -> AppResult<()>;
}

pub trait MediaSearchSource {
    async fn search_media_dirs(&self, keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>>;
}

pub trait DownloadUrlCache {
    async fn get_download_url(&self, key: &str) -> AppResult<Option<String>>;
    async fn set_download_url(&self, key: &str, url: &str, ttl: Duration) -> AppResult<()>;
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
    async fn get_download_url(&self, file_id: i64) -> DownloadUrlResult<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub file_id: i64,
    pub file_name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub trait LibraryRemote {
    async fn get_file_id_by_path(&self, path: &str) -> AppResult<Option<i64>>;
    async fn list_dir(&self, dir_id: i64) -> AppResult<Vec<RemoteEntry>>;
    async fn download_file(&self, file_id: i64, local_path: &str) -> AppResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntry {
    pub path: String,
    pub is_dir: bool,
}

pub trait FileStore {
    async fn read_to_string_if_exists(&self, path: &str) -> AppResult<Option<String>>;
    async fn metadata_len_if_exists(&self, path: &str) -> AppResult<Option<u64>>;
    async fn ensure_parent_dir(&self, path: &str) -> AppResult<()>;
    async fn write(&self, path: &str, content: &[u8]) -> AppResult<()>;
    async fn read_dir(&self, path: &str) -> AppResult<Vec<LocalEntry>>;
    async fn remove_file(&self, path: &str) -> AppResult<()>;
    async fn remove_dir_all(&self, path: &str) -> AppResult<()>;
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

pub trait FileIndexRepository: Clone {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()>;
    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>;
    async fn get_records_by_ids(&self, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>>;
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

pub trait TelegramExportStateRepository: Clone {
    async fn get(
        &self,
        source_type: &str,
        source_value: &str,
    ) -> AppResult<Option<TelegramExportStateRecord>>;
    async fn upsert(&self, record: &TelegramExportStateRecord) -> AppResult<()>;
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

pub trait ImportRecordRepository: Clone {
    async fn create(&self, input: &ImportRecordCreate) -> AppResult<i64>;
    async fn finalize(&self, id: i64, update: &ImportRecordFinalize) -> AppResult<()>;
    async fn get(&self, id: i64) -> AppResult<Option<ImportRecordView>>;
    async fn list(
        &self,
        filter: &ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> AppResult<ImportRecordPage>;
}
