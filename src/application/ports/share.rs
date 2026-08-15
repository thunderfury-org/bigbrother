use crate::{domain::share::RawFile, error::AppResult};

pub trait ShareResolver: Clone {
    async fn raw_files_from_url(&self, url: &str) -> AppResult<Option<Vec<RawFile>>>;
}
