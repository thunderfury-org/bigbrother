use std::time::Duration;

use crate::error::AppResult;

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
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub file_name: String,
    pub file_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchRecord {
    pub file_name: String,
    pub file_path: String,
    pub size: u64,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub descriptions: Vec<String>,
}

pub trait FileIndexRepository: Clone {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<usize>;
    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>;
}
