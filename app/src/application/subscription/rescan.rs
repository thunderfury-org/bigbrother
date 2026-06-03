use crate::application::file_index::FileIndexService;
use crate::application::file_index_import::ImportFileResult;
use crate::application::import::MetadataLookup;
use crate::application::import_ports::{MediaIdentifier, MediaImporter};
use crate::application::ports::{
    FileIndexRepository, ImportRecordRepository, SubscriptionRepository,
};
use crate::application::recorded_import::RecordedImportService;
use crate::domain::import_record::{ImportSource, ImportSourceKind};
use crate::error::AppResult;

use super::import_filter::filter_by_subscription;

pub(crate) async fn rescan_subscription<R, FI, I, D, RecordRepo>(
    subscription_id: i64,
    sub_repo: &R,
    file_index: &FileIndexService<FI>,
    identifier: &mut D,
    importer: &mut I,
    recorded: &RecordedImportService<RecordRepo>,
) -> AppResult<Vec<ImportFileResult>>
where
    R: SubscriptionRepository,
    FI: FileIndexRepository,
    I: MediaImporter,
    D: MediaIdentifier,
    RecordRepo: ImportRecordRepository,
{
    let subscription = sub_repo.get_by_id(subscription_id).await?.ok_or_else(|| {
        crate::error::AppError::NotFound(format!("subscription {subscription_id} not found"))
    })?;

    let mut queries = Vec::new();
    if let Some(t) = &subscription.title_en
        && !t.is_empty()
    {
        queries.push(t.as_str());
    }
    if let Some(t) = &subscription.title_zh
        && !t.is_empty()
        && !queries.contains(&t.as_str())
    {
        queries.push(t.as_str());
    }

    if queries.is_empty() {
        return Ok(Vec::new());
    }

    let mut seen_ids = std::collections::HashSet::new();
    let mut search_results = Vec::new();
    for query in &queries {
        for record in file_index.search_files(query, 100).await? {
            if seen_ids.insert(record.id) {
                search_results.push(record);
            }
        }
    }
    if search_results.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for record in search_results {
        let file_id = record.id;
        let locations = record.locations;
        for location in locations {
            let source = ImportSource {
                kind: ImportSourceKind::FileIndex,
                raw: format!("file_index:{}:{}", file_id, location.file_name),
            };
            let hash = match record.hash_type.as_str() {
                "sha1" => crate::domain::share::FileHash::Sha1(record.hash_value.clone()),
                _ => crate::domain::share::FileHash::Md5(record.hash_value.clone()),
            };
            let raw_file = crate::domain::share::RawFile {
                id: Some(file_id),
                name: location.file_name,
                hash,
                size: record.size,
                path: location.file_path,
            };
            let descriptions = location.descriptions;
            let raw_files = vec![raw_file];

            let outcome = recorded
                .execute(source, || async {
                    let mut metadata_lookup = MetadataLookup::default();
                    let media_files =
                        metadata_lookup.build_media_files(raw_files.clone(), descriptions);
                    let identified = identifier.identify(media_files).await?;
                    let filtered_groups = filter_by_subscription(sub_repo, identified.groups).await;
                    importer
                        .import_groups(filtered_groups, identified.unmatched)
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
                    tracing::warn!(file_id, error = %err, "rescan import failed");
                    results.push(ImportFileResult::failed(file_id, err.to_string()));
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        import::ImportedMedia,
        import::identify::IdentifyOutcome,
        import_ports::{MediaIdentifier, MediaImporter},
        ports::{
            FileIndexRecordInput, FileLocationRecord, FileSearchRecord, ImportRecordCreate,
            ImportRecordFilter, ImportRecordFinalize, ImportRecordPage, ImportRecordPaging,
            ImportRecordView, SubscriptionCreateInput, SubscriptionRecord,
        },
    };
    use crate::domain::{import::inner::MediaFile, subscription::SubscriptionMediaType};
    use crate::error::AppError;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct FakeSubRepo {
        records: Arc<Mutex<Vec<SubscriptionRecord>>>,
    }

    impl SubscriptionRepository for FakeSubRepo {
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
        async fn identify(&mut self, _files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
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

    fn movie_importer() -> FakeImporter {
        FakeImporter {
            make_result: Box::new(|| {
                Ok(vec![ImportedMedia::Movie {
                    title: "Inception".into(),
                    year: "2010".into(),
                    size: 1_000_000,
                    cost: Duration::from_secs(1),
                    has_failed: false,
                }])
            }),
        }
    }

    #[tokio::test]
    async fn subscription_not_found_returns_404() {
        let sub_repo = FakeSubRepo::default();
        let file_repo = FakeFileRepo::default();
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let mut identifier = FakeIdentifier;
        let mut importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };

        let err = rescan_subscription(
            999,
            &sub_repo,
            &file_index,
            &mut identifier,
            &mut importer,
            &recorded,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn no_files_found_returns_empty() {
        let sub_repo = FakeSubRepo::default();
        sub_repo
            .create(&SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
            })
            .await
            .unwrap();

        let file_repo = FakeFileRepo::default();
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let mut identifier = FakeIdentifier;
        let mut importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &mut identifier,
            &mut importer,
            &recorded,
        )
        .await
        .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn happy_path_returns_import_results() {
        let sub_repo = FakeSubRepo::default();
        sub_repo
            .create(&SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
            })
            .await
            .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![sample_record(1)])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let mut identifier = FakeIdentifier;
        let mut importer = movie_importer();

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &mut identifier,
            &mut importer,
            &recorded,
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "succeeded");
        assert_eq!(results[0].title.as_deref(), Some("Inception"));
        assert_eq!(results[0].year.as_deref(), Some("2010"));
    }
}
