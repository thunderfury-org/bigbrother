use crate::{
    application::{
        file_index::FileIndexService,
        import::{ImportedMedia, MediaIdentifier, MediaImporter, MetadataLookup},
        ports::{CommunityCatalogHandle, CommunityThread, ShareResolver},
        recorded_import::{RecordedImportService, import_outcome_from},
    },
    domain::import_record::{ImportSource, ImportSourceKind, RecordSummary, summarize},
    error::AppResult,
};

#[cfg(test)]
use crate::application::ports::CommunityCatalog;

#[derive(Debug, Default, serde::Serialize)]
pub struct CommunityImportResult {
    pub tid: i64,
    pub thread_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_url: Option<String>,
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

impl CommunityImportResult {
    fn failed(tid: i64, thread_title: String, message: String) -> Self {
        Self {
            tid,
            thread_title,
            status: "failed".into(),
            error: Some(message),
            ..Default::default()
        }
    }

    fn skipped(tid: i64, thread_title: String, share_url: String, reason: &str) -> Self {
        Self {
            tid,
            thread_title,
            share_url: Some(share_url),
            status: "skipped".into(),
            error: Some(reason.into()),
            ..Default::default()
        }
    }

    fn from_imported(
        tid: i64,
        thread_title: String,
        share_url: String,
        imported: &[ImportedMedia],
    ) -> Self {
        let outcomes = imported.iter().map(import_outcome_from).collect::<Vec<_>>();
        let (summary, status) = summarize(&outcomes);
        let (title, year, size) = summary.display_fields();
        Self {
            tid,
            thread_title,
            share_url: Some(share_url),
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
pub struct CommunityService {
    catalog: CommunityCatalogHandle,
    file_index: FileIndexService,
}

impl CommunityService {
    #[cfg(test)]
    pub fn new(catalog: impl CommunityCatalog + 'static, file_index: FileIndexService) -> Self {
        Self::from_handle(std::sync::Arc::new(catalog), file_index)
    }

    pub fn from_handle(catalog: CommunityCatalogHandle, file_index: FileIndexService) -> Self {
        Self {
            catalog,
            file_index,
        }
    }

    pub async fn search_threads(
        &self,
        keyword: &str,
        limit: u64,
    ) -> AppResult<Vec<CommunityThread>> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Ok(Vec::new());
        }
        self.catalog.search_threads(keyword, limit).await
    }

    pub async fn import_threads(
        &self,
        tids: &[i64],
        resolver: &dyn ShareResolver,
        identifier: &dyn MediaIdentifier,
        importer: &dyn MediaImporter,
        recorded: &RecordedImportService,
    ) -> AppResult<Vec<CommunityImportResult>> {
        let mut results = Vec::new();
        for tid in tids {
            match self
                .import_one(*tid, resolver, identifier, importer, recorded)
                .await
            {
                Ok(items) => results.extend(items),
                Err(err) => results.push(CommunityImportResult::failed(
                    *tid,
                    format!("thread-{tid}"),
                    err.to_string(),
                )),
            }
        }
        Ok(results)
    }

    async fn import_one(
        &self,
        tid: i64,
        resolver: &dyn ShareResolver,
        identifier: &dyn MediaIdentifier,
        importer: &dyn MediaImporter,
        recorded: &RecordedImportService,
    ) -> AppResult<Vec<CommunityImportResult>> {
        let shares = self.catalog.share_urls_for_thread(tid).await?;
        let mut results = Vec::new();
        let metadata_lookup = MetadataLookup::default();

        for share_url in shares.share_urls {
            let source = source_for_url(&share_url);
            let raw_files = match resolver.raw_files_from_url(&share_url).await {
                Ok(Some(files)) if !files.is_empty() => files,
                Ok(Some(_)) => {
                    results.push(CommunityImportResult::skipped(
                        tid,
                        shares.title.clone(),
                        share_url,
                        "share has no files",
                    ));
                    continue;
                }
                Ok(None) => {
                    results.push(CommunityImportResult::skipped(
                        tid,
                        shares.title.clone(),
                        share_url,
                        "unsupported share url",
                    ));
                    continue;
                }
                Err(err) => {
                    results.push(CommunityImportResult::failed(
                        tid,
                        shares.title.clone(),
                        err.to_string(),
                    ));
                    results.last_mut().unwrap().share_url = Some(share_url);
                    continue;
                }
            };

            if let Err(err) = self
                .file_index
                .record_raw_files(raw_files.clone(), Some(shares.title.clone()))
                .await
            {
                tracing::warn!(error = %err, tid, "failed to index community share");
            }

            let descriptions = vec![shares.title.clone()];
            let media_files = metadata_lookup.build_media_files(raw_files, descriptions);
            let outcome = recorded
                .execute(source, || async {
                    let identified = identifier.identify(media_files.clone()).await?;
                    importer
                        .import_groups(identified.groups, identified.unmatched)
                        .await
                })
                .await;

            match outcome {
                Ok(imported) => {
                    if imported.is_empty() {
                        results.push(CommunityImportResult::skipped(
                            tid,
                            shares.title.clone(),
                            share_url,
                            "no media matched",
                        ));
                    } else {
                        results.push(CommunityImportResult::from_imported(
                            tid,
                            shares.title.clone(),
                            share_url,
                            &imported,
                        ));
                    }
                }
                Err(err) => {
                    tracing::warn!(tid, error = %err, "import from community thread failed");
                    let mut failed =
                        CommunityImportResult::failed(tid, shares.title.clone(), err.to_string());
                    failed.share_url = Some(share_url);
                    results.push(failed);
                }
            }
        }

        Ok(results)
    }
}

fn source_for_url(raw_url: &str) -> ImportSource {
    let kind = url::Url::parse(raw_url)
        .ok()
        .and_then(|parsed| {
            let host = parsed.host_str()?.to_ascii_lowercase();
            if host.contains("123") {
                Some(ImportSourceKind::Pan123)
            } else if host.contains("189") {
                Some(ImportSourceKind::Pan189)
            } else if host.contains("115") {
                Some(ImportSourceKind::Pan115)
            } else {
                None
            }
        })
        .unwrap_or(ImportSourceKind::Other);
    ImportSource {
        kind,
        raw: raw_url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::CommunityCatalog;
    use crate::{
        application::{
            import::identify::IdentifyOutcome,
            ports::{
                CommunityThreadShares, FileIndexRecordInput, FileIndexRepository, FileSearchRecord,
                ImportRecordCreate, ImportRecordFilter, ImportRecordFinalize, ImportRecordPage,
                ImportRecordPaging, ImportRecordRepository, ImportRecordView,
            },
        },
        domain::share::{FileHash, RawFile},
        error::AppError,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeCatalog {
        shares: CommunityThreadShares,
    }

    #[async_trait::async_trait]
    impl CommunityCatalog for FakeCatalog {
        async fn search_threads(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<CommunityThread>> {
            Ok(Vec::new())
        }

        async fn share_urls_for_thread(&self, _tid: i64) -> AppResult<CommunityThreadShares> {
            Ok(self.shares.clone())
        }
    }

    #[derive(Clone, Default)]
    struct FakeFileRepo;

    #[async_trait::async_trait]
    impl FileIndexRepository for FakeFileRepo {
        async fn record_files(&self, _files: &[FileIndexRecordInput]) -> AppResult<()> {
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
        files: Vec<RawFile>,
    }

    #[async_trait::async_trait]
    impl ShareResolver for FakeResolver {
        async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
            Ok(Some(self.files.clone()))
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
                title: "Black Mirror".into(),
                year: "2011".into(),
                size: 1,
                cost: std::time::Duration::from_millis(1),
                has_failed: false,
            }])
        }
    }

    fn raw_file() -> RawFile {
        RawFile {
            id: None,
            name: "Black.Mirror.S01E01.mkv".into(),
            hash: FileHash::Md5("a".repeat(32)),
            size: 1024,
            path: "/share".into(),
        }
    }

    #[tokio::test]
    async fn import_threads_records_share_url_and_imports() {
        let catalog = FakeCatalog {
            shares: CommunityThreadShares {
                tid: 50570,
                title: "黑镜".into(),
                share_urls: vec!["https://www.123684.com/s/share-key?pwd=pass".into()],
            },
        };
        let service = CommunityService::new(catalog, FileIndexService::new(FakeFileRepo));
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let results = service
            .import_threads(
                &[50570],
                &FakeResolver {
                    files: vec![raw_file()],
                },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "succeeded");
        assert_eq!(results[0].title.as_deref(), Some("Black Mirror"));
        assert_eq!(
            results[0].share_url.as_deref(),
            Some("https://www.123684.com/s/share-key?pwd=pass")
        );
        let summary = results[0].summary.as_ref().expect("import summary");
        assert_eq!(summary.items.len(), 1);
        assert_eq!(summary.total_size, 1);
    }

    struct FakeTvImporter;

    #[async_trait::async_trait]
    impl MediaImporter for FakeTvImporter {
        async fn import_groups(
            &self,
            _groups: Vec<crate::domain::import::inner::Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Ok(vec![ImportedMedia::Tv {
                name: "入青云".into(),
                year: "2025".into(),
                season: 1,
                episodes: vec![15, 16],
                missing_episodes: vec![],
                max_episode_number: 16,
                total_size: 2048,
                number_of_episodes: 16,
                cost: std::time::Duration::from_millis(1200),
                has_failed: false,
                failed_episodes: vec![],
            }])
        }
    }

    #[tokio::test]
    async fn import_threads_includes_tv_summary() {
        let catalog = FakeCatalog {
            shares: CommunityThreadShares {
                tid: 1,
                title: "入青云".into(),
                share_urls: vec!["https://www.123684.com/s/share-key?pwd=pass".into()],
            },
        };
        let service = CommunityService::new(catalog, FileIndexService::new(FakeFileRepo));
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let results = service
            .import_threads(
                &[1],
                &FakeResolver {
                    files: vec![raw_file()],
                },
                &FakeIdentifier,
                &FakeTvImporter,
                &recorded,
            )
            .await
            .unwrap();
        let summary = results[0].summary.as_ref().expect("import summary");
        match &summary.items[0] {
            crate::domain::import_record::SummaryItem::Tv {
                name,
                season,
                episodes,
                ..
            } => {
                assert_eq!(name, "入青云");
                assert_eq!(*season, 1);
                assert_eq!(episodes.len(), 2);
            }
            other => panic!("expected tv summary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_threads_returns_empty_for_blank_keyword() {
        let service =
            CommunityService::new(FakeCatalog::default(), FileIndexService::new(FakeFileRepo));
        let items = service.search_threads("  ", 20).await.unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn source_for_url_maps_pan123_host() {
        let source = source_for_url("https://www.123684.com/s/share-key?pwd=pass");
        assert_eq!(source.kind, ImportSourceKind::Pan123);
    }

    #[tokio::test]
    async fn import_threads_surfaces_catalog_errors() {
        struct FailingCatalog;
        #[async_trait::async_trait]
        impl CommunityCatalog for FailingCatalog {
            async fn search_threads(
                &self,
                _keyword: &str,
                _limit: u64,
            ) -> AppResult<Vec<CommunityThread>> {
                Ok(Vec::new())
            }
            async fn share_urls_for_thread(&self, _tid: i64) -> AppResult<CommunityThreadShares> {
                Err(AppError::Unauthorized("no cookie".into()))
            }
        }
        let service = CommunityService::new(FailingCatalog, FileIndexService::new(FakeFileRepo));
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let results = service
            .import_threads(
                &[1],
                &FakeResolver { files: vec![] },
                &FakeIdentifier,
                &FakeImporter,
                &recorded,
            )
            .await
            .unwrap();
        assert_eq!(results[0].status, "failed");
        assert!(results[0].error.as_deref().unwrap().contains("no cookie"));
    }
}
