use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use reqwest::Url;

use super::*;
use crate::application::import::{
    LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo,
    SearchMovieResult, SearchTvResult, TvDetail,
};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::error::AppError;
use crate::infrastructure::import::local_store::FilesystemImportLocalStore;

#[derive(Debug, PartialEq, Eq)]
enum ImportedMediaSummary {
    Movie {
        title: String,
        year: String,
        size: u64,
        has_failed: bool,
    },
    Tv {
        name: String,
        year: String,
        season: u32,
        episodes: Vec<u32>,
        missing_episodes: Vec<u32>,
        max_episode_number: u32,
        total_size: u64,
        number_of_episodes: u32,
        has_failed: bool,
    },
}

fn summarize_imported(media: ImportedMedia) -> ImportedMediaSummary {
    match media {
        ImportedMedia::Movie {
            title,
            year,
            size,
            has_failed,
            ..
        } => ImportedMediaSummary::Movie {
            title,
            year,
            size,
            has_failed,
        },
        ImportedMedia::Tv {
            name,
            year,
            season,
            episodes,
            missing_episodes,
            max_episode_number,
            total_size,
            number_of_episodes,
            has_failed,
            ..
        } => ImportedMediaSummary::Tv {
            name,
            year,
            season,
            episodes,
            missing_episodes,
            max_episode_number,
            total_size,
            number_of_episodes,
            has_failed,
        },
    }
}

#[derive(Clone, Default)]
struct FakeLibraryGateway {
    state: Arc<Mutex<FakeLibraryState>>,
}

#[derive(Default)]
struct FakeLibraryState {
    dir_ids_by_path: HashMap<String, i64>,
    dir_ids_by_parent: HashMap<i64, HashMap<String, i64>>,
    files_by_dir_id: HashMap<i64, Vec<LibraryFile>>,
    mkdir_paths: Vec<String>,
    mkdir_dirs: Vec<(i64, String)>,
    fast_uploads: Vec<(i64, String, String, u64)>,
    trashed_file_ids: Vec<Vec<i64>>,
    fail_mkdir_path: bool,
    fail_mkdir_dir: bool,
    md5_upload_returns_none: bool,
}

#[derive(Clone, Default)]
struct FakeShareSource {
    calls: Arc<Mutex<Vec<String>>>,
    pan123_files: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
    pan189_share_info: Arc<Mutex<Option<Pan189ShareInfo>>>,
    pan189_files: Arc<Mutex<HashMap<String, (Vec<Pan189Folder>, Vec<Pan189File>)>>>,
    pan189_downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pan115_files: Arc<Mutex<HashMap<String, Vec<Pan115FileEntry>>>>,
}

#[derive(Clone, Default)]
struct FakeMetadataCatalog;

#[derive(Clone)]
struct FakeLocalStore {
    remote_path: String,
    local_root: PathBuf,
    strm_download_url: String,
    fail_remove: bool,
}

impl LibraryGateway for FakeLibraryGateway {
    async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .files_by_dir_id
            .get(&dir_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .dir_ids_by_path
            .get(path)
            .copied())
    }
    async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
        if self.state.lock().unwrap().fail_mkdir_path {
            return Err(AppError::Dependency("mkdir path failed".into()));
        }
        self.state
            .lock()
            .unwrap()
            .mkdir_paths
            .push(path.to_string());
        Ok(1)
    }
    async fn list_library_dir_ids(
        &self,
        dir_id: i64,
    ) -> AppResult<std::collections::HashMap<String, i64>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .dir_ids_by_parent
            .get(&dir_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64> {
        if self.state.lock().unwrap().fail_mkdir_dir {
            return Err(AppError::Dependency("mkdir dir failed".into()));
        }
        self.state
            .lock()
            .unwrap()
            .mkdir_dirs
            .push((parent_dir_id, folder_name.to_string()));
        Ok(10)
    }
    async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
        self.state
            .lock()
            .unwrap()
            .trashed_file_ids
            .push(file_ids.to_vec());
        Ok(())
    }
    async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        if self.state.lock().unwrap().md5_upload_returns_none {
            return Ok(None);
        }
        self.state.lock().unwrap().fast_uploads.push((
            parent_dir_id,
            file_name.to_string(),
            etag.to_string(),
            size,
        ));
        Ok(Some(42))
    }
    async fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        self.state.lock().unwrap().fast_uploads.push((
            parent_dir_id,
            file_name.to_string(),
            sha1.to_string(),
            size,
        ));
        Ok(Some(42))
    }
    async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
        Ok(())
    }
}

impl ShareSource for FakeShareSource {
    async fn list_pan123_share_files(
        &self,
        _share_key: &str,
        _share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<LibraryFile>> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok(self
            .pan123_files
            .lock()
            .unwrap()
            .get(&parent_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn get_pan189_share_info(&self, _share_code: &str) -> AppResult<Pan189ShareInfo> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok(self
            .pan189_share_info
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default())
    }
    async fn list_pan189_share_files(
        &self,
        _share_id: i64,
        _share_mode: i32,
        parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok(self
            .pan189_files
            .lock()
            .unwrap()
            .get(parent_id)
            .cloned()
            .unwrap_or((Vec::new(), Vec::new())))
    }
    async fn download_pan189_share_file(
        &self,
        _share_id: i64,
        file: &Pan189File,
    ) -> AppResult<Vec<u8>> {
        self.calls
            .lock()
            .unwrap()
            .push("share-download".to_string());
        self.pan189_downloads
            .lock()
            .unwrap()
            .get(&file.id)
            .cloned()
            .ok_or_else(|| AppError::InvalidParameter("missing fake pan189 download".into()))
    }
    async fn list_pan115_share_files(
        &self,
        _share_code: &str,
        _receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok(self
            .pan115_files
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .unwrap_or_default())
    }
}

impl MetadataCatalog for FakeMetadataCatalog {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
        if title == "Inception" && year == "2010" {
            Ok(vec![SearchMovieResult {
                id: 27205,
                title: "Inception".into(),
                original_title: "Inception".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }
    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        Ok((id == 27205).then_some(MovieDetail {
            id,
            title: "Inception".into(),
            adult: false,
            genres: Vec::new(),
            original_language: "en".into(),
            original_title: "Inception".into(),
            origin_country: vec!["US".into()],
            release_date: "2010-07-16".into(),
        }))
    }
    async fn search_tv(&self, _title: &str, year: &str) -> AppResult<Vec<SearchTvResult>> {
        if year == "2008" {
            Ok(vec![SearchTvResult {
                id: 1396,
                name: "Breaking Bad".into(),
                original_name: "Breaking Bad".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }
    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        Ok((id == 1396).then_some(TvDetail {
            id,
            name: "Breaking Bad".into(),
            first_air_date: "2008-01-20".into(),
            number_of_episodes: 7,
            number_of_seasons: 1,
            origin_country: vec!["US".into()],
            original_language: "en".into(),
            original_name: "Breaking Bad".into(),
            genres: Vec::new(),
            seasons: vec![crate::application::import::Season {
                id: 1,
                name: "Season 1".into(),
                episode_count: 7,
                season_number: 1,
            }],
        }))
    }
}

impl FakeLocalStore {
    fn new(local_root: PathBuf) -> Self {
        Self {
            remote_path: "/remote".into(),
            local_root,
            strm_download_url: "http://localhost/d".into(),
            fail_remove: false,
        }
    }
}

impl ImportLocalStore for FakeLocalStore {
    fn remote_library_path(&self) -> &str {
        self.remote_path.as_str()
    }

    fn local_path_for_remote(&self, remote_path: &str) -> String {
        let relative = remote_path
            .trim_start_matches(self.remote_path.as_str())
            .trim_start_matches('/');
        self.local_root
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        let local_file_path = self.local_path_for_remote(remote_file_path);
        if let Some(stripped) = local_file_path.strip_suffix(extension) {
            format!("{stripped}.strm")
        } else {
            format!("{local_file_path}.strm")
        }
    }

    async fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()> {
        let local_file_path = self.local_strm_path(remote_file_path, extension);
        let content = format!(
            "{}{}?file_id={}",
            self.strm_download_url, remote_file_path, file_id
        );
        tokio::fs::create_dir_all(PathBuf::from(&local_file_path).parent().unwrap()).await?;
        tokio::fs::write(local_file_path, content).await?;
        Ok(())
    }

    async fn remove_local_file_if_exists(&self, path: &str) -> AppResult<()> {
        if self.fail_remove {
            return Err(AppError::Internal("remove local file failed".into()));
        }
        if let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(AppError::Internal(format!(
                "remove local file failed, {err}"
            )));
        }
        Ok(())
    }

    async fn remove_local_dir_if_exists(&self, path: &str) -> AppResult<()> {
        if self.fail_remove {
            return Err(AppError::Internal("remove local dir failed".into()));
        }
        if let Err(err) = tokio::fs::remove_dir_all(path).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(AppError::Internal(format!(
                "remove local dir failed, {err}"
            )));
        }
        Ok(())
    }
}

#[tokio::test]
async fn service_delegates_to_gateway() {
    let share_source = FakeShareSource::default();
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        share_source.clone(),
        FakeMetadataCatalog,
        FilesystemImportLocalStore::new(
            "/remote".into(),
            "/local".into(),
            "http://localhost".into(),
        ),
    );
    let url = Url::parse("https://www.123684.com/s/test").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    service.import_from_share_url(&share).await.unwrap();

    assert_eq!(share_source.calls.lock().unwrap().as_slice(), ["share"]);
}

#[tokio::test]
async fn import_from_share_url_walks_pan189_entries_and_imports_tv() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let share_source = FakeShareSource {
        pan189_share_info: Arc::new(Mutex::new(Some(Pan189ShareInfo {
            file_id: "root".into(),
            file_name: "Breaking Bad (2008) {tmdb-1396}".into(),
            share_id: 1,
            share_mode: 2,
        }))),
        pan189_files: Arc::new(Mutex::new(HashMap::from([
            (
                "root".to_string(),
                (
                    vec![Pan189Folder {
                        id: "season-1".into(),
                        name: "Season 01".into(),
                    }],
                    Vec::new(),
                ),
            ),
            (
                "season-1".to_string(),
                (
                    Vec::new(),
                    vec![Pan189File {
                        id: "episode-1".into(),
                        name: "Breaking.Bad.S01E01.1080p.mkv".into(),
                        size: 1001,
                        md5: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    }],
                ),
            ),
        ]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        gateway.clone(),
        share_source.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://cloud.189.cn/t/share189").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let imported = service.import_from_share_url(&share).await.unwrap();

    assert_eq!(share_source.calls.lock().unwrap().len(), 3);
    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv { name, season, episodes, total_size, has_failed, .. }]
            if name == "Breaking Bad" && *season == 1 && episodes == &vec![1] && *total_size == 1001 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string()]
    );
    assert_eq!(state.mkdir_dirs, vec![(1, "Season 01".to_string())]);
    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_pan189_share_reports_cas_only_entries() {
    let local_dir = unique_temp_dir();
    let share_source = FakeShareSource {
        pan189_share_info: Arc::new(Mutex::new(Some(Pan189ShareInfo {
            file_id: "root".into(),
            file_name: "Veil of Shadows".into(),
            share_id: 1,
            share_mode: 3,
        }))),
        pan189_files: Arc::new(Mutex::new(HashMap::from([(
            "root".to_string(),
            (
                Vec::new(),
                vec![Pan189File {
                    id: "cas-1".into(),
                    name: "Veil.of.Shadows.S01E01.2160p.mp4.cas".into(),
                    size: 288,
                    md5: "79202e0c3975e92c12ee2b543ebcd968".into(),
                }],
            ),
        )]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        share_source,
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://cloud.189.cn/t/share189").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let error = service.import_from_share_url(&share).await.unwrap_err();

    assert!(error.to_string().contains("天翼 CAS 秒传分享"));
    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_pan189_share_imports_downloaded_cas_entries() {
    let local_dir = unique_temp_dir();
    let share_source = FakeShareSource {
        pan189_share_info: Arc::new(Mutex::new(Some(Pan189ShareInfo {
            file_id: "root".into(),
            file_name: "Breaking Bad (2008) {tmdb-1396}".into(),
            share_id: 1,
            share_mode: 3,
        }))),
        pan189_files: Arc::new(Mutex::new(HashMap::from([(
            "root".to_string(),
            (
                Vec::new(),
                vec![Pan189File {
                    id: "cas-1".into(),
                    name: "Breaking.Bad.S01E01.1080p.mkv.cas".into(),
                    size: 288,
                    md5: "79202e0c3975e92c12ee2b543ebcd968".into(),
                }],
            ),
        )]))),
        pan189_downloads: Arc::new(Mutex::new(HashMap::from([(
            "cas-1".to_string(),
            serde_json::json!({
                "fileName": "Breaking Bad (2008) {tmdb-1396}/Breaking.Bad.S01E01.1080p.mkv",
                "md5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 1001
            })
            .to_string()
            .into_bytes(),
        )]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        share_source,
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://cloud.189.cn/t/share189").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let imported = service.import_from_share_url(&share).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv { name, season, episodes, total_size, has_failed, .. }]
            if name == "Breaking Bad" && *season == 1 && episodes == &vec![1] && *total_size == 1001 && !has_failed
    ));
    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_pan189_share_uses_cas_file_name_as_context_path() {
    let local_dir = unique_temp_dir();
    let share_source = FakeShareSource {
        pan189_share_info: Arc::new(Mutex::new(Some(Pan189ShareInfo {
            file_id: "root".into(),
            file_name: "share-root".into(),
            share_id: 1,
            share_mode: 3,
        }))),
        pan189_files: Arc::new(Mutex::new(HashMap::from([(
            "root".to_string(),
            (
                Vec::new(),
                vec![Pan189File {
                    id: "cas-1".into(),
                    name: "Breaking Bad (2008) {tmdb-1396}.cas".into(),
                    size: 288,
                    md5: "79202e0c3975e92c12ee2b543ebcd968".into(),
                }],
            ),
        )]))),
        pan189_downloads: Arc::new(Mutex::new(HashMap::from([(
            "cas-1".to_string(),
            serde_json::json!({
                "fileName": "S01E01.mp4",
                "md5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 1001
            })
            .to_string()
            .into_bytes(),
        )]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        share_source,
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://cloud.189.cn/t/share189").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let imported = service.import_from_share_url(&share).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv { name, season, episodes, total_size, has_failed, .. }]
            if name == "Breaking Bad" && *season == 1 && episodes == &vec![1] && *total_size == 1001 && !has_failed
    ));
    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_share_url_walks_pan115_entries_and_imports_tv() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let share_source = FakeShareSource {
        pan115_files: Arc::new(Mutex::new(HashMap::from([
            (
                "0".to_string(),
                vec![Pan115FileEntry {
                    fid: None,
                    cid: Some("show-1".into()),
                    name: "Breaking Bad (2008) {tmdb-1396}".into(),
                    size: 0,
                    sha: None,
                }],
            ),
            (
                "show-1".to_string(),
                vec![Pan115FileEntry {
                    fid: None,
                    cid: Some("season-1".into()),
                    name: "Season 01".into(),
                    size: 0,
                    sha: None,
                }],
            ),
            (
                "season-1".to_string(),
                vec![Pan115FileEntry {
                    fid: Some("file-1".into()),
                    cid: None,
                    name: "Breaking.Bad.S01E01.1080p.mkv".into(),
                    size: 1001,
                    sha: Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()),
                }],
            ),
        ]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        gateway.clone(),
        share_source.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://115.com/s/share115?password=recv").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let imported = service.import_from_share_url(&share).await.unwrap();

    assert_eq!(share_source.calls.lock().unwrap().len(), 3);
    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv { name, season, episodes, total_size, has_failed, .. }]
            if name == "Breaking Bad" && *season == 1 && episodes == &vec![1] && *total_size == 1001 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string()]
    );
    assert_eq!(state.mkdir_dirs, vec![(1, "Season 01".to_string())]);
    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_share_url_returns_error_when_pan189_share_code_is_missing() {
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        FakeLocalStore::new(unique_temp_dir()),
    );
    let url = Url::parse("https://cloud.189.cn/web/share").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let error = service.import_from_share_url(&share).await.unwrap_err();

    assert!(error.to_string().contains("Can not extract share code"));
}

#[tokio::test]
async fn import_from_share_url_returns_error_when_pan115_share_code_is_missing() {
    let service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        FakeLocalStore::new(unique_temp_dir()),
    );
    let url = Url::parse("https://115.com/s/").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let error = service.import_from_share_url(&share).await.unwrap_err();

    assert!(error.to_string().contains("Can not extract share code"));
}

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "bigbrother-import-test-{}-{suffix}-{sequence}",
        std::process::id()
    ))
}

fn local_store_for(local_dir: &PathBuf) -> FilesystemImportLocalStore {
    FilesystemImportLocalStore::new(
        "/remote".into(),
        local_dir.to_string_lossy().into_owned(),
        "http://localhost/d".into(),
    )
}

fn existing_library_file(file_id: i64, file_name: &str, size: u64, etag: &str) -> LibraryFile {
    LibraryFile {
        file_id,
        file_name: file_name.to_string(),
        is_dir: false,
        size,
        etag: etag.to_string(),
    }
}

fn existing_season_dir(
    state: &mut FakeLibraryState,
    parent_dir_id: i64,
    season_dir_name: &str,
    season_dir_id: i64,
) {
    state
        .dir_ids_by_parent
        .entry(parent_dir_id)
        .or_default()
        .insert(season_dir_name.to_string(), season_dir_id);
}

#[tokio::test]
async fn import_from_json_writes_strm_and_records_upload() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie {
            title,
            year,
            size,
            has_failed,
            ..
        }] if title == "Inception" && year == "2010" && *size == 1234 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string()]
    );
    assert_eq!(
        state.fast_uploads,
        vec![(
            1,
            "Inception.2010.1080p.mkv".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
            1234
        )]
    );
    drop(state);

    let strm_path =
        local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
    let strm_content = fs::read_to_string(&strm_path).unwrap();
    assert_eq!(
        strm_content,
        "http://localhost/d/remote/电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.mkv?file_id=42"
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_groups_tv_episodes_and_writes_season_strms() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [
            {
                "path": "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E01.1080p.mkv",
                "etag": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 1001u64
            },
            {
                "path": "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E02.1080p.mkv",
                "etag": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size": 1002u64
            }
        ]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    let ImportedMedia::Tv {
        name,
        year,
        season,
        has_failed,
        episodes,
        max_episode_number,
        number_of_episodes,
        missing_episodes,
        total_size,
        ..
    } = imported
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected one tv import result"))
    else {
        panic!("expected tv import result");
    };

    assert_eq!(name, "Breaking Bad");
    assert_eq!(year, "2008");
    assert_eq!(season, 1);
    assert!(!has_failed);
    assert_eq!(episodes, vec![1, 2]);
    assert_eq!(max_episode_number, 2);
    assert_eq!(number_of_episodes, 7);
    assert!(missing_episodes.is_empty(), "{missing_episodes:?}");
    assert_eq!(total_size, 2003);

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string()]
    );
    assert_eq!(state.mkdir_dirs, vec![(1, "Season 01".to_string())]);
    assert_eq!(
        state.fast_uploads,
        vec![
            (
                10,
                "Breaking Bad.2008.S01E01.1080p.mkv".to_string(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                1001
            ),
            (
                10,
                "Breaking Bad.2008.S01E02.1080p.mkv".to_string(),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                1002
            )
        ]
    );
    drop(state);

    let season_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    let ep1 = fs::read_to_string(season_dir.join("Breaking Bad.2008.S01E01.1080p.strm")).unwrap();
    let ep2 = fs::read_to_string(season_dir.join("Breaking Bad.2008.S01E02.1080p.strm")).unwrap();
    assert_eq!(
        ep1,
        "http://localhost/d/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking Bad.2008.S01E01.1080p.mkv?file_id=42"
    );
    assert_eq!(
        ep2,
        "http://localhost/d/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking Bad.2008.S01E02.1080p.mkv?file_id=42"
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_share_url_walks_pan123_entries_and_imports_movie() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let share_source = FakeShareSource {
        pan123_files: Arc::new(Mutex::new(HashMap::from([
            (
                0,
                vec![
                    LibraryFile {
                        file_id: 100,
                        file_name: "Movies".into(),
                        is_dir: true,
                        size: 0,
                        etag: String::new(),
                    },
                    LibraryFile {
                        file_id: 101,
                        file_name: "ignore.txt".into(),
                        is_dir: false,
                        size: 10,
                        etag: "etag-ignore".into(),
                    },
                ],
            ),
            (
                100,
                vec![LibraryFile {
                    file_id: 102,
                    file_name: "Inception.2010.1080p.mkv".into(),
                    is_dir: false,
                    size: 2234,
                    etag: "fedcba9876543210fedcba9876543210".into(),
                }],
            ),
        ]))),
        ..Default::default()
    };
    let service = ImportMediaService::new(
        gateway.clone(),
        share_source.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let url = Url::parse("https://www.123684.com/s/test?pwd=pass").unwrap();
    let share = ShareUrl::from(&url).unwrap();

    let imported = service.import_from_share_url(&share).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie { title, year, size, has_failed, .. }]
            if title == "Inception" && year == "2010" && *size == 2234 && !has_failed
    ));
    assert_eq!(
        share_source.calls.lock().unwrap().as_slice(),
        ["share", "share"]
    );

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string()]
    );
    assert_eq!(
        state.fast_uploads,
        vec![(
            1,
            "Inception.2010.1080p.mkv".to_string(),
            "fedcba9876543210fedcba9876543210".to_string(),
            2234
        )]
    );
    drop(state);

    let strm_path =
        local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
    assert!(strm_path.exists());

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_fslink_parses_prefixed_common_path_and_writes_strm() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let fslink = "123FSLinkV2$共享目录/Movies%0123456789abcdef0123456789abcdef#1234#Inception.2010.1080p.mkv";

    let imported = service.import_from_fslink(fslink).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie {
            title,
            year,
            size,
            has_failed,
            ..
        }] if title == "Inception" && year == "2010" && *size == 1234 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert_eq!(
        state.mkdir_paths,
        vec!["/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string()]
    );
    assert_eq!(
        state.fast_uploads,
        vec![(
            1,
            "Inception.2010.1080p.mkv".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
            1234
        )]
    );
    drop(state);

    let strm_path =
        local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
    let strm_content = fs::read_to_string(&strm_path).unwrap();
    assert_eq!(
        strm_content,
        "http://localhost/d/remote/电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.mkv?file_id=42"
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_skips_when_existing_movie_is_not_smaller() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let movie_path = "/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string();
    {
        let mut state = gateway.state.lock().unwrap();
        state.dir_ids_by_path.insert(movie_path, 99);
        state.files_by_dir_id.insert(
            99,
            vec![existing_library_file(
                501,
                "Inception.2010.2160p.mkv",
                4321,
                "etag-existing-large",
            )],
        );
    }
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(imported.is_empty());

    let state = gateway.state.lock().unwrap();
    assert!(state.mkdir_paths.is_empty());
    assert!(state.fast_uploads.is_empty());
    assert!(state.trashed_file_ids.is_empty());
    drop(state);

    let strm_path =
        local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
    assert!(!strm_path.exists());

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_overwrites_when_incoming_movie_is_larger() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let movie_path = "/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string();
    {
        let mut state = gateway.state.lock().unwrap();
        state.dir_ids_by_path.insert(movie_path.clone(), 77);
        state.files_by_dir_id.insert(
            77,
            vec![existing_library_file(
                601,
                "Inception.2010.720p.mkv",
                900,
                "etag-existing-small",
            )],
        );
    }

    let old_local_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(old_local_dir.join("Inception.2010.720p.strm"), "old").unwrap();

    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie {
            title,
            year,
            size,
            has_failed,
            ..
        }] if title == "Inception" && year == "2010" && *size == 1234 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert!(state.mkdir_paths.is_empty());
    assert_eq!(
        state.fast_uploads,
        vec![(
            77,
            "Inception.2010.1080p.mkv".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
            1234
        )]
    );
    assert_eq!(state.trashed_file_ids, vec![vec![601]]);
    drop(state);

    assert!(!old_local_dir.join("Inception.2010.720p.strm").exists());
    assert!(old_local_dir.join("Inception.2010.1080p.strm").exists());

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn movie_import_contract_is_consistent_across_json_share_and_fslink() {
    let json_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );
    let share_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource {
            pan123_files: Arc::new(Mutex::new(HashMap::from([(
                0,
                vec![LibraryFile {
                    file_id: 102,
                    file_name: "Inception.2010.1080p.mkv".into(),
                    is_dir: false,
                    size: 2234,
                    etag: "fedcba9876543210fedcba9876543210".into(),
                }],
            )]))),
            ..Default::default()
        },
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );
    let fslink_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "fedcba9876543210fedcba9876543210",
            "size": 2234u64
        }]
    }))
    .unwrap();
    let share_url = Url::parse("https://www.123684.com/s/test?pwd=pass").unwrap();
    let share = ShareUrl::from(&share_url).unwrap();
    let fslink = "123FSLinkV2$共享目录/Movies%fedcba9876543210fedcba9876543210#2234#Inception.2010.1080p.mkv";

    let from_json = json_service.import_from_json(json).await.unwrap();
    let from_share = share_service.import_from_share_url(&share).await.unwrap();
    let from_fslink = fslink_service.import_from_fslink(fslink).await.unwrap();

    assert_eq!(from_json.len(), 1);
    assert_eq!(from_share.len(), 1);
    assert_eq!(from_fslink.len(), 1);

    let from_json = summarize_imported(from_json.into_iter().next().unwrap());
    let from_share = summarize_imported(from_share.into_iter().next().unwrap());
    let from_fslink = summarize_imported(from_fslink.into_iter().next().unwrap());

    assert_eq!(from_json, from_share);
    assert_eq!(from_json, from_fslink);
}

#[tokio::test]
async fn tv_import_contract_is_consistent_across_json_share_and_fslink() {
    let json_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );
    let share_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource {
            pan123_files: Arc::new(Mutex::new(HashMap::from([(
                0,
                vec![
                    LibraryFile {
                        file_id: 201,
                        file_name: "Breaking.Bad.2008.S01E01.1080p.mkv".into(),
                        is_dir: false,
                        size: 1001,
                        etag: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    },
                    LibraryFile {
                        file_id: 202,
                        file_name: "Breaking.Bad.2008.S01E02.1080p.mkv".into(),
                        is_dir: false,
                        size: 1002,
                        etag: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                    },
                ],
            )]))),
            ..Default::default()
        },
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );
    let fslink_service = ImportMediaService::new(
        FakeLibraryGateway::default(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&unique_temp_dir()),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [
            {
                "path": "Breaking.Bad.2008.S01E01.1080p.mkv",
                "etag": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 1001u64
            },
            {
                "path": "Breaking.Bad.2008.S01E02.1080p.mkv",
                "etag": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size": 1002u64
            }
        ]
    }))
    .unwrap();
    let share_url = Url::parse("https://www.123684.com/s/test?pwd=pass").unwrap();
    let share = ShareUrl::from(&share_url).unwrap();
    let fslink = "123FSLinkV2$共享目录/TV%aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#1001#Breaking.Bad.2008.S01E01.1080p.mkv$bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb#1002#Breaking.Bad.2008.S01E02.1080p.mkv";

    let from_json = json_service.import_from_json(json).await.unwrap();
    let from_share = share_service.import_from_share_url(&share).await.unwrap();
    let from_fslink = fslink_service.import_from_fslink(fslink).await.unwrap();

    assert_eq!(from_json.len(), 1);
    assert_eq!(from_share.len(), 1);
    assert_eq!(from_fslink.len(), 1);

    let from_json = summarize_imported(from_json.into_iter().next().unwrap());
    let from_share = summarize_imported(from_share.into_iter().next().unwrap());
    let from_fslink = summarize_imported(from_fslink.into_iter().next().unwrap());

    assert_eq!(from_json, from_share);
    assert_eq!(from_json, from_fslink);
}

#[tokio::test]
async fn import_from_json_skips_existing_tv_episode_when_existing_is_not_smaller() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let tv_path = "/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string();
    {
        let mut state = gateway.state.lock().unwrap();
        state.dir_ids_by_path.insert(tv_path.clone(), 88);
        existing_season_dir(&mut state, 88, "Season 01", 89);
        state.files_by_dir_id.insert(
            89,
            vec![existing_library_file(
                701,
                "Breaking Bad.2008.S01E01.2160p.mkv",
                2001,
                "etag-existing-large-tv",
            )],
        );
    }
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Breaking.Bad.2008.S01E01.1080p.mkv",
            "etag": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "size": 1001u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv {
            season,
            episodes,
            total_size,
            has_failed,
            ..
        }] if *season == 1 && episodes.is_empty() && *total_size == 0 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert!(state.mkdir_paths.is_empty());
    assert!(state.mkdir_dirs.is_empty());
    assert!(state.fast_uploads.is_empty());
    assert!(state.trashed_file_ids.is_empty());
    drop(state);

    let season_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    assert!(
        !season_dir
            .join("Breaking Bad.2008.S01E01.1080p.strm")
            .exists()
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_overwrites_existing_tv_episode_when_incoming_is_larger() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let tv_path = "/remote/电视剧/欧美/Breaking Bad (2008) {tmdb-1396}".to_string();
    {
        let mut state = gateway.state.lock().unwrap();
        state.dir_ids_by_path.insert(tv_path.clone(), 98);
        existing_season_dir(&mut state, 98, "Season 01", 99);
        state.files_by_dir_id.insert(
            99,
            vec![existing_library_file(
                801,
                "Breaking Bad.2008.S01E01.720p.mkv",
                900,
                "etag-existing-small-tv",
            )],
        );
    }

    let old_local_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(
        old_local_dir.join("Breaking Bad.2008.S01E01.720p.strm"),
        "old",
    )
    .unwrap();

    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Breaking.Bad.2008.S01E01.1080p.mkv",
            "etag": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "size": 1001u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Tv {
            season,
            episodes,
            total_size,
            has_failed,
            ..
        }] if *season == 1 && episodes == &vec![1] && *total_size == 1001 && !has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert!(state.mkdir_paths.is_empty());
    assert!(state.mkdir_dirs.is_empty());
    assert_eq!(
        state.fast_uploads,
        vec![(
            99,
            "Breaking Bad.2008.S01E01.1080p.mkv".to_string(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1001
        )]
    );
    assert_eq!(state.trashed_file_ids, vec![vec![801]]);
    drop(state);

    assert!(
        !old_local_dir
            .join("Breaking Bad.2008.S01E01.720p.strm")
            .exists()
    );
    assert!(
        old_local_dir
            .join("Breaking Bad.2008.S01E01.1080p.strm")
            .exists()
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_returns_error_when_library_dir_creation_fails() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway.state.lock().unwrap().fail_mkdir_path = true;
    let service = ImportMediaService::new(
        gateway,
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let error = service.import_from_json(json).await.unwrap_err();

    assert_eq!(error.kind(), crate::error::AppErrorKind::Dependency);
    assert!(error.to_string().contains("mkdir path failed"));

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_marks_movie_failed_when_upload_returns_none() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway.state.lock().unwrap().md5_upload_returns_none = true;
    let service = ImportMediaService::new(
        gateway.clone(),
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let imported = service.import_from_json(json).await.unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie {
            title,
            year,
            size,
            has_failed,
            ..
        }] if title == "Inception" && year == "2010" && *size == 1234 && *has_failed
    ));

    let state = gateway.state.lock().unwrap();
    assert_eq!(state.fast_uploads.len(), 0);
    drop(state);

    let strm_path =
        local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
    assert!(!strm_path.exists());

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_json_returns_error_when_local_cleanup_fails_on_overwrite() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let movie_path = "/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string();
    {
        let mut state = gateway.state.lock().unwrap();
        state.dir_ids_by_path.insert(movie_path, 77);
        state.files_by_dir_id.insert(
            77,
            vec![existing_library_file(
                601,
                "Inception.2010.720p.mkv",
                900,
                "etag-existing-small",
            )],
        );
    }

    let old_local_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(old_local_dir.join("Inception.2010.720p.strm"), "old").unwrap();

    let mut local_store = FakeLocalStore::new(local_dir.clone());
    local_store.fail_remove = true;
    let service = ImportMediaService::new(
        gateway,
        FakeShareSource::default(),
        FakeMetadataCatalog,
        local_store,
    );

    let json = serde_json::to_vec(&serde_json::json!({
        "files": [{
            "path": "Inception.2010.1080p.mkv",
            "etag": "0123456789abcdef0123456789abcdef",
            "size": 1234u64
        }]
    }))
    .unwrap();

    let error = service.import_from_json(json).await.unwrap_err();

    assert_eq!(error.kind(), crate::error::AppErrorKind::Internal);
    assert!(error.to_string().contains("remove local file failed"));

    let _ = fs::remove_dir_all(local_dir);
}
