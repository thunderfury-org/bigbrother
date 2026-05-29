use std::collections::HashMap;

use crate::{
    application::import::{LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
    domain::media::Title,
    error::AppResult,
};

pub trait LibraryGateway: Clone {
    async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>>;
    async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>>;
    async fn mkdir_library_path(&self, path: &str) -> AppResult<i64>;
    async fn list_library_dir_ids(&self, dir_id: i64) -> AppResult<HashMap<String, i64>>;
    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64>;
    async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()>;
    async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &str,
        size: u64,
    ) -> AppResult<Option<i64>>;
    async fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> AppResult<Option<i64>>;
    async fn download_library_file(&self, file_id: i64, local_path: &str) -> AppResult<()>;
}

pub trait MetadataCatalog: Clone {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>>;
    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>>;
    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>>;
    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>>;
}

pub trait TitleExtractor: Clone {
    async fn extract_title(&self, description: &str) -> AppResult<Option<Title>>;
}

pub trait ImportLocalStore: Clone {
    fn remote_library_path(&self) -> &str;
    fn local_path_for_remote(&self, remote_path: &str) -> String;
    fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String;
    async fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()>;
    async fn remove_local_file_if_exists(&self, path: &str) -> AppResult<()>;
    async fn remove_local_dir_if_exists(&self, path: &str) -> AppResult<()>;
}
