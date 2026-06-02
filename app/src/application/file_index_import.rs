use std::collections::HashSet;

use crate::{
    application::{
        file_index::FileIndexService,
        import::{ImportedMedia, MetadataLookup, identify::IdentifyOutcome},
        import_ports::{MediaIdentifier, MediaImporter},
        ports::{FileIndexRepository, ImportRecordRepository},
        recorded_import::RecordedImportService,
    },
    domain::{
        import::inner::MediaFile,
        import_record::{ImportSource, ImportSourceKind},
    },
    error::{AppError, AppResult},
};

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportFileResult {
    pub id: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ImportFileResult {
    fn skipped(id: i64, reason: &str) -> Self {
        Self {
            id,
            status: "skipped".into(),
            error: Some(reason.into()),
            ..Default::default()
        }
    }

    fn failed(id: i64, message: String) -> Self {
        Self {
            id,
            status: "failed".into(),
            error: Some(message),
            ..Default::default()
        }
    }

    fn from_imported(id: i64, item: &ImportedMedia) -> Self {
        match item {
            ImportedMedia::Movie {
                title, year, size, ..
            } => Self {
                id,
                status: "succeeded".into(),
                title: Some(title.clone()),
                year: Some(year.clone()),
                size: Some(*size),
                error: None,
            },
            ImportedMedia::Tv { name, year, .. } => Self {
                id,
                status: "succeeded".into(),
                title: Some(name.clone()),
                year: Some(year.clone()),
                size: None,
                error: None,
            },
            ImportedMedia::Skipped { .. } => Self {
                id,
                status: "skipped".into(),
                title: Some("skipped".into()),
                year: Some(String::new()),
                size: None,
                error: None,
            },
        }
    }
}

pub struct FileIndexImportService<R> {
    file_index: FileIndexService<R>,
}

impl<R: FileIndexRepository> FileIndexImportService<R> {
    pub fn new(file_index: FileIndexService<R>) -> Self {
        Self { file_index }
    }

    pub async fn import_from_fingerprints<I, D, RecordRepo>(
        &self,
        ids: &[i64],
        identifier: &mut D,
        importer: &mut I,
        recorded: &RecordedImportService<RecordRepo>,
    ) -> AppResult<Vec<ImportFileResult>>
    where
        I: MediaImporter,
        D: MediaIdentifier,
        RecordRepo: ImportRecordRepository,
    {
        let ready_files = self.file_index.get_import_ready_files(ids).await?;

        let found_ids: HashSet<i64> = ready_files.iter().map(|(id, _, _)| *id).collect();
        let missing_ids: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| !found_ids.contains(id))
            .collect();

        if ready_files.is_empty() {
            return Err(AppError::InvalidParameter(
                "no valid files found for the given ids".into(),
            ));
        }

        let mut results = Vec::new();

        for missing_id in missing_ids {
            results.push(ImportFileResult::skipped(
                missing_id,
                "not found or unsupported hash type",
            ));
        }

        for (file_id, raw_file, descriptions) in ready_files {
            let source = ImportSource {
                kind: ImportSourceKind::FileIndex,
                raw: format!("file_index:{}:{}", file_id, raw_file.name),
            };

            let raw_files = vec![raw_file];

            let outcome = recorded
                .execute(source, || async {
                    let mut metadata_lookup = MetadataLookup::default();
                    let media_files =
                        metadata_lookup.build_media_files(raw_files.clone(), descriptions);
                    let identified = identifier.identify(media_files).await?;
                    importer
                        .import_groups(identified.groups, identified.unmatched)
                        .await
                })
                .await;

            match outcome {
                Ok(imported) => {
                    for item in &imported {
                        results.push(ImportFileResult::from_imported(file_id, item));
                    }
                    if imported.is_empty() {
                        results.push(ImportFileResult::skipped(file_id, "no media matched"));
                    }
                }
                Err(err) => {
                    tracing::warn!(file_id, error = %err, "import from file index failed");
                    results.push(ImportFileResult::failed(file_id, err.to_string()));
                }
            }
        }

        Ok(results)
    }
}

impl<M, T> MediaIdentifier for crate::application::import::identify::MediaIdentifyService<M, T>
where
    M: crate::application::import_ports::MetadataCatalog + Send + Sync + 'static,
    T: crate::application::import_ports::TitleExtractor + Send + Sync + 'static,
{
    async fn identify(&mut self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
        self.identify(files).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        import::ImportedMedia,
        ports::{
            FileIndexRecordInput, FileLocationRecord, FileSearchRecord, ImportRecordCreate,
            ImportRecordFilter, ImportRecordFinalize, ImportRecordPage, ImportRecordPaging,
            ImportRecordView,
        },
    };
    use crate::domain::import_record::ImportSourceKind;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct FakeFileRepo {
        records: Arc<Mutex<Vec<FileSearchRecord>>>,
    }

    impl FileIndexRepository for FakeFileRepo {
        async fn record_files(&self, _inputs: &[FileIndexRecordInput]) -> AppResult<()> {
            Ok(())
        }
        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(self.records.lock().unwrap().clone())
        }
        async fn get_records_by_ids(&self, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect())
        }
    }

    #[derive(Clone, Default)]
    struct FakeImportRepo {
        created: Arc<Mutex<Vec<ImportRecordCreate>>>,
        finalized: Arc<Mutex<Vec<(i64, ImportRecordFinalize)>>>,
    }

    impl ImportRecordRepository for FakeImportRepo {
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

    struct FakeIdentifier;

    impl MediaIdentifier for FakeIdentifier {
        async fn identify(
            &mut self,
            _files: Vec<crate::domain::import::inner::MediaFile>,
        ) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: vec![],
                unmatched: vec![],
            })
        }
    }

    struct FakeImporter {
        make_result: Box<dyn Fn() -> AppResult<Vec<ImportedMedia>> + Send>,
    }

    impl MediaImporter for FakeImporter {
        async fn import_groups(
            &mut self,
            _groups: Vec<crate::domain::import::inner::Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            (self.make_result)()
        }
    }

    fn sample_record(id: i64) -> FileSearchRecord {
        FileSearchRecord {
            id,
            size: 1000,
            hash_type: "md5".into(),
            hash_value: "abcdef".into(),
            locations: vec![FileLocationRecord {
                file_name: format!("Movie.{id}.1080p.mkv"),
                file_path: format!("/movies/{id}"),
                descriptions: vec!["desc1".into()],
            }],
        }
    }

    fn service_with_records(
        records: Vec<FileSearchRecord>,
    ) -> FileIndexImportService<FakeFileRepo> {
        let repo = FakeFileRepo {
            records: Arc::new(Mutex::new(records)),
        };
        FileIndexImportService::new(FileIndexService::new(repo))
    }

    fn movie_importer() -> FakeImporter {
        FakeImporter {
            make_result: Box::new(|| {
                Ok(vec![ImportedMedia::Movie {
                    title: "Movie".into(),
                    year: "2024".into(),
                    size: 1_000_000,
                    cost: Duration::from_secs(1),
                    has_failed: false,
                }])
            }),
        }
    }

    #[tokio::test]
    async fn empty_ids_returns_error() {
        let service = service_with_records(vec![]);
        let mut importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let err = service
            .import_from_fingerprints(&[], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn no_matching_records_returns_error() {
        let service = service_with_records(vec![]);
        let mut importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let err = service
            .import_from_fingerprints(&[999], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn successful_movie_import() {
        let service = service_with_records(vec![sample_record(1)]);
        let mut importer = movie_importer();
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].status, "succeeded");
        assert_eq!(results[0].title.as_deref(), Some("Movie"));
        assert_eq!(results[0].year.as_deref(), Some("2024"));
        assert_eq!(results[0].size, Some(1_000_000));
    }

    #[tokio::test]
    async fn skipped_item_reports_skipped_status() {
        let service = service_with_records(vec![sample_record(1)]);
        let mut importer = FakeImporter {
            make_result: Box::new(|| {
                Ok(vec![ImportedMedia::Skipped {
                    count: 1,
                    files: vec!["test.mkv".into()],
                }])
            }),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
    }

    #[tokio::test]
    async fn empty_import_result_reports_skipped() {
        let service = service_with_records(vec![sample_record(1)]);
        let mut importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
        assert_eq!(results[0].error.as_deref(), Some("no media matched"));
    }

    #[tokio::test]
    async fn transfer_error_reports_failed() {
        let service = service_with_records(vec![sample_record(1)]);
        let mut importer = FakeImporter {
            make_result: Box::new(|| Err(AppError::Network("timeout".into(), true))),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].status, "failed");
        assert!(results[0].error.as_deref().unwrap().contains("timeout"));
    }

    #[tokio::test]
    async fn batch_import_processes_multiple_fingerprints() {
        let service = service_with_records(vec![sample_record(1), sample_record(2)]);
        let mut importer = movie_importer();
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1, 2], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[1].id, 2);
    }

    #[tokio::test]
    async fn creates_import_record_per_fingerprint() {
        let service = service_with_records(vec![sample_record(1), sample_record(2)]);
        let mut importer = movie_importer();
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        service
            .import_from_fingerprints(&[1, 2], &mut FakeIdentifier, &mut importer, &recorded)
            .await
            .unwrap();

        let created = import_repo.created.lock().unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].source_kind, ImportSourceKind::FileIndex);
        assert_eq!(created[1].source_kind, ImportSourceKind::FileIndex);
    }
}
