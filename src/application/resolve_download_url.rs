use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, warn};

use crate::error::{AppError, AppResult};

use super::ports::{
    DownloadUrlCache, DownloadUrlCacheHandle, DownloadUrlSource, DownloadUrlSourceHandle,
};

type SharedResolveResult = AppResult<String>;

#[derive(Debug)]
struct InflightResolve {
    result: Mutex<Option<SharedResolveResult>>,
    notify: Notify,
}

impl InflightResolve {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    async fn wait(&self) -> SharedResolveResult {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if let Some(result) = self.result.lock().await.clone() {
                return result;
            }

            notified.await;
        }
    }

    async fn complete(&self, result: SharedResolveResult) {
        *self.result.lock().await = Some(result);
        self.notify.notify_waiters();
    }
}

#[derive(Clone)]
pub struct ResolveDownloadUrlService {
    cache: DownloadUrlCacheHandle,
    source: DownloadUrlSourceHandle,
    inflight: Arc<Mutex<HashMap<i64, Arc<InflightResolve>>>>,
}

impl ResolveDownloadUrlService {
    pub fn new(
        cache: impl DownloadUrlCache + 'static,
        source: impl DownloadUrlSource + 'static,
    ) -> Self {
        Self {
            cache: Arc::new(cache),
            source: Arc::new(source),
            inflight: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ResolveDownloadUrlService {
    /// Resolve a download URL for the given file_id.
    ///
    /// Returns `Ok(url)` on success, or an `AppError`:
    /// - `AppError::Unauthorized` when the source reports unauthorized
    /// - `AppError::NotFound` when the file is not found
    /// - `AppError::ExternalService` when the source encounters an error
    pub async fn resolve(&self, file_id: i64) -> AppResult<String> {
        let cache_key = format!("pan123:download_url:{file_id}");
        if let Some(cached_url) = self.cache.get_download_url(&cache_key).await? {
            return Ok(cached_url);
        }
        debug!("download url cache miss for file {file_id}");

        let (entry, owner) = {
            let mut inflight = self.inflight.lock().await;
            if let Some(entry) = inflight.get(&file_id) {
                debug!("joining in-flight download url resolve for file {file_id}");
                (entry.clone(), false)
            } else {
                let entry = Arc::new(InflightResolve::new());
                inflight.insert(file_id, entry.clone());
                (entry, true)
            }
        };

        if !owner {
            return entry.wait().await;
        }

        let started_at = Instant::now();
        let result = self.resolve_uncached(file_id, &cache_key).await;
        match &result {
            Ok(_) => {
                debug!(
                    "resolved download url for file {file_id} in {:?}",
                    started_at.elapsed()
                );
            }
            Err(err) => {
                warn!(
                    "download url resolve failed for file {file_id} after {:?}: {err}",
                    started_at.elapsed()
                );
            }
        }
        entry.complete(result.clone()).await;
        self.inflight.lock().await.remove(&file_id);
        result
    }

    async fn resolve_uncached(&self, file_id: i64, cache_key: &str) -> SharedResolveResult {
        let url = self.source.get_download_url(file_id).await?;
        if url.is_empty() {
            return Err(AppError::ExternalService(
                "download url source returned empty url".to_owned(),
                false,
            ));
        }

        if let Err(err) = self
            .cache
            .set_download_url(cache_key, &url, Duration::from_mins(30))
            .await
        {
            error!("Failed to cache download url for file {file_id}, {err}");
        }

        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct FakeCache {
        stored: Arc<Mutex<HashMap<String, String>>>,
    }

    impl FakeCache {
        fn with_download_url(file_id: i64, url: &str) -> Self {
            let cache = Self::default();
            cache
                .stored
                .lock()
                .unwrap()
                .insert(format!("pan123:download_url:{file_id}"), url.to_owned());
            cache
        }
    }

    #[async_trait::async_trait]
    impl DownloadUrlCache for FakeCache {
        async fn get_download_url(&self, key: &str) -> AppResult<Option<String>> {
            Ok(self.stored.lock().unwrap().get(key).cloned())
        }

        async fn set_download_url(&self, key: &str, url: &str, _ttl: Duration) -> AppResult<()> {
            self.stored
                .lock()
                .unwrap()
                .insert(key.to_owned(), url.to_owned());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeSource {
        results: Arc<Mutex<HashMap<i64, Result<String, &'static str>>>>,
        calls: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl FakeSource {
        fn new(default_result: Result<String, &'static str>) -> Self {
            Self {
                results: Arc::new(Mutex::new(HashMap::from([(1, default_result)]))),
                calls: Arc::new(AtomicUsize::new(0)),
                delay: Duration::ZERO,
            }
        }

        fn with_results(
            results: impl IntoIterator<Item = (i64, Result<String, &'static str>)>,
        ) -> Self {
            Self {
                results: Arc::new(Mutex::new(results.into_iter().collect())),
                calls: Arc::new(AtomicUsize::new(0)),
                delay: Duration::ZERO,
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl DownloadUrlSource for FakeSource {
        async fn get_download_url(&self, file_id: i64) -> AppResult<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }

            let result = self
                .results
                .lock()
                .unwrap()
                .get(&file_id)
                .cloned()
                .unwrap_or(Err("not_found"));

            match result {
                Ok(url) => Ok(url),
                Err("not_found") => Err(AppError::NotFound("missing".to_string())),
                Err("unauthorized") => Err(AppError::Unauthorized("unauthorized".to_string())),
                Err(kind) => Err(AppError::ExternalService(kind.to_string(), false)),
            }
        }
    }

    #[tokio::test]
    async fn resolve_returns_cached_url() {
        let cache = FakeCache::with_download_url(1, "https://cached");
        let source = FakeSource::new(Ok("https://remote".to_string()));
        let service = ResolveDownloadUrlService::new(cache, source.clone());

        let result = service.resolve(1).await.unwrap();
        assert_eq!(result, "https://cached");
        assert_eq!(source.calls(), 0);
    }

    #[tokio::test]
    async fn resolve_maps_not_found() {
        let service =
            ResolveDownloadUrlService::new(FakeCache::default(), FakeSource::new(Err("not_found")));

        let error = service.resolve(1).await.unwrap_err();
        assert!(matches!(error, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_maps_unauthorized() {
        let service = ResolveDownloadUrlService::new(
            FakeCache::default(),
            FakeSource::new(Err("unauthorized")),
        );

        let error = service.resolve(1).await.unwrap_err();
        assert!(matches!(error, AppError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn resolve_maps_source_error_to_external_service_error() {
        let service =
            ResolveDownloadUrlService::new(FakeCache::default(), FakeSource::new(Err("boom")));

        let error = service.resolve(1).await.unwrap_err();
        assert!(matches!(error, AppError::ExternalService(_, _)));
    }

    #[tokio::test]
    async fn resolve_maps_empty_source_url_to_external_service_error() {
        let service = ResolveDownloadUrlService::new(
            FakeCache::default(),
            FakeSource::new(Ok(String::new())),
        );

        let error = service.resolve(1).await.unwrap_err();
        assert!(matches!(error, AppError::ExternalService(_, false)));
    }

    #[tokio::test]
    async fn resolve_coalesces_concurrent_cache_misses_for_same_file_id() {
        let source =
            FakeSource::new(Ok("https://remote".to_string())).with_delay(Duration::from_millis(50));
        let service = Arc::new(ResolveDownloadUrlService::new(
            FakeCache::default(),
            source.clone(),
        ));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let service = service.clone();
            handles.push(tokio::spawn(async move { service.resolve(1).await }));
        }

        let mut urls = Vec::new();
        for handle in handles {
            match handle.await.unwrap() {
                Ok(url) => urls.push(url),
                Err(other) => panic!("expected ok, got {other:?}"),
            }
        }

        assert_eq!(urls, vec!["https://remote".to_string(); 8]);
        assert_eq!(source.calls(), 1);
    }

    #[tokio::test]
    async fn resolve_does_not_coalesce_different_file_ids() {
        let source = FakeSource::with_results([
            (1, Ok("https://remote/1".to_string())),
            (2, Ok("https://remote/2".to_string())),
        ])
        .with_delay(Duration::from_millis(20));
        let service = Arc::new(ResolveDownloadUrlService::new(
            FakeCache::default(),
            source.clone(),
        ));

        let first = {
            let service = service.clone();
            tokio::spawn(async move { service.resolve(1).await })
        };
        let second = {
            let service = service.clone();
            tokio::spawn(async move { service.resolve(2).await })
        };

        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();

        assert_eq!(first, "https://remote/1");
        assert_eq!(second, "https://remote/2");
        assert_eq!(source.calls(), 2);
    }

    #[tokio::test]
    async fn resolve_cleans_inflight_after_failure_and_allows_retry() {
        let source = FakeSource::new(Err("boom")).with_delay(Duration::from_millis(50));
        let service = Arc::new(ResolveDownloadUrlService::new(
            FakeCache::default(),
            source.clone(),
        ));

        let first = {
            let service = service.clone();
            tokio::spawn(async move { service.resolve(1).await })
        };
        let second = {
            let service = service.clone();
            tokio::spawn(async move { service.resolve(1).await })
        };

        assert!(matches!(
            first.await.unwrap().unwrap_err(),
            AppError::ExternalService(_, _)
        ));
        assert!(matches!(
            second.await.unwrap().unwrap_err(),
            AppError::ExternalService(_, _)
        ));
        assert_eq!(source.calls(), 1);

        let retry = service.resolve(1).await.unwrap_err();
        assert!(matches!(retry, AppError::ExternalService(_, _)));
        assert_eq!(source.calls(), 2);
    }
}
