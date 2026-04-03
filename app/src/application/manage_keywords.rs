use crate::error::{AppError, AppResult};

use super::ports::{KeywordRecord, KeywordRepository};

pub struct ManageKeywordsService<R> {
    repo: R,
}

impl<R> ManageKeywordsService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R> ManageKeywordsService<R>
where
    R: KeywordRepository,
{
    pub async fn list(&self) -> AppResult<Vec<KeywordRecord>> {
        self.repo.list_all_keywords().await
    }

    pub async fn list_values(&self) -> AppResult<Vec<String>> {
        Ok(self
            .repo
            .list_all_keywords()
            .await?
            .into_iter()
            .map(|keyword| keyword.value)
            .collect())
    }

    pub async fn add(&self, keyword: &str) -> AppResult<String> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Err(AppError::InvalidParameter("keyword is empty".to_owned()));
        }

        self.repo.add_keyword(keyword).await?;
        Ok(keyword.to_owned())
    }

    pub async fn delete(&self, id: i64) -> AppResult<()> {
        self.repo.delete_keyword(id).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct FakeKeywordRepo {
        keywords: Arc<Mutex<Vec<KeywordRecord>>>,
    }

    impl KeywordRepository for FakeKeywordRepo {
        async fn list_all_keywords(&self) -> AppResult<Vec<KeywordRecord>> {
            Ok(self.keywords.lock().unwrap().clone())
        }

        async fn add_keyword(&self, value: &str) -> AppResult<()> {
            let mut keywords = self.keywords.lock().unwrap();
            let id = keywords.len() as i64 + 1;
            keywords.push(KeywordRecord {
                id,
                value: value.to_owned(),
            });
            Ok(())
        }

        async fn delete_keyword(&self, id: i64) -> AppResult<()> {
            let mut keywords = self.keywords.lock().unwrap();
            keywords.retain(|keyword| keyword.id != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn add_trims_keyword() {
        let service = ManageKeywordsService::new(FakeKeywordRepo::default());
        let keyword = service.add("  rust  ").await.unwrap();

        assert_eq!(keyword, "rust");
        assert_eq!(
            service.list_values().await.unwrap(),
            vec!["rust".to_string()]
        );
    }

    #[tokio::test]
    async fn add_rejects_empty_keyword() {
        let service = ManageKeywordsService::new(FakeKeywordRepo::default());
        let err = service.add("   ").await.unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
    }
}
