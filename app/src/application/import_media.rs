use crate::{
    error::AppResult,
    library::{ImportedMedia, ShareUrl},
};

pub trait ImportMediaGateway {
    async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>>;
    async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>>;
    async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>>;
}

pub struct ImportMediaService<G> {
    gateway: G,
}

impl<G> ImportMediaService<G> {
    pub fn new(gateway: G) -> Self {
        Self { gateway }
    }
}

impl<G> ImportMediaService<G>
where
    G: ImportMediaGateway,
{
    pub async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        self.gateway.import_from_share_url(url).await
    }

    pub async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        self.gateway.import_from_fslink(fslink).await
    }

    pub async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        self.gateway.import_from_json(json).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use reqwest::Url;

    use super::*;

    #[derive(Clone, Default)]
    struct FakeImportGateway {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl ImportMediaGateway for FakeImportGateway {
        async fn import_from_share_url(
            &self,
            _url: &ShareUrl<'_>,
        ) -> AppResult<Vec<ImportedMedia>> {
            self.calls.lock().unwrap().push("share".to_string());
            Ok(Vec::new())
        }

        async fn import_from_fslink(&self, _fslink: &str) -> AppResult<Vec<ImportedMedia>> {
            self.calls.lock().unwrap().push("fslink".to_string());
            Ok(Vec::new())
        }

        async fn import_from_json(&self, _json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
            self.calls.lock().unwrap().push("json".to_string());
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn service_delegates_to_gateway() {
        let gateway = FakeImportGateway::default();
        let service = ImportMediaService::new(gateway.clone());
        let url = Url::parse("https://www.123684.com/s/test").unwrap();
        let share = ShareUrl::from(&url).unwrap();

        service.import_from_share_url(&share).await.unwrap();
        service.import_from_fslink("fslink").await.unwrap();
        service.import_from_json(vec![1, 2, 3]).await.unwrap();

        assert_eq!(
            gateway.calls.lock().unwrap().as_slice(),
            ["share", "fslink", "json"]
        );
    }
}
