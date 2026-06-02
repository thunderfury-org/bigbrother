use crate::application::import_ports::MetadataCatalog;
use crate::application::ports::{SubscriptionCreateInput, SubscriptionRecord, SubscriptionRepository};
use crate::domain::subscription::SubscriptionMediaType;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub(crate) struct SubscriptionCandidate {
    pub tmdb_id: u32,
    pub media_type: SubscriptionMediaType,
    pub title: String,
    pub original_title: String,
}

#[derive(Clone)]
pub(crate) struct ManageSubscriptionsService<R, M> {
    repo: R,
    metadata_catalog: M,
}

impl<R, M> ManageSubscriptionsService<R, M>
where
    R: SubscriptionRepository,
    M: MetadataCatalog,
{
    pub(crate) fn new(repo: R, metadata_catalog: M) -> Self {
        Self {
            repo,
            metadata_catalog,
        }
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<SubscriptionRecord>> {
        self.repo.list_all().await
    }

    pub(crate) async fn create(&self, input: SubscriptionCreateInput) -> AppResult<i64> {
        let has_title = input
            .title_zh
            .as_deref()
            .is_some_and(|t| !t.is_empty())
            || input
                .title_en
                .as_deref()
                .is_some_and(|t| !t.is_empty());
        if !has_title {
            return Err(AppError::InvalidParameter(
                "at least one of title_zh or title_en must be non-empty".into(),
            ));
        }
        self.repo.create(&input).await
    }

    pub(crate) async fn delete(&self, id: i64) -> AppResult<()> {
        self.repo.delete(id).await
    }

    pub(crate) async fn candidates(&self, query: &str) -> AppResult<Vec<SubscriptionCandidate>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let (movies, tvs) = tokio::join!(
            self.metadata_catalog.search_movie(query, ""),
            self.metadata_catalog.search_tv(query, "")
        );

        let movies = movies?;
        let tvs = tvs?;

        let mut candidates = Vec::with_capacity(movies.len() + tvs.len());

        for m in movies {
            candidates.push(SubscriptionCandidate {
                tmdb_id: m.id,
                media_type: SubscriptionMediaType::Movie,
                title: m.title,
                original_title: m.original_title,
            });
        }

        for t in tvs {
            candidates.push(SubscriptionCandidate {
                tmdb_id: t.id,
                media_type: SubscriptionMediaType::Tv,
                title: t.name,
                original_title: t.original_name,
            });
        }

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::import::{SearchMovieResult, SearchTvResult};
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeRepo {
        records: Arc<Mutex<Vec<SubscriptionRecord>>>,
    }

    impl SubscriptionRepository for FakeRepo {
        async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
            Ok(self.records.lock().unwrap().clone())
        }
        async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>> {
            Ok(self.records.lock().unwrap().iter().find(|r| r.id == id).cloned())
        }
        async fn find_by_tmdb_id(
            &self,
            tmdb_id: u32,
            media_type: &SubscriptionMediaType,
        ) -> AppResult<Option<SubscriptionRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.tmdb_id == tmdb_id && r.media_type == *media_type)
                .cloned())
        }
        async fn create(&self, input: &SubscriptionCreateInput) -> AppResult<i64> {
            let mut records = self.records.lock().unwrap();
            let id = records.len() as i64 + 1;
            records.push(SubscriptionRecord {
                id,
                tmdb_id: input.tmdb_id,
                media_type: input.media_type,
                title_zh: input.title_zh.clone(),
                title_en: input.title_en.clone(),
                create_time: Utc::now(),
                update_time: Utc::now(),
            });
            Ok(id)
        }
        async fn delete(&self, id: i64) -> AppResult<()> {
            self.records.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeCatalog;

    impl MetadataCatalog for FakeCatalog {
        async fn search_movie(&self, title: &str, _year: &str) -> AppResult<Vec<SearchMovieResult>> {
            if title == "Inception" {
                Ok(vec![SearchMovieResult {
                    id: 27205,
                    title: "Inception".into(),
                    original_title: "Inception".into(),
                }])
            } else {
                Ok(vec![])
            }
        }
        async fn get_movie_detail(&self, _id: u32) -> AppResult<Option<crate::domain::import::MovieDetail>> {
            Ok(None)
        }
        async fn search_tv(&self, title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
            if title == "Breaking" {
                Ok(vec![SearchTvResult {
                    id: 1396,
                    name: "Breaking Bad".into(),
                    original_name: "Breaking Bad".into(),
                }])
            } else {
                Ok(vec![])
            }
        }
        async fn get_tv_detail(&self, _id: u32) -> AppResult<Option<crate::domain::import::TvDetail>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn create_rejects_both_titles_empty() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let err = svc
            .create(SubscriptionCreateInput {
                tmdb_id: 1,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn create_rejects_empty_string_titles() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let err = svc
            .create(SubscriptionCreateInput {
                tmdb_id: 1,
                media_type: SubscriptionMediaType::Movie,
                title_zh: Some("".into()),
                title_en: Some("".into()),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn create_accepts_valid_input() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let id = svc
            .create(SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: Some("盗梦空间".into()),
                title_en: Some("Inception".into()),
            })
            .await
            .unwrap();
        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn list_returns_all_records() {
        let repo = FakeRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: None,
            title_en: Some("Test".into()),
        })
        .await
        .unwrap();
        let svc = ManageSubscriptionsService::new(repo, FakeCatalog);
        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn delete_removes_record() {
        let repo = FakeRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: None,
            title_en: Some("Test".into()),
        })
        .await
        .unwrap();
        let svc = ManageSubscriptionsService::new(repo.clone(), FakeCatalog);
        svc.delete(1).await.unwrap();
        assert!(repo.list_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn candidates_merges_movie_and_tv_results() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let candidates = svc.candidates("Inception").await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].tmdb_id, 27205);
        assert_eq!(candidates[0].media_type, SubscriptionMediaType::Movie);
    }

    #[tokio::test]
    async fn candidates_returns_empty_for_no_match() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let candidates = svc.candidates("Nonexistent").await.unwrap();
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn candidates_returns_empty_for_empty_query() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog);
        let candidates = svc.candidates("  ").await.unwrap();
        assert!(candidates.is_empty());
    }
}
