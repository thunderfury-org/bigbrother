use std::collections::HashMap;

use crate::{
    application::import::{
        Genre, LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder,
        Pan189ShareInfo, SearchMovieResult, SearchTvResult, Season, TvDetail,
    },
    application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource},
    error::AppResult,
    infrastructure::client::{pan115, pan123, pan189, tmdb},
};

#[derive(Clone)]
pub struct PanLibraryGateway {
    pan123: pan123::Client,
}

#[derive(Clone)]
pub struct ShareImportGateway {
    pan115: pan115::Client,
    pan123: pan123::Client,
    pan189: pan189::Client,
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

impl ShareImportGateway {
    pub fn new(pan115: pan115::Client, pan123: pan123::Client, pan189: pan189::Client) -> Self {
        Self {
            pan115,
            pan123,
            pan189,
        }
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
            etag: value.etag,
        }
    }
}

impl From<pan189::ShareInfo> for Pan189ShareInfo {
    fn from(value: pan189::ShareInfo) -> Self {
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            share_id: value.share_id,
            share_mode: value.share_mode,
        }
    }
}

impl From<pan189::Folder> for Pan189Folder {
    fn from(value: pan189::Folder) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

impl From<pan189::File> for Pan189File {
    fn from(value: pan189::File) -> Self {
        Self {
            name: value.name,
            size: value.size,
            md5: value.md5,
        }
    }
}

impl From<pan115::FileEntry> for Pan115FileEntry {
    fn from(value: pan115::FileEntry) -> Self {
        let is_file = value.is_file();
        Self {
            fid: if is_file { value.fid } else { None },
            cid: value.cid,
            name: value.name,
            size: value.size,
            sha: value.sha,
        }
    }
}

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
        etag: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        Ok(self
            .pan123
            .fast_upload(parent_dir_id, file_name, etag, size)
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

impl ShareSource for ShareImportGateway {
    async fn list_pan123_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<LibraryFile>> {
        Ok(self
            .pan123
            .list_share_files(share_key, share_password, parent_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn get_pan189_share_info(&self, share_code: &str) -> AppResult<Pan189ShareInfo> {
        Ok(self.pan189.get_share_info(share_code).await?.into())
    }

    async fn list_pan189_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
        let (folders, files) = self
            .pan189
            .list_share_files(share_id, share_mode, parent_id)
            .await?;
        Ok((
            folders.into_iter().map(Into::into).collect(),
            files.into_iter().map(Into::into).collect(),
        ))
    }

    async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>> {
        Ok(self
            .pan115
            .list_share_files(share_code, receive_code, cid)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

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
