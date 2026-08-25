use crate::{
    application::{
        file_index::FileIndexService,
        import::{ImportedMedia, MediaIdentifier, MediaImporter, MetadataLookup},
        ports::ShareResolver,
        recorded_import::{RecordedImportService, import_outcome_from},
    },
    domain::import_record::{ImportSource, RecordSummary, summarize},
    error::{AppError, AppResult},
};

#[derive(Debug, Default, serde::Serialize)]
pub struct ShareImportResult {
    pub url: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RecordSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ShareImportResult {
    fn skipped(url: String, reason: &str) -> Self {
        Self {
            url,
            status: "skipped".into(),
            error: Some(reason.into()),
            ..Default::default()
        }
    }

    fn failed(url: String, message: String) -> Self {
        Self {
            url,
            status: "failed".into(),
            error: Some(message),
            ..Default::default()
        }
    }

    fn from_imported(url: String, imported: &[ImportedMedia]) -> Self {
        let outcomes = imported.iter().map(import_outcome_from).collect::<Vec<_>>();
        let (summary, status) = summarize(&outcomes);
        let (title, year, size) = summary.display_fields();
        Self {
            url,
            status: status.as_str().to_owned(),
            title,
            year,
            size,
            summary: Some(summary),
            error: None,
        }
    }
}

#[derive(Clone)]
pub struct ShareImportService {
    file_index: FileIndexService,
}

impl ShareImportService {
    pub fn new(file_index: FileIndexService) -> Self {
        Self { file_index }
    }

    pub async fn import_url(
        &self,
        source: ImportSource,
        description: Option<String>,
        resolver: &dyn ShareResolver,
        identifier: &dyn MediaIdentifier,
        importer: &dyn MediaImporter,
        recorded: &RecordedImportService,
    ) -> AppResult<ShareImportResult> {
        let url = source.raw.trim();
        if url.is_empty() {
            return Err(AppError::InvalidParameter("url must not be empty".into()));
        }
        let source = ImportSource {
            kind: source.kind,
            raw: url.to_owned(),
        };

        let raw_files = match resolver.raw_files_from_url(source.raw.as_str()).await? {
            Some(files) if !files.is_empty() => files,
            Some(_) => {
                return Ok(ShareImportResult::skipped(source.raw, "share has no files"));
            }
            None => {
                return Err(AppError::InvalidParameter(format!(
                    "unsupported share url: {}",
                    source.raw
                )));
            }
        };

        if let Err(err) = self
            .file_index
            .record_raw_files(raw_files.clone(), description.clone())
            .await
        {
            tracing::warn!(error = %err, "failed to index share url");
        }

        let descriptions: Vec<String> = description.into_iter().collect();
        let media_files = MetadataLookup::default().build_media_files(raw_files, descriptions);
        let url = source.raw.clone();
        let outcome = recorded
            .execute(source, || async {
                let identified = identifier.identify(media_files.clone()).await?;
                importer
                    .import_groups(identified.groups, identified.unmatched)
                    .await
            })
            .await;

        match outcome {
            Ok(imported) if imported.is_empty() => {
                Ok(ShareImportResult::skipped(url, "no media matched"))
            }
            Ok(imported) => Ok(ShareImportResult::from_imported(url, &imported)),
            Err(err) => Ok(ShareImportResult::failed(url, err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            import::identify::IdentifyOutcome,
            ports::{
                FileIndexRecordInput, FileIndexRepository, FileSearchRecord, ImportRecordCreate,
                ImportRecordFilter, ImportRecordFinalize, ImportRecordPage, ImportRecordPaging,
                ImportRecordRepository, ImportRecordView,
            },
        },
        domain::{
            import_record::ImportSourceKind,
            share::{FileHash, RawFile},
        },
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeFileRepo {
        recorded: Arc<Mutex<Vec<Vec<FileIndexRecordInput>>>>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl FileIndexRepository for FakeFileRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
            if self.fail {
                return Err(AppError::Database("index write failed".into(), false));
            }
            self.recorded.lock().unwrap().push(files.to_vec());
            Ok(())
        }
        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
        async fn get_records_by_ids(&self, _ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct FakeImportRepo {
        created: Arc<Mutex<Vec<ImportRecordCreate>>>,
    }

    #[async_trait::async_trait]
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

    struct FakeResolver {
        result: AppResult<Option<Vec<RawFile>>>,
    }

    #[async_trait::async_trait]
    impl ShareResolver for FakeResolver {
        async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
            self.result.clone()
        }
    }

    struct FakeIdentifier;

    #[async_trait::async_trait]
    impl MediaIdentifier for FakeIdentifier {
        async fn identify(
            &self,
            _files: Vec<crate::domain::import::inner::MediaFile>,
        ) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: vec![],
                unmatched: vec![],
            })
        }
    }

    struct FakeImporter;

    #[async_trait::async_trait]
    impl MediaImporter for FakeImporter {
        async fn import_groups(
            &self,
            _groups: Vec<crate::domain::import::inner::Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Ok(vec![ImportedMedia::Movie {
                title: "Inception".into(),
                year: "2010".into(),
                size: 1024,
                cost: std::time::Duration::from_millis(5),
                has_failed: false,
            }])
        }
    }

    struct EmptyImporter;

    #[async_trait::async_trait]
    impl MediaImporter for EmptyImporter {
        async fn import_groups(
            &self,
            _groups: Vec<crate::domain::import::inner::Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Ok(vec![])
        }
    }

    struct FailingImporter;

    #[async_trait::async_trait]
    impl MediaImporter for FailingImporter {
        async fn import_groups(
            &self,
            _groups: Vec<crate::domain::import::inner::Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Err(AppError::ExternalService("transfer failed".into(), false))
        }
    }

    fn pan123_url() -> &'static str {
        "https://www.123684.com/s/share-key?pwd=pass"
    }

    fn share_source(raw: &str, kind: ImportSourceKind) -> ImportSource {
        ImportSource {
            kind,
            raw: raw.to_owned(),
        }
    }

    fn raw_file() -> RawFile {
        RawFile {
            id: None,
            name: "Inception.2010.1080p.mkv".into(),
            hash: FileHash::Md5("a".repeat(32)),
            size: 1024,
            path: "/share".into(),
        }
    }

    fn service(repo: FakeFileRepo) -> ShareImportService {
        ShareImportService::new(FileIndexService::new(repo))
    }

    #[tokio::test]
    async fn rejects_blank_url() {
        let imports = FakeImportRepo::default();
        let recorded = RecordedImportService::new(imports.clone());
        let err = service(FakeFileRepo::default())
            .import_url(
                share_source("  ", ImportSourceKind::Other),
                None,
                &FakeResolver {
                    result: Ok(Some(vec![raw_file()])),
                },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::InvalidParameter(message) if message == "url must not be empty")
        );
        assert!(imports.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_unsupported_share_url() {
        let imports = FakeImportRepo::default();
        let recorded = RecordedImportService::new(imports.clone());
        let err = service(FakeFileRepo::default())
            .import_url(
                share_source("https://example.com/share", ImportSourceKind::Other),
                None,
                &FakeResolver { result: Ok(None) },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::InvalidParameter(message) if message.contains("unsupported share url")
        ));
        assert!(imports.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn skips_share_with_no_files() {
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let result = service(FakeFileRepo::default())
            .import_url(
                share_source(pan123_url(), ImportSourceKind::Pan123),
                None,
                &FakeResolver {
                    result: Ok(Some(Vec::new())),
                },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(result.status, "skipped");
        assert_eq!(result.error.as_deref(), Some("share has no files"));
        assert_eq!(result.url, pan123_url());
    }

    #[tokio::test]
    async fn indexes_and_imports_supported_share_without_subscription_gate() {
        let files = FakeFileRepo::default();
        let imports = FakeImportRepo::default();
        let recorded = RecordedImportService::new(imports.clone());
        let result = service(files.clone())
            .import_url(
                share_source(pan123_url(), ImportSourceKind::Pan123),
                Some("operator note".into()),
                &FakeResolver {
                    result: Ok(Some(vec![raw_file()])),
                },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap();

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.title.as_deref(), Some("Inception"));
        assert_eq!(result.year.as_deref(), Some("2010"));
        assert_eq!(result.size, Some(1024));
        let summary = result.summary.as_ref().expect("import summary");
        assert_eq!(summary.items.len(), 1);
        assert_eq!(summary.total_size, 1024);

        let indexed = files.recorded.lock().unwrap();
        assert_eq!(indexed.len(), 1);
        assert_eq!(indexed[0][0].file_name, "Inception.2010.1080p.mkv");
        assert_eq!(indexed[0][0].description.as_deref(), Some("operator note"));

        let created = imports.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].source_kind, ImportSourceKind::Pan123);
        assert_eq!(created[0].source, pan123_url());
    }

    #[tokio::test]
    async fn index_failure_does_not_block_import() {
        let files = FakeFileRepo {
            fail: true,
            ..FakeFileRepo::default()
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let result = service(files)
            .import_url(
                share_source(pan123_url(), ImportSourceKind::Pan123),
                None,
                &FakeResolver {
                    result: Ok(Some(vec![raw_file()])),
                },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(result.status, "succeeded");
        assert_eq!(result.title.as_deref(), Some("Inception"));
    }

    #[tokio::test]
    async fn skips_when_no_media_matched() {
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let result = service(FakeFileRepo::default())
            .import_url(
                share_source(pan123_url(), ImportSourceKind::Pan123),
                None,
                &FakeResolver {
                    result: Ok(Some(vec![raw_file()])),
                },
                &FakeIdentifier,
                &EmptyImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(result.status, "skipped");
        assert_eq!(result.error.as_deref(), Some("no media matched"));
    }

    #[tokio::test]
    async fn returns_failed_result_when_import_errors() {
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let result = service(FakeFileRepo::default())
            .import_url(
                share_source(pan123_url(), ImportSourceKind::Pan123),
                None,
                &FakeResolver {
                    result: Ok(Some(vec![raw_file()])),
                },
                &FakeIdentifier,
                &FailingImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(result.status, "failed");
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|message| message.contains("transfer failed"))
        );
    }
}
