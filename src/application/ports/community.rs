use std::sync::Arc;

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityThread {
    pub tid: i64,
    pub title: String,
    pub tags: Vec<String>,
    pub author: String,
    pub posted_at: String,
    pub comments: u32,
    pub likes: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunityThreadShares {
    pub tid: i64,
    pub title: String,
    pub share_urls: Vec<String>,
}

#[async_trait::async_trait]
pub trait CommunityCatalog: Send + Sync {
    async fn search_threads(&self, keyword: &str, limit: u64) -> AppResult<Vec<CommunityThread>>;

    async fn share_urls_for_thread(&self, tid: i64) -> AppResult<CommunityThreadShares>;
}

pub type CommunityCatalogHandle = Arc<dyn CommunityCatalog>;
