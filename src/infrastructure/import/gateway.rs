use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    application::ports::{
        DownloadUrlSource, LibraryGateway, MediaDirectoryRecord, MetadataCatalog,
    },
    domain::import::{
        Genre, LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, Season, TvDetail,
    },
    domain::share::FileHash,
    error::{AppError, AppResult},
    infrastructure::{
        cache::Cache,
        client::{RequestError, pan123, tmdb},
    },
};

#[derive(Clone)]
pub struct PanLibraryGateway {
    pan123: pan123::Client,
    dir_cache: Arc<RwLock<LibraryDirCache>>,
}

#[derive(Default)]
struct LibraryDirCache {
    path_to_id: HashMap<String, i64>,
    id_to_path: HashMap<i64, String>,
    children: HashMap<i64, HashMap<String, i64>>,
}

#[derive(Clone)]
pub struct TmdbMetadataGateway {
    tmdb: tmdb::Client,
    cache: Option<Cache>,
}

impl PanLibraryGateway {
    pub fn new(pan123: pan123::Client) -> Self {
        Self {
            pan123,
            dir_cache: Arc::new(RwLock::new(LibraryDirCache::new())),
        }
    }

    async fn resolve_dir(&self, path: &str, create: bool) -> AppResult<Option<i64>> {
        let normalized = normalize_dir_path(path);
        {
            let cache = self.dir_cache.read().await;
            if let Some(id) = cache.get_path(&normalized) {
                return Ok(Some(id));
            }
        }

        let parts = path_parts(path);
        if parts.is_empty() {
            return Ok(Some(0));
        }

        let mut current_id = 0i64;
        let mut current_path = String::new();
        for part in parts {
            let next_path = join_dir_path(&current_path, part);
            {
                let cache = self.dir_cache.read().await;
                if let Some(id) = cache.get_path(&next_path) {
                    current_id = id;
                    current_path = next_path;
                    continue;
                }
                if let Some(id) = cache.get_child(current_id, part) {
                    drop(cache);
                    self.dir_cache.write().await.put_dir(&next_path, id);
                    current_id = id;
                    current_path = next_path;
                    continue;
                }
            }

            let files = self.list_and_cache(current_id).await?;
            if let Some(id) = files
                .iter()
                .find(|file| file.is_dir() && file.file_name == part)
                .map(|file| file.file_id)
            {
                current_id = id;
                current_path = next_path;
                continue;
            }

            if !create {
                return Ok(None);
            }

            let created_id = match self.pan123.mkdir(current_id, part).await {
                Ok(id) => id,
                Err(RequestError::AlreadyExists) => {
                    let files = self.list_and_cache(current_id).await?;
                    files
                        .iter()
                        .find(|file| file.is_dir() && file.file_name == part)
                        .map(|file| file.file_id)
                        .ok_or_else(|| {
                            RequestError::NotFound(format!(
                                "folder {part} not found in parent {current_id}"
                            ))
                        })?
                }
                Err(err) => return Err(err.into()),
            };
            self.dir_cache
                .write()
                .await
                .put_child(current_id, part, created_id);
            current_id = created_id;
            current_path = next_path;
        }

        Ok(Some(current_id))
    }

    async fn list_and_cache(&self, dir_id: i64) -> AppResult<Vec<pan123::File>> {
        let files = self.pan123.list(dir_id).await?;
        let dirs = files
            .iter()
            .filter(|file| file.is_dir())
            .map(|file| (file.file_name.clone(), file.file_id))
            .collect::<HashMap<_, _>>();
        self.dir_cache.write().await.put_children(dir_id, dirs);
        Ok(files)
    }
}

impl LibraryDirCache {
    fn new() -> Self {
        let mut cache = Self::default();
        cache.path_to_id.insert(String::new(), 0);
        cache.id_to_path.insert(0, String::new());
        cache
    }

    fn get_path(&self, path: &str) -> Option<i64> {
        self.path_to_id.get(path).copied()
    }

    fn get_child(&self, parent_id: i64, name: &str) -> Option<i64> {
        self.children.get(&parent_id)?.get(name).copied()
    }

    fn put_dir(&mut self, path: &str, id: i64) {
        if let Some(old_path) = self.id_to_path.insert(id, path.to_owned())
            && old_path != path
        {
            self.path_to_id.remove(&old_path);
        }
        self.path_to_id.insert(path.to_owned(), id);
    }

    fn put_child(&mut self, parent_id: i64, name: &str, id: i64) {
        self.children
            .entry(parent_id)
            .or_default()
            .insert(name.to_owned(), id);
        if let Some(parent_path) = self.id_to_path.get(&parent_id).cloned() {
            self.put_dir(&join_dir_path(&parent_path, name), id);
        }
    }

    fn put_children(&mut self, parent_id: i64, dirs: HashMap<String, i64>) {
        if let Some(old) = self.children.remove(&parent_id) {
            for (name, id) in old {
                if dirs.get(&name) != Some(&id) {
                    self.remove_ids(&[id]);
                }
            }
        }
        for (name, id) in &dirs {
            self.put_child(parent_id, name, *id);
        }
        self.children.insert(parent_id, dirs);
    }

    fn remove_ids(&mut self, ids: &[i64]) {
        let mut to_remove = ids.to_vec();
        let mut index = 0;
        while index < to_remove.len() {
            let id = to_remove[index];
            index += 1;
            if let Some(path) = self.id_to_path.remove(&id) {
                self.path_to_id.remove(&path);
                if !path.is_empty() {
                    let prefix = format!("{path}/");
                    let descendants = self
                        .path_to_id
                        .iter()
                        .filter(|(child_path, _)| child_path.starts_with(&prefix))
                        .map(|(_, child_id)| *child_id)
                        .collect::<Vec<_>>();
                    for child_id in descendants {
                        if !to_remove.contains(&child_id) {
                            to_remove.push(child_id);
                        }
                    }
                }
            }
            self.children.remove(&id);
            for children in self.children.values_mut() {
                children.retain(|_, child_id| *child_id != id);
            }
        }
    }
}

fn path_parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn normalize_dir_path(path: &str) -> String {
    let parts = path_parts(path);
    if parts.is_empty() {
        String::new()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn join_dir_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

const TMDB_EMPTY_TTL: Duration = Duration::from_secs(60 * 60);
const TMDB_SEARCH_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const TMDB_TV_DETAIL_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TMDB_MOVIE_DETAIL_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

fn tmdb_search_cache_key(kind: &str, title: &str, year: &str) -> String {
    let digest = Sha256::digest(format!("{year}\0{title}"));
    format!("tmdb:{kind}:search:{}", hex::encode(digest))
}

fn tmdb_movie_detail_cache_key(id: u32) -> String {
    format!("tmdb:movie:detail:{id}")
}

fn tmdb_tv_detail_cache_key(id: u32) -> String {
    format!("tmdb:tv:detail:{id}")
}

impl TmdbMetadataGateway {
    pub fn new(tmdb: tmdb::Client) -> Self {
        Self { tmdb, cache: None }
    }

    pub fn with_cache(self, cache: Cache) -> Self {
        Self {
            tmdb: self.tmdb,
            cache: Some(cache),
        }
    }

    async fn get_cached<V: DeserializeOwned + Send>(&self, key: &str) -> Option<V> {
        let cache = self.cache.as_ref()?;
        match cache.get::<V>(key).await {
            Ok(value) => value,
            Err(err) => {
                warn!(key, error = %err, "invalid tmdb cache entry, refetching");
                if let Err(delete_err) = cache.delete(key).await {
                    warn!(key, error = %delete_err, "failed to delete invalid tmdb cache entry");
                }
                None
            }
        }
    }

    async fn set_cached<V: Serialize + Send + Sync>(&self, key: &str, value: &V, ttl: Duration) {
        let Some(cache) = &self.cache else {
            return;
        };
        if let Err(err) = cache.set(key, value, Some(ttl)).await {
            warn!(key, error = %err, "failed to cache tmdb response");
        }
    }
}

impl From<tmdb::Genre> for Genre {
    fn from(value: tmdb::Genre) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

impl From<tmdb::Season> for Season {
    fn from(value: tmdb::Season) -> Self {
        Self {
            id: value.id,
            name: value.name,
            episode_count: value.episode_count,
            season_number: value.season_number,
        }
    }
}

impl From<tmdb::MovieDetail> for MovieDetail {
    fn from(value: tmdb::MovieDetail) -> Self {
        Self {
            id: value.id,
            title: value.title,
            adult: value.adult,
            genres: value.genres.into_iter().map(Into::into).collect(),
            original_language: value.original_language,
            original_title: value.original_title,
            origin_country: value.origin_country,
            release_date: value.release_date,
        }
    }
}

impl From<tmdb::SearchMovieResult> for SearchMovieResult {
    fn from(value: tmdb::SearchMovieResult) -> Self {
        Self {
            id: value.id,
            title: value.title,
            original_title: value.original_title,
            release_date: value.release_date,
            poster_path: value.poster_path,
            overview: value.overview,
        }
    }
}

impl From<tmdb::TvDetail> for TvDetail {
    fn from(value: tmdb::TvDetail) -> Self {
        Self {
            id: value.id,
            name: value.name,
            first_air_date: value.first_air_date,
            number_of_episodes: value.number_of_episodes,
            number_of_seasons: value.number_of_seasons,
            origin_country: value.origin_country,
            original_language: value.original_language,
            original_name: value.original_name,
            genres: value.genres.into_iter().map(Into::into).collect(),
            seasons: value.seasons.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<tmdb::SearchTvResult> for SearchTvResult {
    fn from(value: tmdb::SearchTvResult) -> Self {
        Self {
            id: value.id,
            name: value.name,
            original_name: value.original_name,
            first_air_date: value.first_air_date,
            poster_path: value.poster_path,
            overview: value.overview,
        }
    }
}

impl From<pan123::File> for LibraryFile {
    fn from(value: pan123::File) -> Self {
        let is_dir = value.is_dir();
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            is_dir,
            size: value.size,
            hash: value.etag,
        }
    }
}

fn map_download_url_error(err: RequestError) -> AppError {
    match err {
        RequestError::Unauthorized => {
            AppError::Unauthorized("download url source unauthorized".to_owned())
        }
        RequestError::NotFound(message) | RequestError::ShareCancelled(message) => {
            AppError::NotFound(message)
        }
        err => AppError::ExternalService(format!("failed to get download url: {err}"), false),
    }
}

#[async_trait::async_trait]
impl DownloadUrlSource for PanLibraryGateway {
    async fn get_download_url(&self, file_id: i64) -> AppResult<String> {
        self.pan123
            .get_download_url(file_id)
            .await
            .map_err(map_download_url_error)
    }
}

#[async_trait::async_trait]
impl LibraryGateway for PanLibraryGateway {
    async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
        Ok(self
            .list_and_cache(dir_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        self.resolve_dir(path, false).await
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<i64> {
        self.resolve_dir(path, true)
            .await?
            .ok_or_else(|| AppError::Internal("ensure_dir must create missing directories".into()))
    }

    async fn list_library_dir_ids(&self, dir_id: i64) -> AppResult<HashMap<String, i64>> {
        let dirs = self.pan123.list_dir_ids(dir_id).await?;
        self.dir_cache
            .write()
            .await
            .put_children(dir_id, dirs.clone());
        Ok(dirs)
    }

    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64> {
        let id = self.pan123.mkdir(parent_dir_id, folder_name).await?;
        self.dir_cache
            .write()
            .await
            .put_child(parent_dir_id, folder_name, id);
        Ok(id)
    }

    async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
        self.pan123.trash_files(file_ids).await?;
        self.dir_cache.write().await.remove_ids(file_ids);
        Ok(())
    }

    async fn upload(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &FileHash,
        size: u64,
    ) -> AppResult<Option<i64>> {
        match hash {
            FileHash::Md5(value) => Ok(self
                .pan123
                .fast_upload(parent_dir_id, file_name, value, size)
                .await?),
            FileHash::Sha1(value) => Ok(self
                .pan123
                .fast_upload_with_sha1(parent_dir_id, file_name, value, size)
                .await?),
        }
    }

    async fn download_library_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
        Ok(self.pan123.download_file(file_id, local_path).await?)
    }

    async fn search_media_dirs(&self, keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>> {
        Ok(self
            .pan123
            .search_dirs_with_paths(keyword)
            .await?
            .into_iter()
            .map(|record| MediaDirectoryRecord {
                dir_id: record.file_id,
                display_name: record.file_name,
                remote_path: record.path,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl MetadataCatalog for TmdbMetadataGateway {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
        let key = tmdb_search_cache_key("movie", title, year);
        if let Some(cached) = self.get_cached::<Vec<tmdb::SearchMovieResult>>(&key).await {
            return Ok(cached.into_iter().map(Into::into).collect());
        }

        let results = self.tmdb.search_movie(title, year).await?;
        let ttl = if results.is_empty() {
            TMDB_EMPTY_TTL
        } else {
            TMDB_SEARCH_TTL
        };
        self.set_cached(&key, &results, ttl).await;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        let key = tmdb_movie_detail_cache_key(id);
        if let Some(cached) = self.get_cached::<Option<tmdb::MovieDetail>>(&key).await {
            return Ok(cached.map(Into::into));
        }

        let detail = self.tmdb.get_movie_detail(id).await?;
        let ttl = if detail.is_some() {
            TMDB_MOVIE_DETAIL_TTL
        } else {
            TMDB_EMPTY_TTL
        };
        self.set_cached(&key, &detail, ttl).await;
        Ok(detail.map(Into::into))
    }

    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>> {
        let key = tmdb_search_cache_key("tv", title, year);
        if let Some(cached) = self.get_cached::<Vec<tmdb::SearchTvResult>>(&key).await {
            return Ok(cached.into_iter().map(Into::into).collect());
        }

        let results = self.tmdb.search_tv(title, year).await?;
        let ttl = if results.is_empty() {
            TMDB_EMPTY_TTL
        } else {
            TMDB_SEARCH_TTL
        };
        self.set_cached(&key, &results, ttl).await;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        let key = tmdb_tv_detail_cache_key(id);
        if let Some(cached) = self.get_cached::<Option<tmdb::TvDetail>>(&key).await {
            return Ok(cached.map(Into::into));
        }

        let detail = self.tmdb.get_tv_detail(id).await?;
        let ttl = if detail.is_some() {
            TMDB_TV_DETAIL_TTL
        } else {
            TMDB_EMPTY_TTL
        };
        self.set_cached(&key, &detail, ttl).await;
        Ok(detail.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::application::ports::{DownloadUrlSource, LibraryGateway};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param, query_param_is_missing},
    };

    fn dir_json(file_id: i64, name: &str, parent: i64) -> serde_json::Value {
        serde_json::json!({
            "fileId": file_id,
            "filename": name,
            "type": 1,
            "size": 0,
            "etag": format!("e{file_id}"),
            "parentFileId": parent,
            "trashed": 0
        })
    }

    fn list_template(last_file_id: i64, files: Vec<serde_json::Value>) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "message": "ok",
            "data": {
                "lastFileId": last_file_id,
                "fileList": files
            }
        }))
    }

    async fn mock_list_page(
        server: &MockServer,
        parent: i64,
        request_last: i64,
        response_last: i64,
        files: Vec<serde_json::Value>,
        expected: u64,
    ) {
        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", parent.to_string()))
            .and(query_param("lastFileId", request_last.to_string()))
            .and(query_param_is_missing("searchData"))
            .respond_with(list_template(response_last, files))
            .expect(expected)
            .mount(server)
            .await;
    }

    async fn mock_list_once(server: &MockServer, parent: i64, files: Vec<serde_json::Value>) {
        mock_list_page(server, parent, 0, -1, files, 1).await;
    }

    async fn mock_list_sequence(
        server: &MockServer,
        parent: i64,
        pages: Vec<Vec<serde_json::Value>>,
    ) {
        let templates = pages
            .into_iter()
            .map(|files| list_template(-1, files))
            .collect::<Vec<_>>();
        let expected = templates.len() as u64;
        let calls = AtomicUsize::new(0);
        Mock::given(method("GET"))
            .and(path("/api/v2/file/list"))
            .and(query_param("parentFileId", parent.to_string()))
            .and(query_param("lastFileId", "0"))
            .and(query_param_is_missing("searchData"))
            .respond_with(move |_: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                templates
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| ResponseTemplate::new(500))
            })
            .expect(expected)
            .mount(server)
            .await;
    }

    async fn mock_mkdir(server: &MockServer, dir_id: i64) {
        Mock::given(method("POST"))
            .and(path("/upload/v1/file/mkdir"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": { "dirID": dir_id }
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    async fn mock_mkdir_already_exists(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/upload/v1/file/mkdir"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 1,
                "message": "exists"
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    async fn mock_trash(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/v1/file/trash"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {}
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    fn unique_cache_dir() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("bigbrother-library-gateway-{nanos}"))
            .display()
            .to_string()
    }

    async fn gateway(server: &MockServer) -> PanLibraryGateway {
        let client = pan123::Client::with_open_api_base(
            "test-user",
            "test-pass",
            &server.uri(),
            &unique_cache_dir(),
            server.uri().as_str(),
        );
        client
            .set_token_for_test(
                "test-token",
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            )
            .await;
        PanLibraryGateway::new(client)
    }

    #[test]
    fn map_download_url_error_preserves_expected_variants() {
        assert!(matches!(
            map_download_url_error(RequestError::Unauthorized),
            AppError::Unauthorized(_)
        ));
        assert!(matches!(
            map_download_url_error(RequestError::NotFound("missing".to_string())),
            AppError::NotFound(message) if message == "missing"
        ));
        assert!(matches!(
            map_download_url_error(RequestError::ShareCancelled("cancelled".to_string())),
            AppError::NotFound(message) if message == "cancelled"
        ));
        assert!(matches!(
            map_download_url_error(RequestError::TooManyRequests),
            AppError::ExternalService(message, false) if message.contains("too many requests")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::ShareAuditNotPass),
            AppError::ExternalService(message, false) if message.contains("share audit not pass")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::BadRequest("bad".to_string())),
            AppError::ExternalService(message, false) if message.contains("bad")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::ConnectError("conn".to_string())),
            AppError::ExternalService(message, false) if message.contains("conn")
        ));
        assert!(matches!(
            map_download_url_error(RequestError::Timeout("timeout".to_string())),
            AppError::ExternalService(message, false) if message.contains("timeout")
        ));
    }

    #[tokio::test]
    async fn get_download_url_maps_empty_url_error() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v1/file/download_info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "message": "ok",
                "data": {
                    "downloadUrl": ""
                }
            })))
            .mount(&server)
            .await;

        let error = DownloadUrlSource::get_download_url(&gateway, 99)
            .await
            .unwrap_err();

        assert!(
            matches!(error, AppError::ExternalService(message, false) if message.contains("empty download url"))
        );
    }

    #[tokio::test]
    async fn ensure_dir_walks_existing_path_without_search_and_caches_it() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_once(&server, 1, vec![dir_json(2, "电影", 1)]).await;
        mock_list_once(&server, 2, vec![dir_json(3, "华语", 2)]).await;
        mock_list_once(&server, 3, vec![dir_json(4, "A", 3)]).await;

        let first = gateway.ensure_dir("/remote/电影/华语/A").await.unwrap();
        let second = gateway.ensure_dir("/remote/电影/华语/A").await.unwrap();

        assert_eq!(first, 4);
        assert_eq!(second, 4);
    }

    #[tokio::test]
    async fn ensure_dir_reuses_cached_prefix_for_a_new_leaf() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_once(&server, 1, vec![dir_json(2, "电影", 1)]).await;
        mock_list_once(&server, 2, vec![dir_json(3, "华语", 2)]).await;
        mock_list_sequence(
            &server,
            3,
            vec![vec![dir_json(4, "A", 3)], vec![dir_json(5, "B", 3)]],
        )
        .await;

        let first = gateway.ensure_dir("/remote/电影/华语/A").await.unwrap();
        let second = gateway.ensure_dir("/remote/电影/华语/B").await.unwrap();

        assert_eq!(first, 4);
        assert_eq!(second, 5);
    }

    #[tokio::test]
    async fn ensure_dir_mkdirs_missing_leaf() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_once(&server, 1, vec![dir_json(2, "电影", 1)]).await;
        mock_list_once(&server, 2, vec![dir_json(3, "华语", 2)]).await;
        mock_list_once(&server, 3, vec![]).await;
        mock_mkdir(&server, 4).await;

        let id = gateway.ensure_dir("/remote/电影/华语/X").await.unwrap();

        assert_eq!(id, 4);
    }

    #[tokio::test]
    async fn ensure_dir_lists_again_when_mkdir_already_exists() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_sequence(&server, 1, vec![vec![], vec![dir_json(2, "电影", 1)]]).await;
        mock_mkdir_already_exists(&server).await;

        let id = gateway.ensure_dir("/remote/电影").await.unwrap();

        assert_eq!(id, 2);
    }

    #[tokio::test]
    async fn get_library_dir_id_by_path_uses_ensure_dir_cache() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        let cloned = gateway.clone();
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_once(&server, 1, vec![dir_json(2, "电影", 1)]).await;

        let movie_id = gateway.ensure_dir("/remote/电影").await.unwrap();
        let remote_id = cloned.get_library_dir_id_by_path("/remote").await.unwrap();

        assert_eq!(movie_id, 2);
        assert_eq!(remote_id, Some(1));
    }

    #[tokio::test]
    async fn list_library_files_seeds_children_for_ensure_dir() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_once(&server, 1, vec![dir_json(2, "电影", 1)]).await;
        mock_list_once(&server, 2, vec![dir_json(3, "华语", 2)]).await;
        mock_list_once(&server, 3, vec![]).await;
        mock_mkdir(&server, 4).await;

        let root_id = gateway
            .get_library_dir_id_by_path("/remote")
            .await
            .unwrap()
            .unwrap();
        gateway.list_library_files(root_id).await.unwrap();
        let id = gateway.ensure_dir("/remote/电影/华语/X").await.unwrap();

        assert_eq!(root_id, 1);
        assert_eq!(id, 4);
    }

    #[tokio::test]
    async fn ensure_dir_finds_dir_from_second_list_page() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_page(&server, 0, 0, 50, vec![dir_json(1, "Movie1", 0)], 1).await;
        mock_list_page(&server, 0, 50, -1, vec![dir_json(101, "Movie101", 0)], 1).await;

        let id = gateway.ensure_dir("/Movie101").await.unwrap();

        assert_eq!(id, 101);
    }

    #[tokio::test]
    async fn trash_invalidates_cached_dir_id() {
        let server = MockServer::start().await;
        let gateway = gateway(&server).await;
        mock_list_once(&server, 0, vec![dir_json(1, "remote", 0)]).await;
        mock_list_sequence(&server, 1, vec![vec![dir_json(2, "电影", 1)], vec![]]).await;
        mock_trash(&server).await;
        mock_mkdir(&server, 20).await;

        let first = gateway.ensure_dir("/remote/电影").await.unwrap();
        gateway.trash_library_files(&[2]).await.unwrap();
        let second = gateway.ensure_dir("/remote/电影").await.unwrap();

        assert_eq!(first, 2);
        assert_eq!(second, 20);
    }
}

#[cfg(test)]
#[path = "tmdb_cache_tests.rs"]
mod tmdb_cache_tests;
