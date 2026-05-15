use url::Url;

use crate::{
    application::import::Pan189File,
    domain::share::RawFile,
    error::{AppError, AppResult},
};

use super::{
    ShareClient, collect::collect_pan189_directory_entries, file_parser::ShareFileParser,
    traversal::ShareTraversal,
};

pub(crate) fn parse_share_code(url: &Url) -> Option<String> {
    if !(url.host_str().is_some_and(|host| host == "cloud.189.cn")
        && (url.path().starts_with("/t/") || url.path() == "/web/share"))
    {
        return None;
    }

    let share_code = url
        .query_pairs()
        .find(|(key, value)| key == "code" && !value.is_empty())
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| {
            if url.path().starts_with("/t/") {
                path_segment_after_prefix(url, 1)
            } else {
                String::new()
            }
        });

    (!share_code.is_empty()).then_some(share_code)
}

#[derive(Clone)]
pub struct Pan189ShareService<S> {
    share_source: S,
}

impl<S: ShareClient> Pan189ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_share_code(&self, share_code: &str) -> AppResult<Vec<RawFile>> {
        if share_code.is_empty() {
            return Err(AppError::NotFound(
                "Can not extract share code from URL".into(),
            ));
        }

        let share_info = self.share_source.get_pan189_share_info(share_code).await?;
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
                cas_raw_files.extend(ShareFileParser::parse_json_bytes_with_context(
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

fn path_segment_after_prefix(url: &Url, index: usize) -> String {
    url.path()
        .strip_prefix('/')
        .and_then(|path| path.split('/').nth(index))
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::{Pan189ShareService, parse_share_code};
    use url::Url;

    use crate::{
        application::import::{
            Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo, QuarkFile, QuarkFolder,
            QuarkShareInfo,
        },
        error::{AppError, AppResult},
    };

    use super::super::ShareClient;

    #[test]
    fn matches_supported_pan189_urls_and_parses_share_code() {
        let path_url = Url::parse("https://cloud.189.cn/t/share189").unwrap();
        let query_url = Url::parse("https://cloud.189.cn/web/share?code=share189").unwrap();
        let missing_code_url = Url::parse("https://cloud.189.cn/web/share").unwrap();

        assert_eq!(parse_share_code(&path_url), Some("share189".into()));
        assert_eq!(parse_share_code(&query_url), Some("share189".into()));
        assert_eq!(parse_share_code(&missing_code_url), None);
    }

    type Pan189FilesByParent = HashMap<String, (Vec<Pan189Folder>, Vec<Pan189File>)>;

    #[derive(Clone, Default)]
    struct FakeShareClient {
        pan189_share_info: Arc<Mutex<Option<Pan189ShareInfo>>>,
        pan189_files: Arc<Mutex<Pan189FilesByParent>>,
        pan189_downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl ShareClient for FakeShareClient {
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
        let source = FakeShareClient {
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
            .raw_files_from_share_code("share189")
            .await
            .unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].name, "S01E01.mp4");
        assert_eq!(raw_files[0].path, "Breaking Bad (2008) {tmdb-1396}");
    }
}
