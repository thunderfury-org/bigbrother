use url::Url;

use crate::{
    application::ports::share::ShareResolver,
    domain::share::RawFile,
    error::AppResult,
    infrastructure::share::{
        pan115::Pan115ShareService,
        pan123::Pan123ShareService,
        pan189::Pan189ShareService,
        quark::QuarkShareService,
        url::{ShareUrl, parse_share_url},
    },
};

use super::ShareClient;

#[derive(Clone)]
pub struct ShareResolverService<S> {
    pan115: Pan115ShareService<S>,
    pan123: Pan123ShareService<S>,
    pan189: Pan189ShareService<S>,
    quark: QuarkShareService<S>,
}

impl<S: ShareClient> ShareResolverService<S> {
    pub fn new(share_source: S) -> Self {
        Self {
            pan115: Pan115ShareService::new(share_source.clone()),
            pan123: Pan123ShareService::new(share_source.clone()),
            pan189: Pan189ShareService::new(share_source.clone()),
            quark: QuarkShareService::new(share_source),
        }
    }
}

impl<S: ShareClient> ShareResolver for ShareResolverService<S> {
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>> {
        match parse_share_url(url) {
            Some(ShareUrl::Pan123(_)) => self.pan123.raw_files_from_url(url).await.map(Some),
            Some(ShareUrl::Pan189(_)) => self.pan189.raw_files_from_url(url).await.map(Some),
            Some(ShareUrl::Pan115(_)) => self.pan115.raw_files_from_url(url).await.map(Some),
            Some(ShareUrl::Quark(_)) => self.quark.raw_files_from_url(url).await.map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use super::ShareResolverService;
    use url::Url;

    use crate::{
        application::{
            import::{
                LibraryFile, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo, QuarkFile,
                QuarkFolder, QuarkShareInfo,
            },
            ports::share::ShareResolver,
        },
        error::AppResult,
    };

    #[derive(Clone, Default)]
    struct FakeShareClient {
        pan123_files: Arc<Mutex<HashMap<i64, Vec<LibraryFile>>>>,
    }

    impl super::super::ShareClient for FakeShareClient {
        async fn list_pan123_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            parent_id: i64,
        ) -> AppResult<Vec<LibraryFile>> {
            Ok(self
                .pan123_files
                .lock()
                .unwrap()
                .get(&parent_id)
                .cloned()
                .unwrap_or_default())
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
    async fn returns_none_for_unsupported_url() {
        let resolver = ShareResolverService::new(FakeShareClient::default());

        let result = resolver
            .raw_files_from_url(&Url::parse("https://example.com/share").unwrap())
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn routes_pan123_urls_to_pan123_service() {
        let resolver = ShareResolverService::new(FakeShareClient {
            pan123_files: Arc::new(Mutex::new(HashMap::from([(
                0,
                vec![LibraryFile {
                    file_id: 7,
                    file_name: "Movie.mkv".into(),
                    is_dir: false,
                    size: 99,
                    etag: "ABCDEF0123456789ABCDEF0123456789".into(),
                }],
            )]))),
        });

        let result = resolver
            .raw_files_from_url(&Url::parse("https://www.123684.com/s/share-key").unwrap())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Movie.mkv");
        assert_eq!(result[0].size, 99);
    }
}
