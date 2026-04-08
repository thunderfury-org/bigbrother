use std::{collections::HashMap, io, path::Path};

use crate::{
    domain::library::path_mapping::SyncPathMapper,
    error::{AppError, AppResult},
};

use super::{
    ImportPathConfig, LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder,
    Pan189ShareInfo, SearchMovieResult, SearchTvResult, TvDetail,
};

pub(crate) trait ImportClient: Clone {
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
        etag: &str,
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
    async fn list_pan123_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<LibraryFile>>;
    async fn get_pan189_share_info(&self, share_code: &str) -> AppResult<Pan189ShareInfo>;
    async fn list_pan189_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)>;
    async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>>;
    async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>>;
    async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>>;
    async fn search_tv(&self, title: &str, year: &str) -> AppResult<Vec<SearchTvResult>>;
    async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>>;
}

#[derive(Clone)]
pub(super) struct ImportRemote<C> {
    client: C,
    paths: ImportPathConfig,
    path_mapper: SyncPathMapper,
}

impl<C> ImportRemote<C>
where
    C: ImportClient,
{
    pub(super) fn new(client: C, paths: ImportPathConfig) -> Self {
        let path_mapper = SyncPathMapper::new(paths.remote_path.clone(), paths.local_path.clone());

        Self {
            client,
            paths,
            path_mapper,
        }
    }

    pub(super) async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
        self.client.list_library_files(dir_id).await
    }

    pub(super) async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        self.client.get_library_dir_id_by_path(path).await
    }

    pub(super) async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
        self.client.mkdir_library_path(path).await
    }

    pub(super) async fn list_library_dir_ids(
        &self,
        dir_id: i64,
    ) -> AppResult<HashMap<String, i64>> {
        self.client.list_library_dir_ids(dir_id).await
    }

    pub(super) async fn mkdir_library_dir(
        &self,
        parent_dir_id: i64,
        folder_name: &str,
    ) -> AppResult<i64> {
        self.client
            .mkdir_library_dir(parent_dir_id, folder_name)
            .await
    }

    pub(super) async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
        self.client.trash_library_files(file_ids).await
    }

    pub(super) async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        self.client
            .fast_upload_md5(parent_dir_id, file_name, etag, size)
            .await
    }

    pub(super) async fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        self.client
            .fast_upload_sha1(parent_dir_id, file_name, sha1, size)
            .await
    }

    pub(super) async fn download_library_file(
        &self,
        file_id: i64,
        local_path: &str,
    ) -> AppResult<()> {
        self.client.download_library_file(file_id, local_path).await
    }

    pub(super) async fn list_pan123_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<LibraryFile>> {
        self.client
            .list_pan123_share_files(share_key, share_password, parent_id)
            .await
    }

    pub(super) async fn get_pan189_share_info(
        &self,
        share_code: &str,
    ) -> AppResult<Pan189ShareInfo> {
        self.client.get_pan189_share_info(share_code).await
    }

    pub(super) async fn list_pan189_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
        self.client
            .list_pan189_share_files(share_id, share_mode, parent_id)
            .await
    }

    pub(super) async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>> {
        self.client
            .list_pan115_share_files(share_code, receive_code, cid)
            .await
    }

    pub(super) async fn search_movie(
        &self,
        title: &str,
        year: &str,
    ) -> AppResult<Vec<SearchMovieResult>> {
        self.client.search_movie(title, year).await
    }

    pub(super) async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
        self.client.get_movie_detail(id).await
    }

    pub(super) async fn search_tv(
        &self,
        title: &str,
        year: &str,
    ) -> AppResult<Vec<SearchTvResult>> {
        self.client.search_tv(title, year).await
    }

    pub(super) async fn get_tv_detail(&self, id: u32) -> AppResult<Option<TvDetail>> {
        self.client.get_tv_detail(id).await
    }

    pub(super) fn library_remote_path(&self) -> &str {
        self.paths.remote_path.as_str()
    }

    pub(super) fn local_path_for_remote(&self, remote_path: &str) -> String {
        self.path_mapper.remote_to_local_path(remote_path)
    }

    pub(super) fn build_strm_url(&self, remote_file_path: &str, file_id: i64) -> String {
        format!(
            "{}{}?file_id={}",
            self.paths.strm_download_url, remote_file_path, file_id
        )
    }

    pub(super) fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.path_mapper
            .remote_to_local_strm_path(remote_file_path, extension)
    }

    pub(super) async fn write_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()> {
        let local_file_path = self.local_strm_path(remote_file_path, extension);
        let strm_file_content = self.build_strm_url(remote_file_path, file_id);

        tokio::fs::create_dir_all(Path::new(&local_file_path).parent().unwrap()).await?;
        tokio::fs::write(local_file_path.as_str(), strm_file_content.as_bytes()).await?;
        Ok(())
    }

    pub(super) async fn remove_local_file_if_exists(&self, path: &str) -> AppResult<()> {
        if let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(AppError::Internal(format!(
                "Failed to delete local file, error: {}",
                err
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct FakeImportClient;

    impl ImportClient for FakeImportClient {
        async fn list_library_files(&self, _dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            unreachable!()
        }

        async fn get_library_dir_id_by_path(&self, _path: &str) -> AppResult<Option<i64>> {
            unreachable!()
        }

        async fn mkdir_library_path(&self, _path: &str) -> AppResult<i64> {
            unreachable!()
        }

        async fn list_library_dir_ids(&self, _dir_id: i64) -> AppResult<HashMap<String, i64>> {
            unreachable!()
        }

        async fn mkdir_library_dir(
            &self,
            _parent_dir_id: i64,
            _folder_name: &str,
        ) -> AppResult<i64> {
            unreachable!()
        }

        async fn trash_library_files(&self, _file_ids: &[i64]) -> AppResult<()> {
            unreachable!()
        }

        async fn fast_upload_md5(
            &self,
            _parent_dir_id: i64,
            _file_name: &str,
            _etag: &str,
            _size: u64,
        ) -> AppResult<Option<i64>> {
            unreachable!()
        }

        async fn fast_upload_sha1(
            &self,
            _parent_dir_id: i64,
            _file_name: &str,
            _sha1: &str,
            _size: u64,
        ) -> AppResult<Option<i64>> {
            unreachable!()
        }

        async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
            unreachable!()
        }

        async fn list_pan123_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            _parent_id: i64,
        ) -> AppResult<Vec<LibraryFile>> {
            unreachable!()
        }

        async fn get_pan189_share_info(&self, _share_code: &str) -> AppResult<Pan189ShareInfo> {
            unreachable!()
        }

        async fn list_pan189_share_files(
            &self,
            _share_id: i64,
            _share_mode: i32,
            _parent_id: &str,
        ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
            unreachable!()
        }

        async fn list_pan115_share_files(
            &self,
            _share_code: &str,
            _receive_code: &str,
            _cid: &str,
        ) -> AppResult<Vec<Pan115FileEntry>> {
            unreachable!()
        }

        async fn search_movie(
            &self,
            _title: &str,
            _year: &str,
        ) -> AppResult<Vec<SearchMovieResult>> {
            unreachable!()
        }

        async fn get_movie_detail(&self, _id: u32) -> AppResult<Option<MovieDetail>> {
            unreachable!()
        }

        async fn search_tv(&self, _title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
            unreachable!()
        }

        async fn get_tv_detail(&self, _id: u32) -> AppResult<Option<TvDetail>> {
            unreachable!()
        }
    }

    fn import_remote() -> ImportRemote<FakeImportClient> {
        ImportRemote::new(
            FakeImportClient,
            ImportPathConfig::new(
                "/remote".to_string(),
                "/local".to_string(),
                "http://localhost/d".to_string(),
            ),
        )
    }

    #[test]
    fn local_path_rewrites_remote_prefix() {
        let remote = import_remote();

        let local = remote.local_path_for_remote("/remote/show/ep01.mkv");

        assert_eq!(local, "/local/show/ep01.mkv");
    }

    #[test]
    fn build_strm_url_uses_configured_prefix() {
        let remote = import_remote();

        let url = remote.build_strm_url("/remote/show/ep01.mkv", 42);

        assert_eq!(url, "http://localhost/d/remote/show/ep01.mkv?file_id=42");
    }
}
