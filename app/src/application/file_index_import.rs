use crate::{
    application::{
        file_index::FileIndexService,
        import::MetadataLookup,
        import_ports::MediaImporter,
        ports::{FileIndexRepository, ImportRecordRepository},
        recorded_import::RecordedImportService,
    },
    domain::import_record::{ImportSource, ImportSourceKind},
    error::{AppError, AppResult},
};

#[derive(Debug, serde::Serialize)]
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

pub struct FileIndexImportService<R> {
    file_index: FileIndexService<R>,
}

impl<R: FileIndexRepository> FileIndexImportService<R> {
    pub fn new(file_index: FileIndexService<R>) -> Self {
        Self { file_index }
    }

    pub async fn import_from_fingerprints<I, RecordRepo>(
        &self,
        ids: &[i64],
        importer: &mut I,
        recorded: &RecordedImportService<RecordRepo>,
    ) -> AppResult<Vec<ImportFileResult>>
    where
        I: MediaImporter,
        RecordRepo: ImportRecordRepository,
    {
        let ready_files = self.file_index.get_import_ready_files(ids).await?;
        if ready_files.is_empty() {
            return Err(AppError::InvalidParameter(
                "no valid files found for the given ids".into(),
            ));
        }

        let mut results = Vec::new();
        for (file_id, raw_file, descriptions) in ready_files {
            let source = ImportSource {
                kind: ImportSourceKind::FileIndex,
                raw: format!("file_index:{}", raw_file.name),
            };

            let raw_files = vec![raw_file];

            let outcome = recorded
                .execute(source, || async {
                    let mut metadata_lookup = MetadataLookup::default();
                    let media_files =
                        metadata_lookup.build_media_files(raw_files.clone(), descriptions);
                    importer.transfer_media_files(&media_files).await
                })
                .await;

            match outcome {
                Ok(imported) => {
                    for item in &imported {
                        let (title, year, size) = match item {
                            crate::application::import::ImportedMedia::Movie {
                                title,
                                year,
                                size,
                                ..
                            } => (title.clone(), year.clone(), Some(*size)),
                            crate::application::import::ImportedMedia::Tv {
                                name, year, ..
                            } => (name.clone(), year.clone(), None),
                            crate::application::import::ImportedMedia::Skipped { .. } => {
                                ("skipped".into(), String::new(), None)
                            }
                        };
                        let status = match item {
                            crate::application::import::ImportedMedia::Skipped { .. } => "skipped",
                            _ => "succeeded",
                        };
                        results.push(ImportFileResult {
                            id: file_id,
                            status: status.into(),
                            title: Some(title),
                            year: Some(year),
                            size,
                            error: None,
                        });
                    }
                    if imported.is_empty() {
                        results.push(ImportFileResult {
                            id: file_id,
                            status: "skipped".into(),
                            title: None,
                            year: None,
                            size: None,
                            error: Some("no media matched".into()),
                        });
                    }
                }
                Err(err) => {
                    tracing::warn!(file_id, error = %err, "import from file index failed");
                    results.push(ImportFileResult {
                        id: file_id,
                        status: "failed".into(),
                        title: None,
                        year: None,
                        size: None,
                        error: Some(err.to_string()),
                    });
                }
            }
        }

        Ok(results)
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
    use crate::domain::{
        import_record::{ImportSourceKind, ImportStatus, RecordSummary, SummaryItem},
        share::{FileHash, RawFile},
    };
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

    struct FakeImporter {
        make_result: Box<dyn Fn() -> AppResult<Vec<ImportedMedia>> + Send>,
    }

    impl MediaImporter for FakeImporter {
        async fn transfer_media_files(
            &mut self,
            _media_files: &[crate::domain::import::inner::MediaFile],
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
            .import_from_fingerprints(&[], &mut importer, &recorded)
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
            .import_from_fingerprints(&[999], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1, 2], &mut importer, &recorded)
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
            .import_from_fingerprints(&[1, 2], &mut importer, &recorded)
            .await
            .unwrap();

        let created = import_repo.created.lock().unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].source_kind, ImportSourceKind::FileIndex);
        assert_eq!(created[1].source_kind, ImportSourceKind::FileIndex);
    }
}
