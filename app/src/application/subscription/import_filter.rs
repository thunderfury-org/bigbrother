use crate::application::ports::SubscriptionRepository;
use crate::domain::{import::inner::Media, subscription::SubscriptionMediaType};

pub(crate) async fn description_matches_subscription<R: SubscriptionRepository>(
    repo: &R,
    description: &str,
) -> bool {
    let subscriptions = match repo.list_all().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load subscriptions for prefilter");
            return false;
        }
    };
    if subscriptions.is_empty() {
        return false;
    }
    subscriptions.iter().any(|s| {
        s.title_zh
            .as_deref()
            .is_some_and(|t| !t.is_empty() && description.contains(t))
            || s.title_en
                .as_deref()
                .is_some_and(|t| !t.is_empty() && description.contains(t))
    })
}

pub(crate) async fn filter_by_subscription<R: SubscriptionRepository>(
    repo: &R,
    groups: Vec<Media>,
) -> Vec<Media> {
    let mut filtered = Vec::new();
    for media in groups {
        let (tmdb_id, media_type) = match &media {
            Media::Movie { detail, .. } => (detail.id, SubscriptionMediaType::Movie),
            Media::Tv { detail, .. } => (detail.id, SubscriptionMediaType::Tv),
        };
        match repo.find_by_tmdb_id(tmdb_id, &media_type).await {
            Ok(Some(_)) => filtered.push(media),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(tmdb_id, error = %e, "subscription lookup failed, skipping media");
            }
        }
    }
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{SubscriptionCreateInput, SubscriptionRecord};
    use crate::domain::share::RawFile;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSubscriptionRepo {
        records: Arc<Mutex<Vec<SubscriptionRecord>>>,
    }

    impl SubscriptionRepository for FakeSubscriptionRepo {
        async fn list_all(&self) -> crate::error::AppResult<Vec<SubscriptionRecord>> {
            Ok(self.records.lock().unwrap().clone())
        }
        async fn get_by_id(&self, id: i64) -> crate::error::AppResult<Option<SubscriptionRecord>> {
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
        ) -> crate::error::AppResult<Option<SubscriptionRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.tmdb_id == tmdb_id && r.media_type == *media_type)
                .cloned())
        }
        async fn create(&self, input: &SubscriptionCreateInput) -> crate::error::AppResult<i64> {
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
        async fn delete(&self, id: i64) -> crate::error::AppResult<()> {
            self.records.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
    }

    fn make_movie(tmdb_id: u32) -> Media {
        Media::Movie {
            detail: crate::domain::import::MovieDetail {
                id: tmdb_id,
                title: "Test".into(),
                adult: false,
                genres: vec![],
                original_language: "en".into(),
                original_title: "Test".into(),
                origin_country: vec![],
                release_date: "2024-01-01".into(),
            },
            files: vec![crate::domain::import::inner::MediaFile {
                metadata: Box::new(crate::domain::media::Metadata::default()),
                video: RawFile {
                    id: None,
                    name: "test.mkv".into(),
                    hash: crate::domain::share::FileHash::Md5("hash".into()),
                    size: 100,
                    path: "/test".into(),
                },
                subtitles: vec![],
                descriptions: vec![],
            }],
        }
    }

    fn make_tv(tmdb_id: u32) -> Media {
        use std::collections::BTreeMap;
        Media::Tv {
            detail: crate::domain::import::TvDetail {
                id: tmdb_id,
                name: "Test Show".into(),
                first_air_date: "2024-01-01".into(),
                number_of_episodes: 10,
                number_of_seasons: 1,
                origin_country: vec![],
                original_language: "en".into(),
                original_name: "Test Show".into(),
                genres: vec![],
                seasons: vec![],
            },
            files: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn description_matches_returns_true_when_title_zh_matches() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: Some("盗梦空间".into()),
            title_en: Some("Inception".into()),
        })
        .await
        .unwrap();

        assert!(description_matches_subscription(&repo, "分享：盗梦空间 2010").await);
    }

    #[tokio::test]
    async fn description_matches_returns_true_when_title_en_matches() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: Some("盗梦空间".into()),
            title_en: Some("Inception".into()),
        })
        .await
        .unwrap();

        assert!(description_matches_subscription(&repo, "Inception 2010 1080p").await);
    }

    #[tokio::test]
    async fn description_matches_returns_false_when_no_match() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: Some("盗梦空间".into()),
            title_en: Some("Inception".into()),
        })
        .await
        .unwrap();

        assert!(!description_matches_subscription(&repo, "Breaking Bad S01").await);
    }

    #[tokio::test]
    async fn description_matches_returns_false_when_repo_empty() {
        let repo = FakeSubscriptionRepo::default();
        assert!(!description_matches_subscription(&repo, "anything").await);
    }

    #[tokio::test]
    async fn filter_keeps_matching_movie() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 27205,
            media_type: SubscriptionMediaType::Movie,
            title_zh: None,
            title_en: Some("Inception".into()),
        })
        .await
        .unwrap();

        let groups = vec![make_movie(27205), make_movie(99999)];
        let filtered = filter_by_subscription(&repo, groups).await;
        assert_eq!(filtered.len(), 1);
        match &filtered[0] {
            Media::Movie { detail, .. } => assert_eq!(detail.id, 27205),
            _ => panic!("expected movie"),
        }
    }

    #[tokio::test]
    async fn filter_keeps_matching_tv() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1396,
            media_type: SubscriptionMediaType::Tv,
            title_zh: None,
            title_en: Some("Breaking Bad".into()),
        })
        .await
        .unwrap();

        let groups = vec![make_tv(1396), make_movie(27205)];
        let filtered = filter_by_subscription(&repo, groups).await;
        assert_eq!(filtered.len(), 1);
        match &filtered[0] {
            Media::Tv { detail, .. } => assert_eq!(detail.id, 1396),
            _ => panic!("expected tv"),
        }
    }

    #[tokio::test]
    async fn filter_returns_empty_when_no_match() {
        let repo = FakeSubscriptionRepo::default();
        repo.create(&SubscriptionCreateInput {
            tmdb_id: 1,
            media_type: SubscriptionMediaType::Movie,
            title_zh: None,
            title_en: Some("Other".into()),
        })
        .await
        .unwrap();

        let groups = vec![make_movie(99999)];
        let filtered = filter_by_subscription(&repo, groups).await;
        assert!(filtered.is_empty());
    }
}
