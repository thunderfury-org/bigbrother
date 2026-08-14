use tracing::{info, warn};

use crate::{
    application::{
        file_index::FileIndexService,
        import::ImportedMedia,
        import::MetadataLookup,
        import_ports::{MediaIdentifier, MediaImporter},
        ports::{FileIndexRepository, ImportRecordRepository, SubscriptionRepository},
        recorded_import::RecordedImportService,
        subscription::import_filter,
    },
    domain::{import_record::ImportSource, share::RawFile},
    error::AppResult,
};

#[derive(Debug, Clone)]
pub struct MediaSourceObservation {
    pub import_source: ImportSource,
    pub description: Option<String>,
    pub channel_post: bool,
    pub raw_files: Vec<RawFile>,
}

#[derive(Debug, Clone)]
pub enum ObservationNotice {
    ImportResults(Vec<ImportedMedia>),
    PermanentError { error: crate::error::AppError },
}

#[derive(Debug, Clone)]
pub struct ObservationOutcome {
    pub notice: Option<ObservationNotice>,
}

#[derive(Clone)]
pub struct ProcessObservationService<F, R, I, M, S> {
    file_index: FileIndexService<F>,
    recorded_import: RecordedImportService<R>,
    identify: I,
    import: M,
    subscriptions: S,
}

impl<F, R, I, M, S> ProcessObservationService<F, R, I, M, S> {
    pub fn new(
        file_index: FileIndexService<F>,
        recorded_import: RecordedImportService<R>,
        identify: I,
        import: M,
        subscriptions: S,
    ) -> Self {
        Self {
            file_index,
            recorded_import,
            identify,
            import,
            subscriptions,
        }
    }
}

impl<F, R, I, M, S> ProcessObservationService<F, R, I, M, S>
where
    F: FileIndexRepository,
    R: ImportRecordRepository,
    I: MediaIdentifier + Clone,
    M: MediaImporter + Clone,
    S: SubscriptionRepository,
{
    pub async fn process(
        &self,
        observation: MediaSourceObservation,
    ) -> AppResult<ObservationOutcome> {
        let source_kind = observation.import_source.kind.as_str();
        if let Err(err) = self
            .file_index
            .record_raw_files(
                observation.raw_files.clone(),
                observation.description.clone(),
            )
            .await
        {
            warn!(error = %err, "file index record failed (non-blocking)");
        } else {
            info!(
                source_kind,
                raw_file_count = observation.raw_files.len(),
                "Recorded raw files into file index"
            );
        }

        let should_import = should_import(
            &self.subscriptions,
            observation.channel_post,
            &observation.description,
        )
        .await;
        info!(
            source_kind,
            should_import,
            channel_post = observation.channel_post,
            "Evaluated import policy for media source observation"
        );
        if !should_import {
            return Ok(ObservationOutcome { notice: None });
        }

        let descriptions: Vec<String> = observation.description.into_iter().collect();
        let media_files =
            MetadataLookup::default().build_media_files(observation.raw_files, descriptions);
        info!(
            source_kind,
            media_file_count = media_files.len(),
            "Built media files for import"
        );

        let is_channel_post = observation.channel_post;
        let mut identify = self.identify.clone();
        let mut import = self.import.clone();
        let subscriptions = self.subscriptions.clone();
        let outcome = self
            .recorded_import
            .execute(observation.import_source, move || async move {
                let identified = identify.identify(media_files).await?;
                let groups = if is_channel_post {
                    import_filter::filter_by_subscription(&subscriptions, identified.groups).await
                } else {
                    identified.groups
                };
                import.import_groups(groups, identified.unmatched).await
            })
            .await;

        match outcome {
            Ok(imported) => {
                info!(
                    source_kind,
                    imported_summary_count = imported.len(),
                    "Import completed for media source observation"
                );
                Ok(ObservationOutcome {
                    notice: Some(ObservationNotice::ImportResults(imported)),
                })
            }
            Err(err) if !err.is_retryable() => Ok(ObservationOutcome {
                notice: Some(ObservationNotice::PermanentError { error: err }),
            }),
            Err(err) => Err(err),
        }
    }
}

async fn should_import<R: SubscriptionRepository>(
    subscription_repo: &R,
    channel_post: bool,
    description: &Option<String>,
) -> bool {
    if !channel_post {
        return true;
    }
    let text = description.as_deref().unwrap_or_default();
    import_filter::description_matches_subscription(subscription_repo, text).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        import::identify::{IdentifyOutcome, UnmatchedFile},
        ports::{
            FileIndexRecordInput, FileSearchRecord, ImportRecordCreate, ImportRecordFilter,
            ImportRecordFinalize, ImportRecordPage, ImportRecordPaging, ImportRecordView,
            SubscriptionCreateInput, SubscriptionRecord,
        },
    };
    use crate::domain::import::inner::{Media, MediaFile};
    use crate::domain::import_record::ImportSourceKind;
    use crate::domain::share::FileHash;
    use crate::domain::subscription::SubscriptionMediaType;
    use crate::error::AppError;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct FakeFileRepo {
        recorded: Arc<Mutex<Vec<Vec<FileIndexRecordInput>>>>,
        fail: bool,
    }

    impl FileIndexRepository for FakeFileRepo {
        async fn record_files(&self, inputs: &[FileIndexRecordInput]) -> AppResult<()> {
            if self.fail {
                return Err(AppError::Database("index write failed".into(), false));
            }
            self.recorded.lock().unwrap().push(inputs.to_vec());
            Ok(())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            unimplemented!()
        }

        async fn get_records_by_ids(&self, _ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
            unimplemented!()
        }
    }

    #[derive(Clone, Default)]
    struct FakeImportRepo {
        created: Arc<Mutex<Vec<ImportRecordCreate>>>,
    }

    impl ImportRecordRepository for FakeImportRepo {
        async fn create(&self, input: &ImportRecordCreate) -> AppResult<i64> {
            let mut created = self.created.lock().unwrap();
            created.push(input.clone());
            Ok(created.len() as i64)
        }

        async fn finalize(&self, _id: i64, _update: &ImportRecordFinalize) -> AppResult<()> {
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

    #[derive(Clone)]
    struct FakeIdentifier;

    impl MediaIdentifier for FakeIdentifier {
        async fn identify(&mut self, _files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: Vec::new(),
                unmatched: Vec::new(),
            })
        }
    }

    #[derive(Clone)]
    struct FakeImporter {
        calls: Arc<Mutex<usize>>,
        result: AppResult<Vec<ImportedMedia>>,
    }

    impl Default for FakeImporter {
        fn default() -> Self {
            Self {
                calls: Arc::new(Mutex::new(0)),
                result: Ok(vec![sample_imported_movie()]),
            }
        }
    }

    impl MediaImporter for FakeImporter {
        async fn import_groups(
            &mut self,
            _groups: Vec<Media>,
            _unmatched: Vec<UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            *self.calls.lock().unwrap() += 1;
            self.result.clone()
        }
    }

    #[derive(Clone, Default)]
    struct FakeSubscriptionRepo {
        records: Arc<Mutex<Vec<SubscriptionRecord>>>,
    }

    impl FakeSubscriptionRepo {
        fn with_title(title_en: &str) -> Self {
            let repo = Self::default();
            repo.records.lock().unwrap().push(SubscriptionRecord {
                id: 1,
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some(title_en.into()),
                create_time: Utc::now(),
                update_time: Utc::now(),
            });
            repo
        }
    }

    impl SubscriptionRepository for FakeSubscriptionRepo {
        async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
            Ok(self.records.lock().unwrap().clone())
        }

        async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|record| record.id == id)
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
                .find(|record| record.tmdb_id == tmdb_id && record.media_type == *media_type)
                .cloned())
        }

        async fn create(&self, _input: &SubscriptionCreateInput) -> AppResult<i64> {
            unimplemented!()
        }

        async fn delete(&self, _id: i64) -> AppResult<()> {
            unimplemented!()
        }
    }

    fn sample_raw_file() -> RawFile {
        RawFile {
            id: None,
            name: "Inception.2010.1080p.mkv".into(),
            hash: FileHash::Md5("a".repeat(32)),
            size: 1000,
            path: "/share".into(),
        }
    }

    fn sample_imported_movie() -> ImportedMedia {
        ImportedMedia::Movie {
            title: "Inception".into(),
            year: "2010".into(),
            size: 1000,
            cost: Duration::from_secs(1),
            has_failed: false,
        }
    }

    fn observation(description: Option<String>, channel_post: bool) -> MediaSourceObservation {
        MediaSourceObservation {
            import_source: ImportSource {
                kind: ImportSourceKind::Pan115,
                raw: "https://115.com/s/share-id?rc=abc".into(),
            },
            description,
            channel_post,
            raw_files: vec![sample_raw_file()],
        }
    }

    fn service(
        file_index: FakeFileRepo,
        import_repo: FakeImportRepo,
        importer: FakeImporter,
        subscriptions: FakeSubscriptionRepo,
    ) -> ProcessObservationService<
        FakeFileRepo,
        FakeImportRepo,
        FakeIdentifier,
        FakeImporter,
        FakeSubscriptionRepo,
    > {
        ProcessObservationService::new(
            FileIndexService::new(file_index),
            RecordedImportService::new(import_repo),
            FakeIdentifier,
            importer,
            subscriptions,
        )
    }

    fn assert_import_results(outcome: ObservationOutcome) {
        match outcome.notice {
            Some(ObservationNotice::ImportResults(items)) => {
                assert_eq!(items, vec![sample_imported_movie()]);
            }
            other => panic!("expected import results, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn imports_raw_files_for_direct_message() {
        let import_repo = FakeImportRepo::default();
        let created = import_repo.created.clone();
        let importer = FakeImporter::default();
        let import_calls = importer.calls.clone();

        let outcome = service(
            FakeFileRepo::default(),
            import_repo,
            importer,
            FakeSubscriptionRepo::default(),
        )
        .process(observation(Some("Inception 2010".into()), false))
        .await
        .unwrap();

        assert_import_results(outcome);
        assert_eq!(*import_calls.lock().unwrap(), 1);
        assert_eq!(
            created.lock().unwrap()[0].source_kind,
            ImportSourceKind::Pan115
        );
        assert_eq!(
            created.lock().unwrap()[0].source,
            "https://115.com/s/share-id?rc=abc"
        );
    }

    #[tokio::test]
    async fn file_index_failure_does_not_block_import() {
        let file_index = FakeFileRepo {
            recorded: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        };
        let importer = FakeImporter::default();
        let import_calls = importer.calls.clone();

        let outcome = service(
            file_index,
            FakeImportRepo::default(),
            importer,
            FakeSubscriptionRepo::default(),
        )
        .process(observation(Some("Inception 2010".into()), false))
        .await
        .unwrap();

        assert_import_results(outcome);
        assert_eq!(*import_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn channel_post_without_subscription_match_skips_import() {
        let importer = FakeImporter::default();
        let import_calls = importer.calls.clone();
        let file_index = FakeFileRepo::default();
        let recorded = file_index.recorded.clone();

        let outcome = service(
            file_index,
            FakeImportRepo::default(),
            importer,
            FakeSubscriptionRepo::with_title("Inception"),
        )
        .process(observation(Some("Breaking Bad S01".into()), true))
        .await
        .unwrap();

        assert!(outcome.notice.is_none());
        assert_eq!(*import_calls.lock().unwrap(), 0);
        assert_eq!(recorded.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn channel_post_with_matching_title_imports() {
        let importer = FakeImporter::default();
        let import_calls = importer.calls.clone();

        let outcome = service(
            FakeFileRepo::default(),
            FakeImportRepo::default(),
            importer,
            FakeSubscriptionRepo::with_title("Inception"),
        )
        .process(observation(Some("分享：Inception 2010".into()), true))
        .await
        .unwrap();

        assert_import_results(outcome);
        assert_eq!(*import_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn permanent_import_error_returns_notice() {
        let importer = FakeImporter {
            calls: Arc::new(Mutex::new(0)),
            result: Err(AppError::InvalidParameter("library rejected".into())),
        };

        let outcome = service(
            FakeFileRepo::default(),
            FakeImportRepo::default(),
            importer,
            FakeSubscriptionRepo::default(),
        )
        .process(observation(None, false))
        .await
        .unwrap();

        match outcome.notice {
            Some(ObservationNotice::PermanentError { error }) => {
                assert!(error.to_string().contains("library rejected"));
            }
            other => panic!("expected permanent error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retryable_import_error_bubbles_up() {
        let importer = FakeImporter {
            calls: Arc::new(Mutex::new(0)),
            result: Err(AppError::Network("import timeout".into(), true)),
        };

        let err = service(
            FakeFileRepo::default(),
            FakeImportRepo::default(),
            importer,
            FakeSubscriptionRepo::default(),
        )
        .process(observation(None, false))
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::Network(_, true)));
    }
}
