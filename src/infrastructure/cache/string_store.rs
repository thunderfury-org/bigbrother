use std::time::Duration;

use crate::{application::ports::DownloadUrlCache, error::AppResult, infrastructure::cache::Cache};

#[derive(Clone)]
pub struct StringCacheStore {
    cache: Cache,
}

impl StringCacheStore {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl DownloadUrlCache for StringCacheStore {
    async fn get_download_url(&self, key: &str) -> AppResult<Option<String>> {
        self.cache.get(key).await
    }

    async fn set_download_url(&self, key: &str, value: &str, ttl: Duration) -> AppResult<()> {
        self.cache.set(key, &value.to_string(), Some(ttl)).await
    }
}
