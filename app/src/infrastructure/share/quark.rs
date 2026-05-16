use url::Url;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
    infrastructure::client::quark,
};

use super::{collect::collect_quark_directory_entries, traversal::ShareTraversal};

pub(crate) fn parse_share_parts(url: &Url) -> Option<(String, String)> {
    if !(url.host_str().is_some_and(|host| host == "pan.quark.cn") && url.path().starts_with("/s/"))
    {
        return None;
    }

    let share_id = path_segment_after_prefix(url, 1);
    if share_id.is_empty() {
        return None;
    }

    let password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    Some((share_id, password))
}

#[derive(Clone)]
pub struct QuarkShareService<S> {
    share_source: S,
}

pub(crate) trait QuarkShareSource: Clone {
    async fn get_share_info(&self, share_id: &str, password: &str) -> AppResult<String>;
    async fn list_share_files(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> AppResult<(Vec<quark::Folder>, Vec<quark::File>)>;
    async fn batch_get_file_md5s(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        file_infos: &[(String, String)],
    ) -> AppResult<std::collections::HashMap<String, String>>;
}

impl QuarkShareSource for quark::Client {
    async fn get_share_info(&self, share_id: &str, password: &str) -> AppResult<String> {
        Ok(self.get_share_info(share_id, password).await?)
    }

    async fn list_share_files(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        pdir_fid: &str,
    ) -> AppResult<(Vec<quark::Folder>, Vec<quark::File>)> {
        let (folders, files) = self
            .list_share_files(share_id, password, stoken, pdir_fid, 1, 1000)
            .await?;
        if folders.len() + files.len() >= 1000 {
            tracing::warn!(
                "quark directory {} has >= 1000 entries, results may be truncated",
                pdir_fid
            );
        }
        Ok((folders, files))
    }

    async fn batch_get_file_md5s(
        &self,
        share_id: &str,
        password: &str,
        stoken: &str,
        file_infos: &[(String, String)],
    ) -> AppResult<std::collections::HashMap<String, String>> {
        let fids: Vec<String> = file_infos.iter().map(|(fid, _)| fid.clone()).collect();
        let fid_tokens: Vec<String> = file_infos.iter().map(|(_, token)| token.clone()).collect();
        Ok(self
            .batch_download_info(share_id, password, stoken, &fids, &fid_tokens)
            .await?)
    }
}

impl<S: QuarkShareSource> QuarkShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_share(
        &self,
        share_id: &str,
        password: &str,
    ) -> AppResult<Vec<RawFile>> {
        if share_id.is_empty() {
            return Err(AppError::NotFound(
                "Can not extract share id from URL".into(),
            ));
        }

        let share_info = self.share_source.get_share_info(share_id, password).await?;

        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));
        let mut file_infos: Vec<(String, String, String, u64, String)> = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source
                .list_share_files(share_id, password, &share_info, &parent_id)
                .await?;

            for file in &files {
                file_infos.push((
                    file.fid.clone(),
                    file.share_fid_token.clone(),
                    file.file_name.clone(),
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
            .batch_get_file_md5s(share_id, password, &share_info, &md5_pairs)
            .await?;

        Ok(file_infos
            .into_iter()
            .map(|(fid, _token, name, size, path)| RawFile {
                id: None,
                name,
                hash: md5_map
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

    use super::{QuarkShareService, parse_share_parts};
    use url::Url;

    use crate::{domain::share::FileHash, error::AppResult, infrastructure::client::quark};

    #[test]
    fn matches_supported_quark_urls_and_parses_share_parts() {
        let url = Url::parse("https://pan.quark.cn/s/share-id?pwd=abc").unwrap();

        assert_eq!(
            parse_share_parts(&url),
            Some(("share-id".into(), "abc".into()))
        );
    }

    type QuarkFilesByParent = HashMap<String, (Vec<quark::Folder>, Vec<quark::File>)>;

    #[derive(Clone, Default)]
    struct FakeShareClient {
        quark_files: Arc<Mutex<QuarkFilesByParent>>,
        quark_md5s: Arc<Mutex<HashMap<String, String>>>,
    }

    impl super::QuarkShareSource for FakeShareClient {
        async fn get_share_info(&self, _share_id: &str, _password: &str) -> AppResult<String> {
            Ok("stoken".into())
        }

        async fn list_share_files(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            pdir_fid: &str,
        ) -> AppResult<(Vec<quark::Folder>, Vec<quark::File>)> {
            Ok(self
                .quark_files
                .lock()
                .unwrap()
                .get(pdir_fid)
                .cloned()
                .unwrap_or((Vec::new(), Vec::new())))
        }

        async fn batch_get_file_md5s(
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
                        vec![quark::Folder {
                            fid: "dir-1".into(),
                            file_name: "Show".into(),
                        }],
                        Vec::new(),
                    ),
                ),
                (
                    "dir-1".to_string(),
                    (
                        Vec::new(),
                        vec![quark::File {
                            fid: "file-1".into(),
                            file_name: "Episode 01.mkv".into(),
                            size: 42,
                            share_fid_token: "token-1".into(),
                            dir: false,
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
            .raw_files_from_share("share-id", "")
            .await
            .unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].path, "/Show");
        assert!(matches!(
            &raw_files[0].hash,
            FileHash::Md5(value) if value == "0123456789abcdef0123456789abcdef"
        ));
    }
}
