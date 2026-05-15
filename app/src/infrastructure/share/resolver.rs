use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
    infrastructure::share::{
        pan115::{self, Pan115ShareService, Pan115ShareSource},
        pan123::{self, Pan123ShareService, Pan123ShareSource},
        pan189::{self, Pan189ShareService, Pan189ShareSource},
        quark::{self, QuarkShareService, QuarkShareSource},
    },
};

pub trait ShareResolver: Clone {
    async fn raw_files_from_url(&self, url: &str) -> AppResult<Option<Vec<RawFile>>>;
}

#[derive(Clone)]
pub struct ShareResolverService<P123, P189, P115, Q> {
    pan123: Pan123ShareService<P123>,
    pan189: Pan189ShareService<P189>,
    pan115: Pan115ShareService<P115>,
    quark: QuarkShareService<Q>,
}

impl<P123, P189, P115, Q> ShareResolverService<P123, P189, P115, Q> {
    pub fn new(
        pan123: Pan123ShareService<P123>,
        pan189: Pan189ShareService<P189>,
        pan115: Pan115ShareService<P115>,
        quark: QuarkShareService<Q>,
    ) -> Self {
        Self {
            pan123,
            pan189,
            pan115,
            quark,
        }
    }
}

impl<P123: Pan123ShareSource, P189: Pan189ShareSource, P115: Pan115ShareSource, Q: QuarkShareSource> ShareResolver
    for ShareResolverService<P123, P189, P115, Q>
{
    async fn raw_files_from_url(&self, url: &str) -> AppResult<Option<Vec<RawFile>>> {
        let url = url::Url::parse(url).map_err(|err| {
            AppError::InvalidParameter(format!("invalid share url '{url}': {err}"))
        })?;

        if let Some((share_key, password)) = pan123::parse_share_parts(&url) {
            self.pan123
                .raw_files_from_share(&share_key, &password)
                .await
                .map(Some)
        } else if let Some(share_code) = pan189::parse_share_code(&url) {
            self.pan189
                .raw_files_from_share_code(&share_code)
                .await
                .map(Some)
        } else if let Some((share_code, receive_code)) = pan115::parse_share_parts(&url) {
            self.pan115
                .raw_files_from_share(&share_code, &receive_code)
                .await
                .map(Some)
        } else if let Some((share_id, password)) = quark::parse_share_parts(&url) {
            self.quark
                .raw_files_from_share(&share_id, &password)
                .await
                .map(Some)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ShareResolver, ShareResolverService};

    use crate::{
        infrastructure::{
            client::{pan115, pan123, pan189, quark},
            share::{
                pan115::{Pan115ShareService, Pan115ShareSource},
                pan123::{Pan123ShareService, Pan123ShareSource},
                pan189::{Pan189ShareService, Pan189ShareSource},
                quark::{QuarkShareService, QuarkShareSource},
            },
        },
        error::AppResult,
    };

    #[derive(Clone, Default)]
    struct FakePan123ShareSource {
        files_by_parent: std::collections::HashMap<i64, Vec<pan123::File>>,
    }

    impl Pan123ShareSource for FakePan123ShareSource {
        async fn list_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            parent_id: i64,
        ) -> AppResult<Vec<pan123::File>> {
            Ok(self
                .files_by_parent
                .get(&parent_id)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[derive(Clone, Default)]
    struct FakePan189ShareSource;

    impl Pan189ShareSource for FakePan189ShareSource {
        async fn get_share_info(&self, _share_code: &str) -> AppResult<pan189::ShareInfo> {
            Ok(pan189::ShareInfo::fake("", "", 0, 0))
        }

        async fn list_share_files(
            &self,
            _share_id: i64,
            _share_mode: i32,
            _parent_id: &str,
        ) -> AppResult<(Vec<pan189::Folder>, Vec<pan189::File>)> {
            Ok((Vec::new(), Vec::new()))
        }

        async fn download_share_file(
            &self,
            _share_id: i64,
            _file: &pan189::File,
        ) -> AppResult<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct FakePan115ShareSource;

    impl Pan115ShareSource for FakePan115ShareSource {
        async fn list_share_files(
            &self,
            _share_code: &str,
            _receive_code: &str,
            _cid: &str,
        ) -> AppResult<Vec<pan115::FileEntry>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct FakeQuarkShareSource;

    impl QuarkShareSource for FakeQuarkShareSource {
        async fn get_share_info(
            &self,
            _share_id: &str,
            _password: &str,
        ) -> AppResult<String> {
            Ok(String::new())
        }

        async fn list_share_files(
            &self,
            _share_id: &str,
            _password: &str,
            _stoken: &str,
            _pdir_fid: &str,
        ) -> AppResult<(Vec<quark::Folder>, Vec<quark::File>)> {
            Ok((Vec::new(), Vec::new()))
        }

        async fn batch_get_file_md5s(
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
        let resolver = ShareResolverService::new(
            Pan123ShareService::new(FakePan123ShareSource::default()),
            Pan189ShareService::new(FakePan189ShareSource),
            Pan115ShareService::new(FakePan115ShareSource),
            QuarkShareService::new(FakeQuarkShareSource),
        );

        let result = resolver
            .raw_files_from_url("https://example.com/share")
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn routes_pan123_urls_to_pan123_service() {
        let resolver = ShareResolverService::new(
            Pan123ShareService::new(FakePan123ShareSource {
                files_by_parent: std::collections::HashMap::from([(
                    0,
                    vec![pan123::File {
                        file_id: 7,
                        file_name: "Movie.mkv".into(),
                        file_type: 0,
                        size: 99,
                        _created_at: time::OffsetDateTime::UNIX_EPOCH,
                        _updated_at: time::OffsetDateTime::UNIX_EPOCH,
                        etag: "ABCDEF0123456789ABCDEF0123456789".into(),
                        abs_path: String::new(),
                    }],
                )]),
            }),
            Pan189ShareService::new(FakePan189ShareSource),
            Pan115ShareService::new(FakePan115ShareSource),
            QuarkShareService::new(FakeQuarkShareSource),
        );

        let result = resolver
            .raw_files_from_url("https://www.123684.com/s/share-key")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Movie.mkv");
        assert_eq!(result[0].size, 99);
    }

    #[tokio::test]
    async fn rejects_invalid_url() {
        let resolver = ShareResolverService::new(
            Pan123ShareService::new(FakePan123ShareSource::default()),
            Pan189ShareService::new(FakePan189ShareSource),
            Pan115ShareService::new(FakePan115ShareSource),
            QuarkShareService::new(FakeQuarkShareSource),
        );

        let err = resolver.raw_files_from_url("not a url").await.unwrap_err();

        assert!(matches!(err, crate::error::AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("invalid share url"));
    }
}
