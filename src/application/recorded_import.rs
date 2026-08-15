use chrono::Utc;

use crate::{
    application::{
        import::ImportedMedia,
        ports::{
            ImportRecordCreate, ImportRecordFinalize, ImportRecordRepo, ImportRecordRepository,
        },
    },
    domain::import_record::{ImportOutcome, ImportSource, ImportStatus, summarize},
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct RecordedImportService {
    repo: ImportRecordRepo,
}

impl RecordedImportService {
    pub fn new(repo: impl ImportRecordRepository + 'static) -> Self {
        Self {
            repo: std::sync::Arc::new(repo),
        }
    }

    pub async fn execute<F, Fut>(
        &self,
        source: ImportSource,
        run: F,
    ) -> AppResult<Vec<ImportedMedia>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<Vec<ImportedMedia>>>,
    {
        let id = self
            .repo
            .create(&ImportRecordCreate {
                source_kind: source.kind,
                source: source.raw,
                created_at: Utc::now(),
            })
            .await?;

        match run().await {
            Ok(items) => {
                let outcomes: Vec<ImportOutcome> = items.iter().map(import_outcome_from).collect();
                let (summary, status) = summarize(&outcomes);
                let summary_json = serde_json::to_string(&summary).map_err(|e| {
                    AppError::Internal(format!("failed to serialize import summary: {e}"))
                })?;
                self.repo
                    .finalize(
                        id,
                        &ImportRecordFinalize {
                            status,
                            summary_json,
                            error_kind: None,
                            error_message: None,
                            finished_at: Utc::now(),
                        },
                    )
                    .await?;
                Ok(items)
            }
            Err(err) => {
                let finalize = ImportRecordFinalize {
                    status: ImportStatus::Failed,
                    summary_json: "{}".to_owned(),
                    error_kind: Some(classify_error(&err).to_owned()),
                    error_message: Some(err.to_string()),
                    finished_at: Utc::now(),
                };
                if let Err(write_err) = self.repo.finalize(id, &finalize).await {
                    tracing::warn!(error = %write_err, "failed to record import failure");
                }
                Err(err)
            }
        }
    }
}

fn classify_error(err: &AppError) -> &'static str {
    match err {
        AppError::InvalidParameter(_) => "invalid_parameter",
        AppError::NotFound(_) => "not_found",
        AppError::Unauthorized(_) => "unauthorized",
        AppError::Database(_, _) => "database",
        AppError::ExternalService(_, _) => "external_service",
        AppError::Network(_, _) => "network",
        AppError::Internal(_) => "internal",
    }
}

fn import_outcome_from(media: &ImportedMedia) -> ImportOutcome {
    match media {
        ImportedMedia::Movie {
            title,
            year,
            size,
            cost,
            has_failed,
        } => ImportOutcome::Movie {
            title: title.clone(),
            year: year.clone(),
            size: *size,
            cost: *cost,
            has_failed: *has_failed,
        },
        ImportedMedia::Tv {
            name,
            year,
            season,
            episodes,
            missing_episodes,
            max_episode_number,
            total_size,
            number_of_episodes,
            cost,
            has_failed,
            failed_episodes,
        } => ImportOutcome::Tv {
            name: name.clone(),
            year: year.clone(),
            season: *season,
            episodes: episodes.clone(),
            missing_episodes: missing_episodes.clone(),
            failed_episodes: failed_episodes.clone(),
            max_episode_number: *max_episode_number,
            number_of_episodes: *number_of_episodes,
            total_size: *total_size,
            cost: *cost,
            has_failed: *has_failed,
        },
        ImportedMedia::Skipped { files, .. } => ImportOutcome::Skipped {
            files: files.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::application::ports::{
        ImportRecordFilter, ImportRecordPage, ImportRecordPaging, ImportRecordView,
    };
    use crate::domain::import_record::ImportSourceKind;

    #[derive(Clone, Default)]
    struct FakeRepo {
        created: Arc<Mutex<Vec<ImportRecordCreate>>>,
        finalized: Arc<Mutex<Vec<(i64, ImportRecordFinalize)>>>,
    }

    #[async_trait::async_trait]
    impl ImportRecordRepository for FakeRepo {
        async fn create(&self, input: &ImportRecordCreate) -> AppResult<i64> {
            let mut created = self.created.lock().unwrap();
            created.push(input.clone());
            Ok(created.len() as i64)
        }

        async fn finalize(&self, id: i64, update: &ImportRecordFinalize) -> AppResult<()> {
            self.finalized.lock().unwrap().push((id, update.clone()));
            Ok(())
        }

        async fn get(&self, _id: i64) -> AppResult<Option<ImportRecordView>> {
            unimplemented!()
        }

        async fn list(
            &self,
            _filter: &ImportRecordFilter,
            _paging: ImportRecordPaging,
        ) -> AppResult<ImportRecordPage> {
            unimplemented!()
        }
    }

    fn source() -> ImportSource {
        ImportSource {
            kind: ImportSourceKind::Pan189,
            raw: "https://cloud.189.cn/t/abc".into(),
        }
    }

    #[tokio::test]
    async fn creates_running_record_before_executing_and_finalizes_on_success() {
        let repo = FakeRepo::default();
        let service = RecordedImportService::new(repo.clone());

        let result = service
            .execute(source(), || async {
                Ok(vec![ImportedMedia::Movie {
                    title: "Movie".into(),
                    year: "2024".into(),
                    size: 100,
                    cost: Duration::from_secs(1),
                    has_failed: false,
                }])
            })
            .await
            .unwrap();

        assert_eq!(result.len(), 1);

        let created = repo.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].source_kind, ImportSourceKind::Pan189);
        assert_eq!(created[0].source, "https://cloud.189.cn/t/abc");

        let finalized = repo.finalized.lock().unwrap().clone();
        assert_eq!(finalized.len(), 1);
        assert_eq!(finalized[0].0, 1);
        assert_eq!(finalized[0].1.status, ImportStatus::Succeeded);
        assert!(finalized[0].1.summary_json.contains("Movie"));
    }

    #[tokio::test]
    async fn finalizes_as_failed_with_error_classification_when_run_fails() {
        let repo = FakeRepo::default();
        let service = RecordedImportService::new(repo.clone());

        let err = service
            .execute(source(), || async {
                Err::<Vec<ImportedMedia>, _>(AppError::Network("timeout".into(), true))
            })
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::Network(_, _)));

        let finalized = repo.finalized.lock().unwrap().clone();
        assert_eq!(finalized.len(), 1);
        assert_eq!(finalized[0].1.status, ImportStatus::Failed);
        assert_eq!(finalized[0].1.error_kind.as_deref(), Some("network"));
        assert!(
            finalized[0]
                .1
                .error_message
                .as_deref()
                .unwrap()
                .contains("timeout")
        );
    }

    #[tokio::test]
    async fn finalizes_as_partially_failed_when_some_episodes_failed() {
        let repo = FakeRepo::default();
        let service = RecordedImportService::new(repo.clone());

        service
            .execute(source(), || async {
                Ok(vec![ImportedMedia::Tv {
                    name: "Show".into(),
                    year: "2025".into(),
                    season: 1,
                    episodes: vec![1, 2],
                    missing_episodes: vec![],
                    max_episode_number: 3,
                    total_size: 0,
                    number_of_episodes: 3,
                    cost: Duration::from_secs(5),
                    has_failed: true,
                    failed_episodes: vec![3],
                }])
            })
            .await
            .unwrap();

        let finalized = repo.finalized.lock().unwrap().clone();
        assert_eq!(finalized[0].1.status, ImportStatus::PartiallyFailed);
    }

    #[tokio::test]
    async fn finalizes_as_skipped_when_no_outcomes_returned() {
        let repo = FakeRepo::default();
        let service = RecordedImportService::new(repo.clone());

        service
            .execute(source(), || async { Ok(Vec::<ImportedMedia>::new()) })
            .await
            .unwrap();

        let finalized = repo.finalized.lock().unwrap().clone();
        assert_eq!(finalized[0].1.status, ImportStatus::Skipped);
    }
}
