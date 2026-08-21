use std::collections::HashMap;

use crate::{
    application::ports::{
        DownloadUrlSource, LibraryGateway, MediaDirectoryRecord, MetadataCatalog,
    },
    domain::import::{
        Genre, LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, Season, TvDetail,
    },
    domain::share::FileHash,
    error::{AppError, AppResult},
    infrastructure::client::{RequestError, pan123, tmdb},
};

#[derive(Clone)]
pub struct PanLibraryGateway {
    pan123: pan123::Client,
}

#[derive(Clone)]
pub struct TmdbMetadataGateway {
    tmdb: tmdb::Client,
}

impl PanLibraryGateway {
    pub fn new(pan123: pan123::Client) -> Self {
        Self { pan123 }
    }
}

impl TmdbMetadataGateway {
    pub fn new(tmdb: tmdb::Client) -> Self {
        Self { tmdb }
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
            .pan123
            .list(dir_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        Ok(self.pan123.get_file_id_by_path(path).await?)
    }

    async fn ensure_dir(&self, path: &str) -> AppResult<i64> {
        if let Some(id) = self.pan123.get_file_id_by_path(path).await? {
            return Ok(id);
        }
        Ok(self.pan123.mkdir_by_path(path).await?)
    }

    async fn list_library_dir_ids(&self, dir_id: i64) -> AppResult<HashMap<String, i64>> {
        Ok(self.pan123.list_dir_ids(dir_id).await?)
    }

    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64> {
        Ok(self.pan123.mkdir(parent_dir_id, folder_name).await?)
    }

    async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
        Ok(self.pan123.trash_files(file_ids).await?)
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
        Ok(self
            .tmdb
            .search_movie(title, year)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        Ok(self.tmdb.get_movie_detail(id).await?.map(Into::into))
    }

    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>> {
        Ok(self
            .tmdb
            .search_tv(title, year)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        Ok(self.tmdb.get_tv_detail(id).await?.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::application::ports::DownloadUrlSource;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

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
            &format!("{}/refresh", server.uri()),
            "refresh-token",
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
}
