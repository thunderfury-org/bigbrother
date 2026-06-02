use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::identify::MediaIdentifyService;
use super::*;
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, TitleExtractor,
};
use crate::domain::media::Title;
use crate::domain::share::{FileHash, RawFile};
use crate::error::{AppError, AppResult};

pub(crate) struct TestImportService<L, M, F> {
    pub transfer: TransferWorkflow<L, F>,
    pub identify_service: MediaIdentifyService<M, FakeTitleExtractor>,
    pub metadata_lookup: MetadataLookup,
}

impl<L, M, F> TestImportService<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub fn new(library_gateway: L, metadata_catalog: M, local_store: F) -> Self {
        Self {
            transfer: TransferWorkflow::new(library_gateway, local_store),
            identify_service: MediaIdentifyService::new(metadata_catalog, FakeTitleExtractor),
            metadata_lookup: MetadataLookup::default(),
        }
    }

    pub async fn import_from_raw_files(
        &mut self,
        raw_files: Vec<RawFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        let media_files = self
            .metadata_lookup
            .build_media_files(raw_files, Vec::new());
        let outcome = self.identify_service.identify(&media_files).await?;
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
}

#[derive(Clone, Default)]
struct FakeMetadataCatalog;

#[derive(Clone)]
pub(crate) struct FakeTitleExtractor;

impl TitleExtractor for FakeTitleExtractor {
    async fn extract_title(&self, _description: &str) -> AppResult<Option<Title>> {
        Ok(None)
    }
}

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
    async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        if self.state.lock().unwrap().md5_upload_returns_none {
            return Ok(None);
        }
        self.state.lock().unwrap().fast_uploads.push((
            parent_dir_id,
            file_name.to_string(),
            hash.to_string(),
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

fn local_store_for(local_dir: &Path) -> FakeLocalStore {
    FakeLocalStore::new(local_dir.to_path_buf())
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
    let mut service = TestImportService::new(
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
    let mut service = TestImportService::new(
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
async fn import_from_raw_files_imports_movie() {
    let local_dir = unique_temp_dir();
    let gateway = FakeLibraryGateway::default();
    let mut service = TestImportService::new(
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
    let mut service = TestImportService::new(
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

    let mut service = TestImportService::new(
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
    let mut service = TestImportService::new(
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

    let mut service = TestImportService::new(
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
    let mut service =
        TestImportService::new(gateway, FakeMetadataCatalog, local_store_for(&local_dir));
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
    let mut service = TestImportService::new(
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

    let mut local_store = FakeLocalStore::new(local_dir.clone());
    local_store.fail_remove = true;
    let mut service = TestImportService::new(gateway, FakeMetadataCatalog, local_store);
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
