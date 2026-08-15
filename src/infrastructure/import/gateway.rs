use std::collections::HashMap;

use crate::{
    application::ports::{
        LibraryGateway, MediaDirectoryRecord, MediaSearchSource, MetadataCatalog,
    },
    domain::import::{
        Genre, LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, Season, TvDetail,
    },
    error::AppResult,
    infrastructure::client::{pan123, tmdb},
};

#[derive(Clone)]
pub struct PanLibraryGateway {
    pan123: pan123::Client,
}

#[derive(Clone)]
pub struct TmdbMetadataGateway {
    tmdb: tmdb::Client,
}

#[derive(Clone)]
pub struct Pan123MediaSearchGateway {
    pan123: pan123::Client,
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

impl Pan123MediaSearchGateway {
    pub fn new(pan123: pan123::Client) -> Self {
        Self { pan123 }
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

    async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
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

    async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        Ok(self
            .pan123
            .fast_upload(parent_dir_id, file_name, hash, size)
            .await?)
    }

    async fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        Ok(self
            .pan123
            .fast_upload_with_sha1(parent_dir_id, file_name, sha1, size)
            .await?)
    }

    async fn download_library_file(&self, file_id: i64, local_path: &str) -> AppResult<()> {
        Ok(self.pan123.download_file(file_id, local_path).await?)
    }
}

#[async_trait::async_trait]
impl MediaSearchSource for Pan123MediaSearchGateway {
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
