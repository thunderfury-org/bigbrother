use std::{io, path::Path};

use crate::{
    client::{
        RequestResult, pan115, pan123, pan189,
        tmdb::{MovieDetail, SearchMovieResult, SearchTvResult, TvDetail},
    },
    error::{AppError, AppResult},
};

use super::ImportContext;

#[derive(Clone)]
pub(super) struct ImportRemote {
    ctx: ImportContext,
}

impl ImportRemote {
    pub(super) fn new(ctx: ImportContext) -> Self {
        Self { ctx }
    }

    pub(super) async fn list_library_files(&self, dir_id: i64) -> RequestResult<Vec<pan123::File>> {
        self.ctx.clients.pan123.list(dir_id).await
    }

    pub(super) async fn get_library_dir_id_by_path(
        &self,
        path: &str,
    ) -> RequestResult<Option<i64>> {
        self.ctx.clients.pan123.get_file_id_by_path(path).await
    }

    pub(super) async fn mkdir_library_path(&self, path: &str) -> RequestResult<i64> {
        self.ctx.clients.pan123.mkdir_by_path(path).await
    }

    pub(super) async fn list_library_dir_ids(
        &self,
        dir_id: i64,
    ) -> RequestResult<std::collections::HashMap<String, i64>> {
        self.ctx.clients.pan123.list_dir_ids(dir_id).await
    }

    pub(super) async fn mkdir_library_dir(
        &self,
        parent_dir_id: i64,
        folder_name: &str,
    ) -> RequestResult<i64> {
        self.ctx
            .clients
            .pan123
            .mkdir(parent_dir_id, folder_name)
            .await
    }

    pub(super) async fn trash_library_files(&self, file_ids: &[i64]) -> RequestResult<()> {
        self.ctx.clients.pan123.trash_files(file_ids).await
    }

    pub(super) async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.ctx
            .clients
            .pan123
            .fast_upload(parent_dir_id, file_name, etag, size)
            .await
    }

    pub(super) async fn fast_upload_sha1(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        sha1: &str,
        size: u64,
    ) -> RequestResult<Option<i64>> {
        self.ctx
            .clients
            .pan123
            .fast_upload_with_sha1(parent_dir_id, file_name, sha1, size)
            .await
    }

    pub(super) async fn download_library_file(
        &self,
        file_id: i64,
        local_path: &str,
    ) -> RequestResult<()> {
        self.ctx
            .clients
            .pan123
            .download_file(file_id, local_path)
            .await
    }

    pub(super) async fn list_pan123_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> RequestResult<Vec<pan123::File>> {
        self.ctx
            .clients
            .pan123
            .list_share_files(share_key, share_password, parent_id)
            .await
    }

    pub(super) async fn get_pan189_share_info(
        &self,
        share_code: &str,
    ) -> RequestResult<pan189::ShareInfo> {
        self.ctx.clients.pan189.get_share_info(share_code).await
    }

    pub(super) async fn list_pan189_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        parent_id: &str,
    ) -> RequestResult<(Vec<pan189::Folder>, Vec<pan189::File>)> {
        self.ctx
            .clients
            .pan189
            .list_share_files(share_id, share_mode, parent_id)
            .await
    }

    pub(super) async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> RequestResult<Vec<pan115::FileEntry>> {
        self.ctx
            .clients
            .pan115
            .list_share_files(share_code, receive_code, cid)
            .await
    }

    pub(super) async fn search_movie(
        &self,
        title: &str,
        year: &str,
    ) -> RequestResult<Vec<SearchMovieResult>> {
        self.ctx.clients.tmdb.search_movie(title, year).await
    }

    pub(super) async fn get_movie_detail(&self, id: u32) -> RequestResult<Option<MovieDetail>> {
        self.ctx.clients.tmdb.get_movie_detail(id).await
    }

    pub(super) async fn search_tv(
        &self,
        title: &str,
        year: &str,
    ) -> RequestResult<Vec<SearchTvResult>> {
        self.ctx.clients.tmdb.search_tv(title, year).await
    }

    pub(super) async fn get_tv_detail(&self, id: u32) -> RequestResult<Option<TvDetail>> {
        self.ctx.clients.tmdb.get_tv_detail(id).await
    }

    pub(super) fn library_remote_path(&self) -> &str {
        self.ctx.paths.remote_path.as_str()
    }

    pub(super) fn local_path_for_remote(&self, remote_path: &str) -> String {
        remote_path.replace(
            self.ctx.paths.remote_path.as_str(),
            self.ctx.paths.local_path.as_str(),
        )
    }

    pub(super) fn build_strm_url(&self, remote_file_path: &str, file_id: i64) -> String {
        format!(
            "{}{}?file_id={}",
            self.ctx.paths.strm_download_url, remote_file_path, file_id
        )
    }

    pub(super) fn local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.local_path_for_remote(remote_file_path)
            .trim_end_matches(extension)
            .to_owned()
            + ".strm"
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

    fn import_remote() -> ImportRemote {
        ImportRemote::new(ImportContext::new(
            pan115::Client::new(),
            pan123::Client::new("", "", "/tmp/pan123"),
            pan189::Client::new(),
            crate::client::tmdb::Client::new(""),
            "/remote".to_string(),
            "/local".to_string(),
            "http://localhost/d".to_string(),
        ))
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
