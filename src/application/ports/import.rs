use std::{collections::HashMap, sync::Arc};

use crate::{
    domain::{
        import::{LibraryFile, MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
        media::Title,
        share::FileHash,
    },
    error::AppResult,
};

use super::MediaDirectoryRecord;

#[async_trait::async_trait]
pub trait LibraryGateway: Send + Sync {
    async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>>;
    async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>>;
    async fn ensure_dir(&self, path: &str) -> AppResult<i64>;
    async fn list_library_dir_ids(&self, dir_id: i64) -> AppResult<HashMap<String, i64>>;
    async fn mkdir_library_dir(&self, parent_dir_id: i64, folder_name: &str) -> AppResult<i64>;
    async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()>;
    async fn upload(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        hash: &FileHash,
        size: u64,
    ) -> AppResult<Option<i64>>;
    async fn download_library_file(&self, file_id: i64, local_path: &str) -> AppResult<()>;
    async fn search_media_dirs(&self, keyword: &str) -> AppResult<Vec<MediaDirectoryRecord>>;
}

pub type LibraryGatewayHandle = Arc<dyn LibraryGateway>;

#[async_trait::async_trait]
pub trait MetadataCatalog: Send + Sync {
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>>;
    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>>;
    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>>;
    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>>;
}

pub type MetadataCatalogHandle = Arc<dyn MetadataCatalog>;

#[async_trait::async_trait]
pub trait TitleExtractor: Send + Sync {
    async fn extract_title(&self, description: &str) -> AppResult<Option<Title>>;
}

pub type TitleExtractorHandle = Arc<dyn TitleExtractor>;
