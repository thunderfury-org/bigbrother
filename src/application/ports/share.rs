use std::sync::Arc;

use crate::{domain::share::RawFile, error::AppResult};

#[async_trait::async_trait]
pub trait ShareResolver: Send + Sync {
    async fn raw_files_from_url(&self, url: &str) -> AppResult<Option<Vec<RawFile>>>;
}

pub type ShareResolverHandle = Arc<dyn ShareResolver>;
