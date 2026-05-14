use url::Url;

use crate::{domain::share::RawFile, error::AppResult};

pub trait ShareResolver: Clone {
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>>;
}
