use url::Url;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
};

use super::{
    ShareClient,
    collect::collect_quark_directory_entries,
    traversal::ShareTraversal,
    url::{ShareUrl, parse_quark_share_parts, parse_share_url},
};

#[derive(Clone)]
pub struct QuarkShareService<S> {
    share_source: S,
}

impl<S: ShareClient> QuarkShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
        let Some(ShareUrl::Quark(url)) = parse_share_url(url) else {
            return Err(AppError::InvalidParameter(format!(
                "unsupported quark share url: {url}"
            )));
        };
        let (share_id, password) = parse_quark_share_parts(url);
        if share_id.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share id from URL: {url}"
            )));
        }

        let share_info = self
            .share_source
            .get_quark_share_info(&share_id, &password)
            .await?;

        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));
        let mut file_infos: Vec<(String, String, String, u64, String)> = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source
                .list_quark_share_files(&share_id, &password, &share_info.stoken, &parent_id)
                .await?;

            for file in &files {
                file_infos.push((
                    file.fid.clone(),
                    file.share_fid_token.clone(),
                    file.name.clone(),
                    file.size,
                    parent_path.clone(),
                ));
            }

            traversal.extend(collect_quark_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        let md5_pairs: Vec<(String, String)> = file_infos
            .iter()
            .map(|(fid, token, _, _, _)| (fid.clone(), token.clone()))
            .collect();
        let md5_map = self
            .share_source
            .batch_get_quark_file_md5s(&share_id, &password, &share_info.stoken, &md5_pairs)
            .await?;

        Ok(file_infos
            .into_iter()
            .map(|(fid, _token, name, size, path)| RawFile {
                id: None,
                name,
                etag: md5_map
                    .get(&fid)
                    .cloned()
                    .unwrap_or_default()
                    .as_str()
                    .into(),
                size,
                path,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::QuarkShareService;
    use url::Url;

    use crate::{
        application::import::{
            LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo, QuarkFile,
            QuarkFolder, QuarkShareInfo,
        },
        domain::share::Etag,
        error::AppResult,
    };

    use super::super::ShareClient;

    type QuarkFilesByParent = HashMap<String, (Vec<QuarkFolder>, Vec<QuarkFile>)>;

    #[derive(Clone, Default)]
    struct FakeShareClient {
        quark_files: Arc<Mutex<QuarkFilesByParent>>,
        quark_md5s: Arc<Mutex<HashMap<String, String>>>,
    }

    impl ShareClient for FakeShareClient {
        async fn list_pan123_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            _parent_id: i64,
        ) -> AppResult<Vec<LibraryFile>> {
            Ok(Vec::new())
        }

        async fn get_pan189_share_info(&self, _share_code: &str) -> AppResult<Pan189ShareInfo> {
            Ok(Pan189ShareInfo::default())
        }

        async fn list_pan189_share_files(
            &self,
            _share_id: i64,
            _share_mode: i32,
            _parent_id: &str,
        ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
            Ok((Vec::new(), Vec::new()))
        }

        async fn download_pan189_share_file(
            &self,
            _share_id: i64,
            _file: &Pan189File,
        ) -> AppResult<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn list_pan115_share_files(
            &self,
            _share_code: &str,
            _receive_code: &str,
            _cid: &str,
        ) -> AppResult<Vec<Pan115FileEntry>> {
            Ok(Vec::new())
        }

        async fn get_quark_share_info(
            &self,
            _share_id: &str,
            _password: &str,
        ) -> AppResult<QuarkShareInfo> {
            Ok(QuarkShareInfo {
                stoken: "stoken".into(),
            })
        }

        async fn list_quark_share_files(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            pdir_fid: &str,
        ) -> AppResult<(Vec<QuarkFolder>, Vec<QuarkFile>)> {
            Ok(self
                .quark_files
                .lock()
                .unwrap()
                .get(pdir_fid)
                .cloned()
                .unwrap_or((Vec::new(), Vec::new())))
        }

        async fn batch_get_quark_file_md5s(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            _file_infos: &[(String, String)],
        ) -> AppResult<HashMap<String, String>> {
            Ok(self.quark_md5s.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn fills_raw_file_md5_from_batch_lookup() {
        let source = FakeShareClient {
            quark_files: Arc::new(Mutex::new(HashMap::from([
                (
                    "0".to_string(),
                    (
                        vec![QuarkFolder {
                            fid: "dir-1".into(),
                            name: "Show".into(),
                        }],
                        Vec::new(),
                    ),
                ),
                (
                    "dir-1".to_string(),
                    (
                        Vec::new(),
                        vec![QuarkFile {
                            fid: "file-1".into(),
                            name: "Episode 01.mkv".into(),
                            size: 42,
                            share_fid_token: "token-1".into(),
                        }],
                    ),
                ),
            ]))),
            quark_md5s: Arc::new(Mutex::new(HashMap::from([(
                "file-1".to_string(),
                "0123456789abcdef0123456789abcdef".to_string(),
            )]))),
        };

        let raw_files = QuarkShareService::new(source)
            .raw_files_from_url(&Url::parse("https://pan.quark.cn/s/share-id").unwrap())
            .await
            .unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].path, "/Show");
        assert!(matches!(
            &raw_files[0].etag,
            Etag::Md5(value) if value == "0123456789abcdef0123456789abcdef"
        ));
    }
}
