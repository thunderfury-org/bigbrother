use url::Url;

use crate::{
    application::{import::Pan189File, import_ports::ShareSource},
    domain::{
        import::{
            ShareUrl,
            share_collect::collect_pan189_directory_entries,
            share_walk::ShareTraversal,
            source::{parse_json_to_raw_files_with_context, parse_pan189_share_code},
        },
        share::RawFile,
    },
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct Pan189ShareService<S> {
    share_source: S,
}

impl<S: ShareSource> Pan189ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
        let Some(ShareUrl::Pan189(url)) = ShareUrl::from(url) else {
            return Err(AppError::InvalidParameter(format!(
                "unsupported pan189 share url: {url}"
            )));
        };
        let share_code = parse_pan189_share_code(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {url}"
            )));
        }

        let share_info = self.share_source.get_pan189_share_info(&share_code).await?;
        let mut traversal =
            ShareTraversal::new((share_info.file_id, share_info.file_name.to_owned()));
        let mut cas_files = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;
            cas_files.extend(
                files
                    .iter()
                    .filter(|file| is_cas_file(&file.name))
                    .cloned()
                    .map(|file| CasFileCandidate { file }),
            );
            traversal.extend(collect_pan189_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        let raw_files = traversal.into_raw_files();
        let only_contains_cas_files =
            !raw_files.is_empty() && raw_files.iter().all(|file| is_cas_file(&file.name));

        if only_contains_cas_files {
            let mut cas_raw_files = Vec::new();
            for candidate in &cas_files {
                let json = self
                    .share_source
                    .download_pan189_share_file(share_info.share_id, &candidate.file)
                    .await
                    .map_err(|e| {
                        AppError::InvalidParameter(format!(
                            "检测到天翼 CAS 秒传分享，需要使用自己的天翼云盘账号登录后读取 CAS 内容；请确认 pan189.username / pan189.password 可正常登录且账号未触发设备校验后重试: {e}"
                        ))
                    })?;
                cas_raw_files.extend(parse_json_to_raw_files_with_context(
                    json,
                    &cas_context_path(&candidate.file.name),
                )?);
            }
            Ok(cas_raw_files)
        } else {
            Ok(raw_files)
        }
    }
}

#[derive(Clone)]
struct CasFileCandidate {
    file: Pan189File,
}

fn is_cas_file(name: &str) -> bool {
    name.to_lowercase().ends_with(".cas")
}

fn cas_context_path(name: &str) -> String {
    if name.to_lowercase().ends_with(".cas") {
        name[..name.len() - ".cas".len()].to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::Pan189ShareService;
    use url::Url;

    use crate::{
        application::{
            import::{
                Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo, QuarkFile, QuarkFolder,
                QuarkShareInfo,
            },
            import_ports::ShareSource,
        },
        error::{AppError, AppResult},
    };

    #[derive(Clone, Default)]
    struct FakeShareSource {
        pan189_share_info: Arc<Mutex<Option<Pan189ShareInfo>>>,
        pan189_files: Arc<Mutex<HashMap<String, (Vec<Pan189Folder>, Vec<Pan189File>)>>>,
        pan189_downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl ShareSource for FakeShareSource {
        async fn list_pan123_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            _parent_id: i64,
        ) -> AppResult<Vec<crate::application::import::LibraryFile>> {
            Ok(Vec::new())
        }

        async fn get_pan189_share_info(&self, _share_code: &str) -> AppResult<Pan189ShareInfo> {
            self.pan189_share_info
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| AppError::NotFound("missing fake share info".into()))
        }

        async fn list_pan189_share_files(
            &self,
            _share_id: i64,
            _share_mode: i32,
            parent_id: &str,
        ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
            Ok(self
                .pan189_files
                .lock()
                .unwrap()
                .get(parent_id)
                .cloned()
                .unwrap_or((Vec::new(), Vec::new())))
        }

        async fn download_pan189_share_file(
            &self,
            _share_id: i64,
            file: &Pan189File,
        ) -> AppResult<Vec<u8>> {
            self.pan189_downloads
                .lock()
                .unwrap()
                .get(&file.id)
                .cloned()
                .ok_or_else(|| AppError::InvalidParameter("missing fake pan189 download".into()))
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
            Ok(QuarkShareInfo::default())
        }

        async fn list_quark_share_files(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            _pdir_fid: &str,
        ) -> AppResult<(Vec<QuarkFolder>, Vec<QuarkFile>)> {
            Ok((Vec::new(), Vec::new()))
        }

        async fn batch_get_quark_file_md5s(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            _file_infos: &[(String, String)],
        ) -> AppResult<HashMap<String, String>> {
            Ok(HashMap::new())
        }
    }

    #[tokio::test]
    async fn expands_cas_only_share_with_context_path() {
        let source = FakeShareSource {
            pan189_share_info: Arc::new(Mutex::new(Some(Pan189ShareInfo {
                file_id: "root".into(),
                file_name: "share-root".into(),
                share_id: 1,
                share_mode: 3,
            }))),
            pan189_files: Arc::new(Mutex::new(HashMap::from([(
                "root".to_string(),
                (
                    Vec::new(),
                    vec![Pan189File {
                        id: "cas-1".into(),
                        name: "Breaking Bad (2008) {tmdb-1396}.cas".into(),
                        size: 288,
                        md5: "79202e0c3975e92c12ee2b543ebcd968".into(),
                    }],
                ),
            )]))),
            pan189_downloads: Arc::new(Mutex::new(HashMap::from([(
                "cas-1".to_string(),
                serde_json::json!({
                    "fileName": "S01E01.mp4",
                    "md5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "size": 1001
                })
                .to_string()
                .into_bytes(),
            )]))),
        };

        let raw_files = Pan189ShareService::new(source)
            .raw_files_from_url(&Url::parse("https://cloud.189.cn/t/share189").unwrap())
            .await
            .unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].name, "S01E01.mp4");
        assert_eq!(raw_files[0].path, "Breaking Bad (2008) {tmdb-1396}");
    }
}
