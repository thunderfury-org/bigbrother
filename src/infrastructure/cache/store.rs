use std::time::Duration;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    application::ports::DownloadUrlCache, error::AppResult, infrastructure::entity::cache,
};

/// JSON cache with optional TTL.
#[derive(Clone)]
pub struct Cache {
    db: DatabaseConnection,
}

impl Cache {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get<V: DeserializeOwned + Send>(&self, key: &str) -> AppResult<Option<V>> {
        match cache::get_by_key(&self.db, key).await? {
            Some(record) => {
                // Check expiration (lazy deletion)
                if let Some(expired_at) = record.expired_at
                    && expired_at <= Utc::now()
                {
                    cache::delete_by_key(&self.db, key).await?;
                    return Ok(None);
                }

                // Deserialize value
                let value = serde_json::from_str(&record.value)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    pub async fn set<V: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &V,
        ttl: Option<Duration>,
    ) -> AppResult<()> {
        let value_json = serde_json::to_string(value)?;
        let expired_at = ttl.map(|d| Utc::now() + chrono::Duration::from_std(d).unwrap());

        cache::set_record(&self.db, key, &value_json, expired_at).await?;
        Ok(())
    }

    pub async fn clear_expired(&self) -> AppResult<u64> {
        Ok(cache::delete_expired(&self.db).await?)
    }
}

#[async_trait::async_trait]
impl DownloadUrlCache for Cache {
    async fn get_download_url(&self, key: &str) -> AppResult<Option<String>> {
        self.get(key).await
    }

    async fn set_download_url(&self, key: &str, url: &str, ttl: Duration) -> AppResult<()> {
        self.set(key, &url, Some(ttl)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::{Migrator, MigratorTrait};
    use sea_orm::Database;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: u32,
        name: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct ComplexData {
        numbers: Vec<i32>,
        text: String,
        nested: Option<TestData>,
    }

    async fn setup_test_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    #[tokio::test]
    async fn test_cache_bounds() {
        fn assert_send_sync<T: Send + Sync>() {}
        fn assert_download_url_cache<T: DownloadUrlCache>() {}
        assert_send_sync::<Cache>();
        assert_download_url_cache::<Cache>();
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        let data = TestData {
            id: 42,
            name: "test".to_string(),
        };

        // Set value without TTL
        cache.set("test_key", &data, None).await.unwrap();

        // Get value back
        let result: Option<TestData> = cache.get("test_key").await.unwrap();
        assert_eq!(result, Some(data));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        let result: Option<TestData> = cache.get("nonexistent").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_overwrite_existing() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set initial value
        cache.set("key", &"first", None).await.unwrap();

        // Overwrite with new value
        cache.set("key", &"second", None).await.unwrap();

        // Verify new value
        let result: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(result, Some("second".to_string()));
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set with very short TTL
        cache
            .set("ttl_key", &"value", Some(Duration::from_millis(1)))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Value should be expired and return None
        let result: Option<String> = cache.get("ttl_key").await.unwrap();
        assert_eq!(result, None);

        // Key should not exist after lazy deletion
        assert!(cache.get::<String>("ttl_key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_ttl_not_expired() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set with long TTL
        cache
            .set("ttl_key", &"value", Some(Duration::from_secs(3600)))
            .await
            .unwrap();

        // Value should still be available
        let result: Option<String> = cache.get("ttl_key").await.unwrap();
        assert_eq!(result, Some("value".to_string()));
    }

    #[tokio::test]
    async fn test_clear_expired() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set multiple keys with different TTLs
        cache
            .set("expired1", &1, Some(Duration::from_millis(1)))
            .await
            .unwrap();
        cache
            .set("expired2", &2, Some(Duration::from_millis(1)))
            .await
            .unwrap();
        cache
            .set("valid", &3, Some(Duration::from_secs(3600)))
            .await
            .unwrap();
        cache.set("no_ttl", &4, None).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Clear expired entries
        let count = cache.clear_expired().await.unwrap();
        assert_eq!(count, 2); // Only expired1 and expired2

        // Verify valid keys still exist
        assert!(cache.get::<i32>("valid").await.unwrap().is_some());
        assert!(cache.get::<i32>("no_ttl").await.unwrap().is_some());

        // Verify expired keys are gone
        assert!(cache.get::<i32>("expired1").await.unwrap().is_none());
        assert!(cache.get::<i32>("expired2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_complex_data_types() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        let data = ComplexData {
            numbers: vec![1, 2, 3, 4, 5],
            text: "complex data".to_string(),
            nested: Some(TestData {
                id: 99,
                name: "nested".to_string(),
            }),
        };

        cache.set("complex", &data, None).await.unwrap();

        let result: Option<ComplexData> = cache.get("complex").await.unwrap();
        assert_eq!(result, Some(data));
    }

    #[tokio::test]
    async fn test_multiple_keys() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set multiple different keys
        cache.set("key1", &1, None).await.unwrap();
        cache.set("key2", &2, None).await.unwrap();
        cache.set("key3", &3, None).await.unwrap();

        // Verify correct values
        let v1: Option<i32> = cache.get("key1").await.unwrap();
        let v2: Option<i32> = cache.get("key2").await.unwrap();
        let v3: Option<i32> = cache.get("key3").await.unwrap();

        assert_eq!(v1, Some(1));
        assert_eq!(v2, Some(2));
        assert_eq!(v3, Some(3));
    }

    #[tokio::test]
    async fn test_cache_clone() {
        let db = setup_test_db().await;
        let cache1 = Cache::new(db);
        let cache2 = cache1.clone();

        // Set through cache1
        cache1.set("shared", &"data", None).await.unwrap();

        // Read through cache2 (shares same DB connection)
        let result: Option<String> = cache2.get("shared").await.unwrap();
        assert_eq!(result, Some("data".to_string()));
    }

    #[tokio::test]
    async fn test_update_ttl() {
        let db = setup_test_db().await;
        let cache = Cache::new(db);

        // Set with short TTL
        cache
            .set("key", &"value1", Some(Duration::from_millis(100)))
            .await
            .unwrap();

        // Update with longer TTL and new value
        cache
            .set("key", &"value2", Some(Duration::from_secs(3600)))
            .await
            .unwrap();

        // Wait past original TTL
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should still exist with new value
        let result: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(result, Some("value2".to_string()));
    }
}
