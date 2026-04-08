use std::collections::HashMap;

use crate::{
    application::import_ports::{LibraryGateway, ShareSource},
    error::AppResult,
};

use super::{
    ImportPathConfig, LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo,
};

#[derive(Clone)]
pub(super) struct LibraryRemote<L> {
    gateway: L,
    paths: ImportPathConfig,
}

impl<L> LibraryRemote<L>
where
    L: LibraryGateway,
{
    pub(super) fn new(gateway: L, paths: ImportPathConfig) -> Self {
        Self { gateway, paths }
    }

    pub(super) async fn list_library_files(&self, dir_id: i64) -> AppResult<Vec<LibraryFile>> {
        self.gateway.list_library_files(dir_id).await
    }

    pub(super) async fn get_library_dir_id_by_path(&self, path: &str) -> AppResult<Option<i64>> {
        self.gateway.get_library_dir_id_by_path(path).await
    }

    pub(super) async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
        self.gateway.mkdir_library_path(path).await
    }

    pub(super) async fn list_library_dir_ids(
        &self,
        dir_id: i64,
    ) -> AppResult<HashMap<String, i64>> {
        self.gateway.list_library_dir_ids(dir_id).await
    }

    pub(super) async fn mkdir_library_dir(
        &self,
        parent_dir_id: i64,
        folder_name: &str,
    ) -> AppResult<i64> {
        self.gateway
            .mkdir_library_dir(parent_dir_id, folder_name)
            .await
    }

    pub(super) async fn trash_library_files(&self, file_ids: &[i64]) -> AppResult<()> {
        self.gateway.trash_library_files(file_ids).await
    }

    pub(super) async fn fast_upload_md5(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        etag: &str,
        size: u64,
    ) -> AppResult<Option<i64>> {
        self.gateway
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
        self.gateway
            .fast_upload_sha1(parent_dir_id, file_name, sha1, size)
            .await
    }

    pub(super) async fn download_library_file(
        &self,
        file_id: i64,
        local_path: &str,
    ) -> AppResult<()> {
        self.gateway
            .download_library_file(file_id, local_path)
            .await
    }

    pub(super) fn library_remote_path(&self) -> &str {
        self.paths.remote_path.as_str()
    }
}

#[derive(Clone)]
pub(super) struct ShareRemote<S> {
    source: S,
}

impl<S> ShareRemote<S>
where
    S: ShareSource,
{
    pub(super) fn new(source: S) -> Self {
        Self { source }
    }

    pub(super) async fn list_pan123_share_files(
        &self,
        share_key: &str,
        share_password: &str,
        parent_id: i64,
    ) -> AppResult<Vec<LibraryFile>> {
        self.source
            .list_pan123_share_files(share_key, share_password, parent_id)
            .await
    }

    pub(super) async fn get_pan189_share_info(
        &self,
        share_code: &str,
    ) -> AppResult<Pan189ShareInfo> {
        self.source.get_pan189_share_info(share_code).await
    }

    pub(super) async fn list_pan189_share_files(
        &self,
        share_id: i64,
        share_mode: i32,
        parent_id: &str,
    ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
        self.source
            .list_pan189_share_files(share_id, share_mode, parent_id)
            .await
    }

    pub(super) async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>> {
        self.source
            .list_pan115_share_files(share_code, receive_code, cid)
            .await
    }
}
