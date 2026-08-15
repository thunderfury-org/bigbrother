use std::collections::HashMap;

use crate::{
    domain::{
        import::{LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
        media::Title,
    },
    error::AppResult,
};

pub trait LibraryGateway {
    fn list_library_files(
        &self,
        dir_id: i64,
    ) -> impl std::future::Future<Output = AppResult<Vec<LibraryFile>>> + Send;
    fn get_library_dir_id_by_path(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<i64>>> + Send;
    fn mkdir_library_path(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<i64>> + Send;
    fn list_library_dir_ids(
        &self,
        dir_id: i64,
    ) -> impl std::future::Future<Output = AppResult<HashMap<String, i64>>> + Send;
    fn mkdir_library_dir(
        &self,
        parent_dir_id: i64,
        folder_name: &str,
    ) -> impl std::future::Future<Output = AppResult<i64>> + Send;
    fn trash_library_files(
        &self,
        file_ids: &[i64],
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &str,
        size: u64,
    ) -> impl std::future::Future<Output = AppResult<Option<i64>>> + Send;
    fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> impl std::future::Future<Output = AppResult<Option<i64>>> + Send;
    fn download_library_file(
        &self,
        file_id: i64,
        local_path: &str,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}

pub trait MetadataCatalog {
    fn search_movie(
        &self,
        title: &str,
        year: &str,
    ) -> impl std::future::Future<Output = AppResult<Vec<SearchMovieResult>>> + Send;
    fn get_movie_detail(
        &self,
        id: u32,
    ) -> impl std::future::Future<Output = AppResult<Option<MovieDetail>>> + Send;
    fn search_tv(
        &self,
        title: &str,
        year: &str,
    ) -> impl std::future::Future<Output = AppResult<Vec<SearchTvResult>>> + Send;
    fn get_tv_detail(
        &self,
        id: u32,
    ) -> impl std::future::Future<Output = AppResult<Option<TvDetail>>> + Send;
}

pub trait TitleExtractor {
    fn extract_title(
        &self,
        description: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<Title>>> + Send;
}

pub trait ImportLocalStore {
    fn remote_library_path(&self) -> &str;
    fn local_path_for_remote(&self, remote_path: &str) -> String;
    fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String;
    fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn remove_local_file_if_exists(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
    fn remove_local_dir_if_exists(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = AppResult<()>> + Send;
}
