use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::identify::MediaIdentifyService;
use super::*;
use crate::application::{
    import_local_store::ImportLocalStore,
    ports::{
        FileStore, LibraryGateway, LibraryMediaUpdate, LibraryMediaUpdateKind,
        LibraryUpdateNotifier, LocalEntry, MetadataCatalog, NoopLibraryUpdateNotifier,
        TitleExtractor, library_update::test_support::RecordingLibraryUpdateNotifier,
    },
};
use crate::domain::import::{
    LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, TvDetail,
};
use crate::domain::media::Title;
use crate::domain::share::{FileHash, RawFile};
use crate::error::{AppError, AppResult};

pub(crate) struct TestImportService {
    pub transfer: TransferWorkflow,
    pub identify_service: MediaIdentifyService,
    pub metadata_lookup: MetadataLookup,
}

impl TestImportService {
    pub fn new(
        library_gateway: impl LibraryGateway + 'static,
        metadata_catalog: impl MetadataCatalog + 'static,
        local_store: ImportLocalStore,
    ) -> Self {
        Self::with_notifier(
            library_gateway,
            metadata_catalog,
            local_store,
            NoopLibraryUpdateNotifier,
        )
    }

    pub fn with_notifier(
        library_gateway: impl LibraryGateway + 'static,
        metadata_catalog: impl MetadataCatalog + 'static,
        local_store: ImportLocalStore,
        notifier: impl LibraryUpdateNotifier + 'static,
    ) -> Self {
        Self {
            transfer: TransferWorkflow::new(
                library_gateway,
                local_store,
                std::sync::Arc::new(notifier),
            ),
            identify_service: MediaIdentifyService::new(metadata_catalog, FakeTitleExtractor),
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub async fn import_from_raw_files(
        &self,
        raw_files: Vec<RawFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let media_files = self
            .metadata_lookup
            .build_media_files(raw_files, Vec::new());
        let outcome = self.identify_service.identify(media_files).await?;
        self.transfer
            .import_groups(outcome.groups, outcome.unmatched)
            .await
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
    upload_none_for: HashSet<String>,
    fail_uploads_for: HashSet<String>,
    upload_delay: Duration,
    current_in_flight: usize,
    max_in_flight: usize,
}

#[derive(Clone, Default)]
struct FakeMetadataCatalog;

#[derive(Clone)]
pub(crate) struct FakeTitleExtractor;

#[async_trait::async_trait]
impl TitleExtractor for FakeTitleExtractor {
    async fn extract_title(&self, _description: &str) -> AppResult<Option<Title>> {
        Ok(None)
    }
}

#[derive(Clone)]
struct TempFileStore {
    fail_remove: bool,
}

#[async_trait::async_trait]
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
    async fn ensure_dir(&self, path: &str) -> AppResult<i64> {
        if let Some(id) = self
            .state
            .lock()
            .unwrap()
            .dir_ids_by_path
            .get(path)
            .copied()
        {
            return Ok(id);
        }
        if self.state.lock().unwrap().fail_mkdir_path {
            return Err(AppError::ExternalService("mkdir path failed".into(), false));
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
            return Err(AppError::ExternalService("mkdir dir failed".into(), false));
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
    async fn upload(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &FileHash,
        size: u64,
    ) -> AppResult<Option<i64>> {
        let (delay, fail) = {
            let mut state = self.state.lock().unwrap();
            if matches!(hash, FileHash::Md5(_)) && state.md5_upload_returns_none {
                return Ok(None);
            }
            if state
                .upload_none_for
                .iter()
                .any(|needle| file_name.contains(needle))
            {
                return Ok(None);
            }
            let fail = state
                .fail_uploads_for
                .iter()
                .any(|needle| file_name.contains(needle));
            state.current_in_flight += 1;
            state.max_in_flight = state.max_in_flight.max(state.current_in_flight);
            (state.upload_delay, fail)
        };
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let mut state = self.state.lock().unwrap();
        state.current_in_flight -= 1;
        if fail {
            return Err(AppError::ExternalService("upload failed".into(), false));
        }
        state.fast_uploads.push((
            parent_dir_id,
            file_name.to_string(),
            hash.hash_value().to_string(),
            size,
        ));
        Ok(Some(42))
    }
    async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
        Ok(())
    }
    async fn search_media_dirs(
        &self,
        _keyword: &str,
    ) -> AppResult<Vec<crate::application::ports::MediaDirectoryRecord>> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl MetadataCatalog for FakeMetadataCatalog {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
        if title == "Inception" && year == "2010" {
            Ok(vec![SearchMovieResult {
                id: 27205,
                title: "Inception".into(),
                original_title: "Inception".into(),
                ..Default::default()
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
                ..Default::default()
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
            seasons: vec![crate::domain::import::Season {
                id: 1,
                name: "Season 1".into(),
                episode_count: 7,
                season_number: 1,
            }],
        }))
    }
}

#[async_trait::async_trait]
impl FileStore for TempFileStore {
    async fn read_to_string_if_exists(&self, path: &str) -> AppResult<Option<String>> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(Some(content)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    async fn metadata_len_if_exists(&self, path: &str) -> AppResult<Option<u64>> {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    async fn ensure_parent_dir(&self, path: &str) -> AppResult<()> {
        let parent = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;
        Ok(())
    }

    async fn write(&self, path: &str, content: &[u8]) -> AppResult<()> {
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    async fn read_dir(&self, _path: &str) -> AppResult<Vec<LocalEntry>> {
        Ok(Vec::new())
    }

    async fn remove_file(&self, path: &str) -> AppResult<()> {
        tokio::fs::remove_file(path).await?;
        Ok(())
    }

    async fn remove_dir_all(&self, path: &str) -> AppResult<()> {
        tokio::fs::remove_dir_all(path).await?;
        Ok(())
    }

    async fn remove_file_if_exists(&self, path: &str) -> AppResult<()> {
        if self.fail_remove {
            return Err(AppError::Internal("remove local file failed".into()));
        }
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::Internal(format!(
                "remove local file failed, {err}"
            ))),
        }
    }

    async fn remove_dir_all_if_exists(&self, path: &str) -> AppResult<()> {
        if self.fail_remove {
            return Err(AppError::Internal("remove local dir failed".into()));
        }
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(AppError::Internal(format!(
                "remove local dir failed, {err}"
            ))),
        }
    }
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

fn local_store_for(local_dir: &Path) -> ImportLocalStore {
    ImportLocalStore::new(
        TempFileStore { fail_remove: false },
        "/remote".into(),
        local_dir.to_string_lossy().into_owned(),
        "http://localhost/d".into(),
    )
}
fn raw_file(path: &str, hash: &str, size: u64) -> RawFile {
    let path = Path::new(path);
    let parent_path = path
        .parent()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    RawFile {
        id: None,
        name: name.to_string(),
        hash: FileHash::from(hash),
        size,
        path: parent_path.to_string(),
    }
}

fn existing_library_file(file_id: i64, file_name: &str, size: u64, hash: &str) -> LibraryFile {
    LibraryFile {
        file_id,
        file_name: file_name.to_string(),
        is_dir: false,
        size,
        hash: hash.to_string(),
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
async fn import_from_raw_files_writes_strm_and_records_upload() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_groups_tv_episodes_and_writes_season_strms() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E01.1080p.mkv",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1001,
            ),
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E02.1080p.mkv",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1002,
            ),
        ])
        .await
        .unwrap();

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
    let mut uploads = state.fast_uploads.clone();
    uploads.sort_by(|left, right| left.1.cmp(&right.1));
    assert_eq!(
        uploads,
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
async fn import_from_raw_files_transfers_tv_episodes_concurrently() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway.state.lock().unwrap().upload_delay = Duration::from_millis(50);
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let imported = service
        .import_from_raw_files(vec![
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E01.1080p.mkv",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1001,
            ),
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E02.1080p.mkv",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1002,
            ),
        ])
        .await
        .unwrap();

    let ImportedMedia::Tv {
        episodes,
        has_failed,
        ..
    } = imported
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected one tv import result"))
    else {
        panic!("expected tv import result");
    };
    assert_eq!(episodes, vec![1, 2]);
    assert!(!has_failed);

    let state = gateway.state.lock().unwrap();
    assert!(
        state.max_in_flight >= 2,
        "expected overlapping episode uploads, max_in_flight={}",
        state.max_in_flight
    );
    drop(state);

    let season_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    assert!(
        season_dir
            .join("Breaking Bad.2008.S01E01.1080p.strm")
            .exists()
    );
    assert!(
        season_dir
            .join("Breaking Bad.2008.S01E02.1080p.strm")
            .exists()
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_raw_files_marks_failed_tv_episode_when_upload_returns_none() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway
        .state
        .lock()
        .unwrap()
        .upload_none_for
        .insert("S01E02".to_string());
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let imported = service
        .import_from_raw_files(vec![
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E01.1080p.mkv",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1001,
            ),
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E02.1080p.mkv",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1002,
            ),
        ])
        .await
        .unwrap();

    let ImportedMedia::Tv {
        episodes,
        failed_episodes,
        has_failed,
        total_size,
        ..
    } = imported
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected one tv import result"))
    else {
        panic!("expected tv import result");
    };
    assert_eq!(episodes, vec![1]);
    assert_eq!(failed_episodes, vec![2]);
    assert!(has_failed);
    assert_eq!(total_size, 1001);

    let season_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    assert!(
        season_dir
            .join("Breaking Bad.2008.S01E01.1080p.strm")
            .exists()
    );
    assert!(
        !season_dir
            .join("Breaking Bad.2008.S01E02.1080p.strm")
            .exists()
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_raw_files_finishes_in_flight_tv_episode_when_another_upload_fails() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    {
        let mut state = gateway.state.lock().unwrap();
        state.upload_delay = Duration::from_millis(50);
        state.fail_uploads_for.insert("S01E01".to_string());
    }
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );

    let error = service
        .import_from_raw_files(vec![
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E01.1080p.mkv",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1001,
            ),
            raw_file(
                "Breaking Bad (2008) {tmdb-1396}/Season 01/Breaking.Bad.S01E02.1080p.mkv",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1002,
            ),
        ])
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::ExternalService(_, _)));
    assert!(error.to_string().contains("upload failed"));

    let season_dir = local_dir.join("电视剧/欧美/Breaking Bad (2008) {tmdb-1396}/Season 01");
    assert!(
        !season_dir
            .join("Breaking Bad.2008.S01E01.1080p.strm")
            .exists()
    );
    assert!(
        season_dir
            .join("Breaking Bad.2008.S01E02.1080p.strm")
            .exists(),
        "in-flight episode should finish after a sibling upload error"
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_raw_files_imports_movie() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![RawFile {
            id: None,
            name: "Inception.2010.1080p.mkv".into(),
            hash: "fedcba9876543210fedcba9876543210".into(),
            size: 2234,
            path: "Movies".into(),
        }])
        .await
        .unwrap();

    assert!(matches!(
        imported.as_slice(),
        [ImportedMedia::Movie { title, year, size, has_failed, .. }]
            if title == "Inception" && year == "2010" && *size == 2234 && !has_failed
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
async fn import_from_raw_files_skips_when_existing_movie_is_not_smaller() {
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
                "hash-existing-large",
            )],
        );
    }
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_overwrites_when_incoming_movie_is_larger() {
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
                "hash-existing-small",
            )],
        );
    }

    let old_local_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(old_local_dir.join("Inception.2010.720p.strm"), "old").unwrap();

    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_skips_existing_tv_episode_when_existing_is_not_smaller() {
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
                "hash-existing-large-tv",
            )],
        );
    }
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Breaking.Bad.2008.S01E01.1080p.mkv",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1001,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_overwrites_existing_tv_episode_when_incoming_is_larger() {
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
                "hash-existing-small-tv",
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

    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Breaking.Bad.2008.S01E01.1080p.mkv",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1001,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_returns_error_when_library_dir_creation_fails() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway.state.lock().unwrap().fail_mkdir_path = true;
    let service = TestImportService::new(gateway, FakeMetadataCatalog, local_store_for(&local_dir));
    let error = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        crate::error::AppError::ExternalService(_, _)
    ));
    assert!(error.to_string().contains("mkdir path failed"));

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_from_raw_files_marks_movie_failed_when_upload_returns_none() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    gateway.state.lock().unwrap().md5_upload_returns_none = true;
    let service = TestImportService::new(
        gateway.clone(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

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
async fn import_from_raw_files_returns_error_when_local_cleanup_fails_on_overwrite() {
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
                "hash-existing-small",
            )],
        );
    }

    let old_local_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(old_local_dir.join("Inception.2010.720p.strm"), "old").unwrap();

    let local_store = ImportLocalStore::new(
        TempFileStore { fail_remove: true },
        "/remote".into(),
        local_dir.to_string_lossy().into_owned(),
        "http://localhost/d".into(),
    );
    let service = TestImportService::new(gateway, FakeMetadataCatalog, local_store);
    let error = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap_err();

    assert!(matches!(error, crate::error::AppError::Internal(_)));
    assert!(error.to_string().contains("remove local file failed"));

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_notifies_created_strm_and_subtitle() {
    let local_dir = unique_temp_dir();
    let notifier = RecordingLibraryUpdateNotifier::default();
    let service = TestImportService::with_notifier(
        FakeLibraryGateway::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
        notifier.clone(),
    );
    service
        .import_from_raw_files(vec![
            raw_file(
                "Inception.2010.1080p.mkv",
                "0123456789abcdef0123456789abcdef",
                1234,
            ),
            raw_file(
                "Inception.2010.1080p.zh.srt",
                "fedcba9876543210fedcba9876543210",
                12,
            ),
        ])
        .await
        .unwrap();

    let movie_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    assert_eq!(
        notifier.batches(),
        vec![vec![
            LibraryMediaUpdate {
                path: movie_dir
                    .join("Inception.2010.1080p.zh.srt")
                    .to_string_lossy()
                    .into_owned(),
                kind: LibraryMediaUpdateKind::Created,
            },
            LibraryMediaUpdate {
                path: movie_dir
                    .join("Inception.2010.1080p.strm")
                    .to_string_lossy()
                    .into_owned(),
                kind: LibraryMediaUpdateKind::Created,
            },
        ]]
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_notifies_created_and_deleted_when_replacing_movie() {
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
                "hash-existing-small",
            )],
        );
    }

    let old_local_dir = local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}");
    fs::create_dir_all(&old_local_dir).unwrap();
    fs::write(old_local_dir.join("Inception.2010.720p.strm"), "old").unwrap();

    let notifier = RecordingLibraryUpdateNotifier::default();
    let service = TestImportService::with_notifier(
        gateway,
        FakeMetadataCatalog,
        local_store_for(&local_dir),
        notifier.clone(),
    );
    service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

    assert_eq!(
        notifier.flat_updates(),
        vec![
            LibraryMediaUpdate {
                path: old_local_dir
                    .join("Inception.2010.1080p.strm")
                    .to_string_lossy()
                    .into_owned(),
                kind: LibraryMediaUpdateKind::Created,
            },
            LibraryMediaUpdate {
                path: old_local_dir
                    .join("Inception.2010.720p.strm")
                    .to_string_lossy()
                    .into_owned(),
                kind: LibraryMediaUpdateKind::Deleted,
            },
        ]
    );

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_skips_notify_when_existing_movie_is_kept() {
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
                "hash-existing-large",
            )],
        );
    }
    let notifier = RecordingLibraryUpdateNotifier::default();
    let service = TestImportService::with_notifier(
        gateway,
        FakeMetadataCatalog,
        local_store_for(&local_dir),
        notifier.clone(),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

    assert!(imported.is_empty());
    assert!(notifier.batches().is_empty());

    let _ = fs::remove_dir_all(local_dir);
}

#[tokio::test]
async fn import_succeeds_when_library_notify_fails() {
    let local_dir = unique_temp_dir();
    let notifier = RecordingLibraryUpdateNotifier::failing();
    let service = TestImportService::with_notifier(
        FakeLibraryGateway::default(),
        FakeMetadataCatalog,
        local_store_for(&local_dir),
        notifier.clone(),
    );
    let imported = service
        .import_from_raw_files(vec![raw_file(
            "Inception.2010.1080p.mkv",
            "0123456789abcdef0123456789abcdef",
            1234,
        )])
        .await
        .unwrap();

    assert_eq!(imported.len(), 1);
    assert_eq!(notifier.flat_updates().len(), 1);

    let _ = fs::remove_dir_all(local_dir);
}
