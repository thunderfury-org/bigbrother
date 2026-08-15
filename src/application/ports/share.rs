use crate::{domain::share::RawFile, error::AppResult};

pub trait ShareResolver {
    fn raw_files_from_url(
        &self,
        url: &str,
    ) -> impl std::future::Future<Output = AppResult<Option<Vec<RawFile>>>> + Send;
}
