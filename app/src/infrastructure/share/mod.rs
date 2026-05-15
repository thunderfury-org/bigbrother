mod collect;
pub mod file_parser;
pub mod pan115;
pub mod pan123;
pub mod pan189;
pub mod quark;
pub mod resolver;
mod traversal;

use std::collections::HashMap;

use crate::{
    application::import::{
        LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo, QuarkFile,
        QuarkFolder, QuarkShareInfo,
    },
    error::AppResult,
};

pub(crate) trait ShareClient: Clone {
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
    async fn download_pan189_share_file(
        &self,
        share_id: i64,
        file: &Pan189File,
    ) -> AppResult<Vec<u8>>;
    async fn list_pan115_share_files(
        &self,
        share_code: &str,
        receive_code: &str,
        cid: &str,
    ) -> AppResult<Vec<Pan115FileEntry>>;
    async fn get_quark_share_info(
        &self,
        share_id: &str,
        password: &str,
    ) -> AppResult<QuarkShareInfo>;
    async fn list_quark_share_files(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> AppResult<(Vec<QuarkFolder>, Vec<QuarkFile>)>;
    async fn batch_get_quark_file_md5s(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        file_infos: &[(String, String)],
    ) -> AppResult<HashMap<String, String>>;
}
