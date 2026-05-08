use sha2::{Digest, Sha256};
use tracing::warn;
use url::Url;

use crate::{
    application::{
        import::ShareUrl,
        import_media::ImportMediaService,
        import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource},
        ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    },
    domain::import::inner::{Etag, RawFile},
    error::{AppError, AppErrorKind, AppResult},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenFileHash {
    Md5(String),
    Sha1(String),
    #[allow(dead_code)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenFile {
    pub size: u64,
    pub hash: SeenFileHash,
    pub file_name: String,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileIndexSource {
    ShareUrl(String),
    Fslink(String),
    LocalJsonFile(String),
}

impl SeenFile {
    pub fn from_raw_file(file: &RawFile) -> Self {
        let hash = match &file.etag {
            Etag::Md5(value) => SeenFileHash::Md5(value.clone()),
            Etag::Sha1(value) => SeenFileHash::Sha1(value.clone()),
        };

        Self {
            size: file.size,
            hash,
            file_name: file.name.clone(),
            file_path: file.path.clone(),
        }
    }
}

#[derive(Clone)]
pub struct FileIndexService<R> {
    repo: R,
}

impl<R> FileIndexService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R> FileIndexService<R>
where
    R: FileIndexRepository,
{
    pub async fn record_seen_files(
        &self,
        files: Vec<SeenFile>,
        description: Option<String>,
    ) -> AppResult<()> {
        let description = normalize_optional_text(description);
        let inputs = files
            .into_iter()
            .filter_map(|file| to_record_input(file, description.clone()))
            .collect::<Vec<_>>();

        self.repo.record_files(&inputs).await
    }

    pub async fn search_files(
        &self,
        keyword: &str,
        limit: u64,
    ) -> AppResult<Vec<FileSearchRecord>> {
        self.repo.search_files(keyword.trim(), limit).await
    }
}

pub trait FileIndexRawFileSource: Clone {
    async fn raw_files_from_share_url_string(&self, url: &str) -> AppResult<Vec<RawFile>>;
    async fn raw_files_from_fslink_string(&self, fslink: &str) -> AppResult<Vec<RawFile>>;
    async fn raw_files_from_json_bytes(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>>;
}

impl<L, S, M, F> FileIndexRawFileSource for ImportMediaService<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    async fn raw_files_from_share_url_string(&self, raw_url: &str) -> AppResult<Vec<RawFile>> {
        let url = Url::parse(raw_url).map_err(|err| {
            AppError::InvalidParameter(format!("invalid share url '{raw_url}': {err}"))
        })?;
        let share = ShareUrl::from(&url).ok_or_else(|| {
            AppError::InvalidParameter(format!(
                "unsupported share url '{raw_url}', expected pan123, pan189, pan115, or quark share link"
            ))
        })?;
        self.raw_files_from_share_url(&share).await
    }

    async fn raw_files_from_fslink_string(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
        self.raw_files_from_fslink(fslink).await
    }

    async fn raw_files_from_json_bytes(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
        self.raw_files_from_json(json).await
    }
}

#[derive(Clone)]
pub struct FileIndexIngestService<I, R> {
    source: I,
    file_index: FileIndexService<R>,
}

impl<I, R> FileIndexIngestService<I, R> {
    pub fn new(source: I, file_index: FileIndexService<R>) -> Self {
        Self { source, file_index }
    }
}

impl<I, R> FileIndexIngestService<I, R>
where
    I: FileIndexRawFileSource,
    R: FileIndexRepository,
{
    pub async fn ingest_sources(
        &self,
        sources: Vec<FileIndexSource>,
        description: Option<String>,
    ) -> AppResult<()> {
        for source in sources {
            self.ingest_source(source, description.clone()).await?;
        }
        Ok(())
    }

    pub async fn ingest_sources_from_event(
        &self,
        sources: Vec<FileIndexSource>,
        description: Option<String>,
    ) -> AppResult<()> {
        for source in sources {
            let source_for_log = source.clone();
            match self
                .ingest_source(source.clone(), description.clone())
                .await
            {
                Ok(()) => {
                    cleanup_local_event_source(&source_for_log).await;
                }
                Err(err) if is_permanent_index_source_error(&err) => {
                    cleanup_local_event_source(&source_for_log).await;
                    warn!(
                        source = ?source_for_log,
                        error = %err,
                        "skipping permanent file index source error"
                    );
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    async fn ingest_source(
        &self,
        source: FileIndexSource,
        description: Option<String>,
    ) -> AppResult<()> {
        let raw_files = match source {
            FileIndexSource::ShareUrl(raw_url) => {
                self.source
                    .raw_files_from_share_url_string(&raw_url)
                    .await?
            }
            FileIndexSource::Fslink(fslink) => {
                self.source.raw_files_from_fslink_string(&fslink).await?
            }
            FileIndexSource::LocalJsonFile(path) => {
                let json = tokio::fs::read(&path).await.map_err(|err| {
                    AppError::InvalidParameter(format!(
                        "failed to read local index source '{path}': {err}"
                    ))
                })?;
                self.source.raw_files_from_json_bytes(json).await?
            }
        };
        let seen = raw_files.iter().map(SeenFile::from_raw_file).collect();
        self.file_index.record_seen_files(seen, description).await
    }
}

fn is_permanent_index_source_error(error: &AppError) -> bool {
    match error.kind() {
        AppErrorKind::InvalidParameter | AppErrorKind::NotFound | AppErrorKind::RuleRejected => {
            true
        }
        AppErrorKind::Dependency => false,
        AppErrorKind::Runtime | AppErrorKind::Internal => false,
    }
}

async fn cleanup_local_event_source(source: &FileIndexSource) {
    let FileIndexSource::LocalJsonFile(path) = source else {
        return;
    };

    if let Err(err) = tokio::fs::remove_file(path).await {
        warn!(
            source = ?source,
            error = %err,
            "failed to remove local file index source"
        );
    }
}

pub fn location_hash(file_path: &str, file_name: &str) -> String {
    hash_hex(format!(
        "v1\0{}\0{}",
        normalize_file_path(file_path),
        file_name.trim()
    ))
}

pub fn description_hash(description: &str) -> String {
    hash_hex(description.trim())
}

fn to_record_input(file: SeenFile, description: Option<String>) -> Option<FileIndexRecordInput> {
    if file.size == 0 {
        return None;
    }

    let (md5, sha1) = match file.hash {
        SeenFileHash::Md5(value) => (normalize_hash(value), None),
        SeenFileHash::Sha1(value) => (None, normalize_hash(value)),
        SeenFileHash::Unknown(value) => match normalize_hash(value) {
            Some(hash) if hash.len() == 32 => (Some(hash), None),
            Some(hash) if hash.len() == 40 => (None, Some(hash)),
            _ => (None, None),
        },
    };

    if md5.is_none() && sha1.is_none() {
        return None;
    }

    Some(FileIndexRecordInput {
        size: file.size,
        md5,
        sha1,
        file_name: file.file_name.trim().to_owned(),
        file_path: normalize_file_path(&file.file_path),
        description,
    })
}

fn normalize_hash(value: String) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_file_path(value: &str) -> String {
    match value.trim() {
        "/" => String::new(),
        value => value.to_owned(),
    }
}

fn hash_hex(value: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(value.as_ref());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
    }

    impl FileIndexRepository for FakeRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
            self.recorded.lock().unwrap().extend_from_slice(files);
            Ok(())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn record_seen_files_filters_files_without_hash_or_size() {
        let repo = FakeRepo::default();
        let service = FileIndexService::new(repo.clone());

        service
            .record_seen_files(
                vec![
                    SeenFile {
                        size: 0,
                        hash: SeenFileHash::Md5("abc".into()),
                        file_name: "zero.mkv".into(),
                        file_path: "/a".into(),
                    },
                    SeenFile {
                        size: 10,
                        hash: SeenFileHash::Unknown(String::new()),
                        file_name: "missing.mkv".into(),
                        file_path: "/a".into(),
                    },
                    SeenFile {
                        size: 20,
                        hash: SeenFileHash::Md5(" ABCDEF ".into()),
                        file_name: "movie.mkv".into(),
                        file_path: " / ".into(),
                    },
                    SeenFile {
                        size: 30,
                        hash: SeenFileHash::Sha1(
                            " 0123456789012345678901234567890123456789 ".into(),
                        ),
                        file_name: "episode.mkv".into(),
                        file_path: "/Shows".into(),
                    },
                ],
                Some(" desc ".into()),
            )
            .await
            .unwrap();

        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].md5.as_deref(), Some("abcdef"));
        assert_eq!(recorded[0].sha1, None);
        assert_eq!(recorded[0].file_path, "");
        assert_eq!(recorded[0].description.as_deref(), Some("desc"));
        assert_eq!(
            recorded[1].sha1.as_deref(),
            Some("0123456789012345678901234567890123456789")
        );
    }

    #[test]
    fn hashes_location_with_version_and_null_separators() {
        assert_eq!(
            location_hash("/path", "file.mkv"),
            location_hash(" /path ", " file.mkv ")
        );
        assert_ne!(
            location_hash("/pa", "thfile"),
            location_hash("/path", "file")
        );
        assert_eq!(
            location_hash("/", "file.mkv"),
            location_hash("", "file.mkv")
        );
    }

    #[test]
    fn hashes_description_after_trim() {
        assert_eq!(description_hash(" hello "), description_hash("hello"));
        assert_ne!(description_hash("hello"), description_hash("Hello"));
    }
}

#[cfg(test)]
mod ingest_tests {
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::domain::import::inner::{Etag, RawFile};

    #[derive(Clone, Default)]
    struct FakeRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
    }

    impl FileIndexRepository for FakeRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
            self.recorded.lock().unwrap().extend_from_slice(files);
            Ok(())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct FakeRawFileSource {
        share_results: Arc<Mutex<HashMap<String, AppResult<Vec<RawFile>>>>>,
        json_result: Arc<Mutex<Option<AppResult<Vec<RawFile>>>>>,
    }

    impl FakeRawFileSource {
        fn with_share_result(self, url: &str, result: AppResult<Vec<RawFile>>) -> Self {
            self.share_results
                .lock()
                .unwrap()
                .insert(url.to_owned(), result);
            self
        }

        fn with_json_result(self, result: AppResult<Vec<RawFile>>) -> Self {
            *self.json_result.lock().unwrap() = Some(result);
            self
        }
    }

    impl FileIndexRawFileSource for FakeRawFileSource {
        async fn raw_files_from_share_url_string(&self, url: &str) -> AppResult<Vec<RawFile>> {
            self.share_results
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or_else(|| {
                    Err(AppError::InvalidParameter(format!(
                        "missing fake share result for {url}"
                    )))
                })
        }

        async fn raw_files_from_fslink_string(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
            Err(AppError::InvalidParameter(format!(
                "missing fake fslink result for {fslink}"
            )))
        }

        async fn raw_files_from_json_bytes(&self, _json: Vec<u8>) -> AppResult<Vec<RawFile>> {
            self.json_result.lock().unwrap().clone().unwrap_or_else(|| {
                Err(AppError::InvalidParameter(
                    "missing fake json result".to_owned(),
                ))
            })
        }
    }

    fn raw_file(name: &str) -> RawFile {
        RawFile {
            id: None,
            name: name.to_owned(),
            etag: Etag::Md5("0123456789abcdef0123456789abcdef".into()),
            size: 100,
            path: "/Movies".into(),
        }
    }

    fn temp_index_file(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("bigbrother-file-index-{nanos}-{name}.json"))
    }

    #[test]
    fn raw_file_conversion_preserves_path_name_size_and_hash() {
        let file = SeenFile::from_raw_file(&RawFile {
            id: None,
            name: "movie.mkv".into(),
            etag: Etag::Md5("ABC".into()),
            size: 100,
            path: "/Movies".into(),
        });

        assert_eq!(file.file_name, "movie.mkv");
        assert_eq!(file.file_path, "/Movies");
        assert_eq!(file.size, 100);
        assert_eq!(file.hash, SeenFileHash::Md5("ABC".into()));
    }

    #[tokio::test]
    async fn event_ingest_skips_pan189_share_audit_failure_and_continues() {
        let repo = FakeRepo::default();
        let source = FakeRawFileSource::default()
            .with_share_result(
                "https://cloud.189.cn/t/bad",
                Err(AppError::RuleRejected(
                    "request error, share audit not pass".into(),
                )),
            )
            .with_share_result(
                "https://www.123pan.com/s/good",
                Ok(vec![raw_file("movie.mkv")]),
            );
        let service = FileIndexIngestService::new(source, FileIndexService::new(repo.clone()));

        service
            .ingest_sources_from_event(
                vec![
                    FileIndexSource::ShareUrl("https://cloud.189.cn/t/bad".into()),
                    FileIndexSource::ShareUrl("https://www.123pan.com/s/good".into()),
                ],
                Some("seen resource".into()),
            )
            .await
            .unwrap();

        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].file_name, "movie.mkv");
        assert_eq!(recorded[0].description.as_deref(), Some("seen resource"));
    }

    #[tokio::test]
    async fn event_ingest_returns_transient_dependency_error_for_retry() {
        let repo = FakeRepo::default();
        let source = FakeRawFileSource::default().with_share_result(
            "https://www.123pan.com/s/transient",
            Err(AppError::Dependency("request error, timeout".into())),
        );
        let service = FileIndexIngestService::new(source, FileIndexService::new(repo));

        let error = service
            .ingest_sources_from_event(
                vec![FileIndexSource::ShareUrl(
                    "https://www.123pan.com/s/transient".into(),
                )],
                None,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Dependency(message) if message.contains("timeout")));
    }

    #[tokio::test]
    async fn event_ingest_deletes_local_json_source_after_success() {
        let repo = FakeRepo::default();
        let path = temp_index_file("success");
        tokio::fs::write(&path, b"{}").await.unwrap();
        let source = FakeRawFileSource::default().with_json_result(Ok(vec![raw_file("movie.mkv")]));
        let service = FileIndexIngestService::new(source, FileIndexService::new(repo));

        service
            .ingest_sources_from_event(
                vec![FileIndexSource::LocalJsonFile(
                    path.to_string_lossy().to_string(),
                )],
                None,
            )
            .await
            .unwrap();

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn event_ingest_deletes_local_json_source_after_permanent_error() {
        let repo = FakeRepo::default();
        let path = temp_index_file("permanent");
        tokio::fs::write(&path, b"{}").await.unwrap();
        let source = FakeRawFileSource::default()
            .with_json_result(Err(AppError::InvalidParameter("bad json".into())));
        let service = FileIndexIngestService::new(source, FileIndexService::new(repo));

        service
            .ingest_sources_from_event(
                vec![FileIndexSource::LocalJsonFile(
                    path.to_string_lossy().to_string(),
                )],
                None,
            )
            .await
            .unwrap();

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn event_ingest_keeps_local_json_source_for_retryable_error() {
        let repo = FakeRepo::default();
        let path = temp_index_file("retry");
        tokio::fs::write(&path, b"{}").await.unwrap();
        let source = FakeRawFileSource::default()
            .with_json_result(Err(AppError::Dependency("timeout".into())));
        let service = FileIndexIngestService::new(source, FileIndexService::new(repo));

        let error = service
            .ingest_sources_from_event(
                vec![FileIndexSource::LocalJsonFile(
                    path.to_string_lossy().to_string(),
                )],
                None,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Dependency(message) if message.contains("timeout")));
        assert!(path.exists());
        let _ = tokio::fs::remove_file(path).await;
    }
}
