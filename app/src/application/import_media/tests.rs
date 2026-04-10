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
use crate::application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource};
use crate::infrastructure::import::local_store::FilesystemImportLocalStore;

#[derive(Clone, Default)]
struct FakeLibraryGateway {
    state: Arc<Mutex<FakeLibraryState>>,
}

#[derive(Default)]
struct FakeLibraryState {
    mkdir_paths: Vec<String>,
    mkdir_dirs: Vec<(i64, String)>,
    fast_uploads: Vec<(i64, String, String, u64)>,
}

#[derive(Clone, Default)]
struct FakeShareSource {
    calls: Arc<Mutex<Vec<String>>>,
    pan123_files: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
}

#[derive(Clone, Default)]
struct FakeMetadataCatalog;

impl LibraryGateway for FakeLibraryGateway {
    async fn list_library_files(&self, _dir_id: i64) -> AppResult<Vec<LibraryFile>> {
        Ok(Vec::new())
    }
    async fn get_library_dir_id_by_path(&self, _path: &str) -> AppResult<Option<i64>> {
        Ok(None)
    }
    async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
        self.state
            .lock()
            .unwrap()
            .mkdir_paths
            .push(path.to_string());
        Ok(1)
    }
    async fn list_library_dir_ids(
        &self,
        _dir_id: i64,
    ) -> AppResult<std::collections::HashMap<String, i64>> {
        Ok(Default::default())
    }
    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64> {
        self.state
            .lock()
            .unwrap()
            .mkdir_dirs
            .push((parent_dir_id, folder_name.to_string()));
        Ok(10)
    }
    async fn trash_library_files(&self, _file_ids: &[i64]) -> AppResult<()> {
        Ok(())
    }
    async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
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
        _parent_dir_id: i64,
        _file_name: &str,
        _sha1: &str,
        _size: u64,
    ) -> AppResult<Option<i64>> {
        Ok(None)
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
        Ok(Default::default())
    }
    async fn list_pan189_share_files(
        &self,
        _share_id: i64,
        _share_mode: i32,
        _parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok((Vec::new(), Vec::new()))
    }
    async fn list_pan115_share_files(
        &self,
        _share_code: &str,
        _receive_code: &str,
        _cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>> {
        self.calls.lock().unwrap().push("share".to_string());
        Ok(Vec::new())
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
