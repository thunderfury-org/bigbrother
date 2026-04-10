use std::time::Duration;

use tracing::error;

use crate::error::{AppError, AppResult};

use super::ports::{DownloadUrlCache, DownloadUrlError, DownloadUrlSource};

#[derive(Debug)]
pub enum ResolveDownloadUrlResult {
    Redirect(String),
    Unauthorized,
    NotFound,
}

#[derive(Clone)]
pub struct ResolveDownloadUrlService<C, S> {
    cache: C,
    source: S,
}

impl<C, S> ResolveDownloadUrlService<C, S> {
    pub fn new(cache: C, source: S) -> Self {
        Self { cache, source }
    }
}

impl<C, S> ResolveDownloadUrlService<C, S>
where
    C: DownloadUrlCache,
    S: DownloadUrlSource,
{
    pub async fn resolve(&self, file_id: i64) -> AppResult<ResolveDownloadUrlResult> {
        let cache_key = format!("pan123:download_url:{file_id}");
        if let Some(cached_url) = self.cache.get_download_url(&cache_key).await? {
            return Ok(ResolveDownloadUrlResult::Redirect(cached_url));
        }

        match self.source.get_download_url(file_id).await {
            Ok(url) => {
                if url.is_empty() {
                    return Err(AppError::RuleRejected(
                        "download url source returned empty url".to_owned(),
                    ));
                }

                if let Err(err) = self
                    .cache
                    .set_download_url(&cache_key, &url, Duration::from_mins(30))
                    .await
                {
                    error!("Failed to cache download url for file {file_id}, {err}");
                }

                Ok(ResolveDownloadUrlResult::Redirect(url))
            }
            Err(DownloadUrlError::Unauthorized) => Ok(ResolveDownloadUrlResult::Unauthorized),
            Err(DownloadUrlError::NotFound(_)) => Ok(ResolveDownloadUrlResult::NotFound),
            Err(err) => Err(AppError::Dependency(format!(
                "failed to get download url: {err}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::ports::DownloadUrlResult;

    use super::*;

    #[derive(Clone, Default)]
    struct FakeCache {
        stored: Arc<Mutex<Option<String>>>,
    }

    impl DownloadUrlCache for FakeCache {
        async fn get_download_url(&self, _key: &str) -> AppResult<Option<String>> {
            Ok(self.stored.lock().unwrap().clone())
        }

        async fn set_download_url(&self, _key: &str, url: &str, _ttl: Duration) -> AppResult<()> {
            *self.stored.lock().unwrap() = Some(url.to_owned());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeSource {
        result: Arc<Mutex<Result<String, &'static str>>>,
    }

    impl DownloadUrlSource for FakeSource {
        async fn get_download_url(&self, _file_id: i64) -> DownloadUrlResult<String> {
            match &*self.result.lock().unwrap() {
                Ok(url) => Ok(url.clone()),
                Err(kind) if *kind == "not_found" => {
                    Err(DownloadUrlError::NotFound("missing".to_string()))
                }
                Err(kind) if *kind == "unauthorized" => Err(DownloadUrlError::Unauthorized),
                Err(kind) => Err(DownloadUrlError::Error((*kind).to_string())),
            }
        }
    }

    #[tokio::test]
    async fn resolve_returns_cached_url() {
        let cache = FakeCache {
            stored: Arc::new(Mutex::new(Some("https://cached".to_string()))),
        };
        let source = FakeSource {
            result: Arc::new(Mutex::new(Ok("https://remote".to_string()))),
        };
        let service = ResolveDownloadUrlService::new(cache, source);

        let result = service.resolve(1).await.unwrap();
        assert!(matches!(
            result,
            ResolveDownloadUrlResult::Redirect(url) if url == "https://cached"
        ));
    }

    #[tokio::test]
    async fn resolve_maps_not_found() {
        let service = ResolveDownloadUrlService::new(
            FakeCache::default(),
            FakeSource {
                result: Arc::new(Mutex::new(Err("not_found"))),
            },
        );

        let result = service.resolve(1).await.unwrap();
        assert!(matches!(result, ResolveDownloadUrlResult::NotFound));
    }

    #[tokio::test]
    async fn resolve_maps_source_error_to_dependency_error() {
        let service = ResolveDownloadUrlService::new(
            FakeCache::default(),
            FakeSource {
                result: Arc::new(Mutex::new(Err("boom"))),
            },
        );

        let error = service.resolve(1).await.unwrap_err();
        assert!(matches!(error, AppError::Dependency(_)));
    }
}
