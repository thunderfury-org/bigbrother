use crate::application::ports::{
    MetadataCatalog, MetadataCatalogHandle, SubscriptionCreateInput, SubscriptionRecord,
    SubscriptionRepo, SubscriptionRepository,
};
use crate::domain::subscription::SubscriptionMediaType;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub(crate) struct SubscriptionCandidate {
    pub tmdb_id: u32,
    pub media_type: SubscriptionMediaType,
    pub title: String,
    pub original_title: String,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ManageSubscriptionsService {
    repo: SubscriptionRepo,
    metadata_catalog: MetadataCatalogHandle,
}

impl ManageSubscriptionsService {
    pub(crate) fn new(
        repo: impl SubscriptionRepository + 'static,
        metadata_catalog: impl MetadataCatalog + 'static,
    ) -> Self {
        Self {
            repo: std::sync::Arc::new(repo),
            metadata_catalog: std::sync::Arc::new(metadata_catalog),
        }
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<SubscriptionRecord>> {
        let records = self.repo.list_all().await?;
        let mut out = Vec::with_capacity(records.len());
        for mut record in records {
            if needs_display_backfill(&record)
                && let Err(error) = self.backfill_display(&mut record).await
            {
                tracing::warn!(
                    error = %error,
                    tmdb_id = record.tmdb_id,
                    "subscription display backfill failed"
                );
            }
            out.push(record);
        }
        Ok(out)
    }

    pub(crate) async fn create(&self, mut input: SubscriptionCreateInput) -> AppResult<i64> {
        let has_title = input.title_zh.as_deref().is_some_and(|t| !t.is_empty())
            || input.title_en.as_deref().is_some_and(|t| !t.is_empty());
        if !has_title {
            return Err(AppError::InvalidParameter(
                "at least one of title_zh or title_en must be non-empty".into(),
            ));
        }
        input.year = normalize_optional(input.year);
        input.poster_path = normalize_optional(input.poster_path);
        input.overview = normalize_optional(input.overview);
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

        for movie in movies {
            candidates.push(SubscriptionCandidate {
                tmdb_id: movie.id,
                media_type: SubscriptionMediaType::Movie,
                title: movie.title,
                original_title: movie.original_title,
                year: year_from_date(&movie.release_date),
                poster_path: normalize_optional(movie.poster_path),
                overview: nonempty_text(movie.overview),
            });
        }

        for tv in tvs {
            candidates.push(SubscriptionCandidate {
                tmdb_id: tv.id,
                media_type: SubscriptionMediaType::Tv,
                title: tv.name,
                original_title: tv.original_name,
                year: year_from_date(&tv.first_air_date),
                poster_path: normalize_optional(tv.poster_path),
                overview: nonempty_text(tv.overview),
            });
        }

        Ok(candidates)
    }

    async fn backfill_display(&self, record: &mut SubscriptionRecord) -> AppResult<()> {
        let query = record
            .title_zh
            .as_deref()
            .filter(|title| !title.is_empty())
            .or(record.title_en.as_deref().filter(|title| !title.is_empty()))
            .unwrap_or("");
        if query.is_empty() {
            return Ok(());
        }

        let (year, poster_path, overview) = match record.media_type {
            SubscriptionMediaType::Movie => {
                let hits = self.metadata_catalog.search_movie(query, "").await?;
                let Some(hit) = hits.into_iter().find(|hit| hit.id == record.tmdb_id) else {
                    return Ok(());
                };
                (
                    year_from_date(&hit.release_date),
                    normalize_optional(hit.poster_path),
                    nonempty_text(hit.overview),
                )
            }
            SubscriptionMediaType::Tv => {
                let hits = self.metadata_catalog.search_tv(query, "").await?;
                let Some(hit) = hits.into_iter().find(|hit| hit.id == record.tmdb_id) else {
                    return Ok(());
                };
                (
                    year_from_date(&hit.first_air_date),
                    normalize_optional(hit.poster_path),
                    nonempty_text(hit.overview),
                )
            }
        };

        if year.is_none() && poster_path.is_none() && overview.is_none() {
            return Ok(());
        }

        self.repo
            .update_display(
                record.id,
                year.clone(),
                poster_path.clone(),
                overview.clone(),
            )
            .await?;
        record.year = year;
        record.poster_path = poster_path;
        record.overview = overview;
        Ok(())
    }
}

fn needs_display_backfill(record: &SubscriptionRecord) -> bool {
    record.year.is_none() && record.poster_path.is_none() && record.overview.is_none()
}

fn year_from_date(date: &str) -> Option<String> {
    let year = date.get(..4)?;
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
        Some(year.to_owned())
    } else {
        None
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(nonempty_text)
}

fn nonempty_text(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
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

    #[async_trait::async_trait]
    impl SubscriptionRepository for FakeRepo {
        async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
            Ok(self.records.lock().unwrap().clone())
        }
        async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
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
                year: input.year.clone(),
                poster_path: input.poster_path.clone(),
                overview: input.overview.clone(),
                create_time: Utc::now(),
                update_time: Utc::now(),
            });
            Ok(id)
        }
        async fn update_display(
            &self,
            id: i64,
            year: Option<String>,
            poster_path: Option<String>,
            overview: Option<String>,
        ) -> AppResult<()> {
            let mut records = self.records.lock().unwrap();
            if let Some(record) = records.iter_mut().find(|r| r.id == id) {
                record.year = year;
                record.poster_path = poster_path;
                record.overview = overview;
                record.update_time = Utc::now();
            }
            Ok(())
        }
        async fn delete(&self, id: i64) -> AppResult<()> {
            self.records.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FakeCatalog {
        movie_searches: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl MetadataCatalog for FakeCatalog {
        async fn search_movie(
            &self,
            title: &str,
            _year: &str,
        ) -> AppResult<Vec<SearchMovieResult>> {
            self.movie_searches.lock().unwrap().push(title.to_owned());
            if title == "Inception" {
                Ok(vec![SearchMovieResult {
                    id: 27205,
                    title: "Inception".into(),
                    original_title: "Inception".into(),
                    release_date: "2010-07-16".into(),
                    poster_path: Some("/inception.jpg".into()),
                    overview: "A thief who steals corporate secrets.".into(),
                }])
            } else {
                Ok(vec![])
            }
        }
        async fn get_movie_detail(
            &self,
            _id: u32,
        ) -> AppResult<Option<crate::domain::import::MovieDetail>> {
            Ok(None)
        }
        async fn search_tv(&self, title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
            if title == "Breaking" {
                Ok(vec![SearchTvResult {
                    id: 1396,
                    name: "Breaking Bad".into(),
                    original_name: "Breaking Bad".into(),
                    first_air_date: "2008-01-20".into(),
                    poster_path: Some("/breaking-bad.jpg".into()),
                    overview: "A chemistry teacher turned meth maker.".into(),
                }])
            } else {
                Ok(vec![])
            }
        }
        async fn get_tv_detail(
            &self,
            _id: u32,
        ) -> AppResult<Option<crate::domain::import::TvDetail>> {
            Ok(None)
        }
    }

    fn create_input(
        tmdb_id: u32,
        title_zh: Option<&str>,
        title_en: Option<&str>,
    ) -> SubscriptionCreateInput {
        SubscriptionCreateInput {
            tmdb_id,
            media_type: SubscriptionMediaType::Movie,
            title_zh: title_zh.map(str::to_owned),
            title_en: title_en.map(str::to_owned),
            year: None,
            poster_path: None,
            overview: None,
        }
    }

    #[tokio::test]
    async fn create_rejects_both_titles_empty() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let err = svc.create(create_input(1, None, None)).await.unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn create_rejects_empty_string_titles() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let err = svc
            .create(create_input(1, Some(""), Some("")))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn create_accepts_valid_input() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let id = svc
            .create(SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: Some("盗梦空间".into()),
                title_en: Some("Inception".into()),
                year: Some("2010".into()),
                poster_path: Some("/inception.jpg".into()),
                overview: Some("A thief who steals corporate secrets.".into()),
            })
            .await
            .unwrap();
        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn create_stores_display_snapshot() {
        let repo = FakeRepo::default();
        let svc = ManageSubscriptionsService::new(repo.clone(), FakeCatalog::default());
        svc.create(SubscriptionCreateInput {
            tmdb_id: 27205,
            media_type: SubscriptionMediaType::Movie,
            title_zh: Some("盗梦空间".into()),
            title_en: Some("Inception".into()),
            year: Some("2010".into()),
            poster_path: Some("/inception.jpg".into()),
            overview: Some("A thief who steals corporate secrets.".into()),
        })
        .await
        .unwrap();

        let stored = repo.list_all().await.unwrap();
        assert_eq!(stored[0].year.as_deref(), Some("2010"));
        assert_eq!(stored[0].poster_path.as_deref(), Some("/inception.jpg"));
        assert_eq!(
            stored[0].overview.as_deref(),
            Some("A thief who steals corporate secrets.")
        );
    }

    #[tokio::test]
    async fn list_returns_all_records() {
        let repo = FakeRepo::default();
        repo.create(&create_input(1, None, Some("Test")))
            .await
            .unwrap();
        let svc = ManageSubscriptionsService::new(repo, FakeCatalog::default());
        let list = svc.list().await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn list_backfills_missing_display_fields() {
        let repo = FakeRepo::default();
        repo.create(&create_input(27205, None, Some("Inception")))
            .await
            .unwrap();
        let catalog = FakeCatalog::default();
        let svc = ManageSubscriptionsService::new(repo.clone(), catalog.clone());

        let list = svc.list().await.unwrap();
        assert_eq!(list[0].year.as_deref(), Some("2010"));
        assert_eq!(list[0].poster_path.as_deref(), Some("/inception.jpg"));
        assert_eq!(
            list[0].overview.as_deref(),
            Some("A thief who steals corporate secrets.")
        );

        let stored = repo.list_all().await.unwrap();
        assert_eq!(stored[0].year.as_deref(), Some("2010"));

        svc.list().await.unwrap();
        assert_eq!(catalog.movie_searches.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn delete_removes_record() {
        let repo = FakeRepo::default();
        repo.create(&create_input(1, None, Some("Test")))
            .await
            .unwrap();
        let svc = ManageSubscriptionsService::new(repo.clone(), FakeCatalog::default());
        svc.delete(1).await.unwrap();
        assert!(repo.list_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn candidates_merges_movie_and_tv_results() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let candidates = svc.candidates("Inception").await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].tmdb_id, 27205);
        assert_eq!(candidates[0].media_type, SubscriptionMediaType::Movie);
        assert_eq!(candidates[0].year.as_deref(), Some("2010"));
        assert_eq!(candidates[0].poster_path.as_deref(), Some("/inception.jpg"));
        assert_eq!(
            candidates[0].overview.as_deref(),
            Some("A thief who steals corporate secrets.")
        );
    }

    #[tokio::test]
    async fn candidates_maps_tv_display_fields() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let candidates = svc.candidates("Breaking").await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].tmdb_id, 1396);
        assert_eq!(candidates[0].media_type, SubscriptionMediaType::Tv);
        assert_eq!(candidates[0].year.as_deref(), Some("2008"));
        assert_eq!(
            candidates[0].poster_path.as_deref(),
            Some("/breaking-bad.jpg")
        );
    }

    #[tokio::test]
    async fn candidates_returns_empty_for_no_match() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let candidates = svc.candidates("Nonexistent").await.unwrap();
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn candidates_returns_empty_for_empty_query() {
        let svc = ManageSubscriptionsService::new(FakeRepo::default(), FakeCatalog::default());
        let candidates = svc.candidates("  ").await.unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn year_from_date_reads_leading_year() {
        assert_eq!(year_from_date("2010-07-16").as_deref(), Some("2010"));
        assert_eq!(year_from_date(""), None);
        assert_eq!(year_from_date("tba"), None);
    }
}
