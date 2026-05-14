use url::Url;

use crate::{domain::share::RawFile, error::AppResult};

#[allow(dead_code)]
pub trait ShareResolver: Clone {
    #[allow(dead_code)]
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>>;
}

#[cfg(test)]
mod tests {
    use super::ShareResolver;
    use crate::error::AppResult;

    #[derive(Clone)]
    struct FakeShareResolver;

    impl ShareResolver for FakeShareResolver {
        async fn raw_files_from_url(
            &self,
            _url: &url::Url,
        ) -> AppResult<Option<Vec<crate::domain::share::RawFile>>> {
            Ok(None)
        }
    }

    fn accepts_share_resolver<T: ShareResolver>(_resolver: T) {}

    #[test]
    fn share_resolver_trait_can_be_consumed() {
        accepts_share_resolver(FakeShareResolver);
    }
}
