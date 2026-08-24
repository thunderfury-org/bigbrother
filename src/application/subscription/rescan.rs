use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::application::file_index::FileIndexService;
use crate::application::file_index_import::ImportFileResult;
use crate::application::import::MetadataLookup;
use crate::application::import::identify::{IdentifyOutcome, UnmatchedFile};
use crate::application::import::{ImportedMedia, MediaIdentifier, MediaImporter};
use crate::application::ports::{SubscriptionRecord, SubscriptionRepository};
use crate::application::recorded_import::RecordedImportService;
use crate::application::subscription::import_filter::media_matches_subscription;
use crate::domain::import::inner::{Media, MediaFile};
use crate::domain::import::policy::{insert_movie_media, insert_tv_media};
use crate::domain::import_record::{ImportSource, ImportSourceKind};
use crate::error::{AppError, AppResult};

const RESCAN_FILE_SEARCH_LIMIT: u64 = 100;
const HIGH_RELEVANCE_MAX_RANK: i64 = 1;

pub(crate) async fn rescan_subscription(
    subscription_id: i64,
    sub_repo: &dyn SubscriptionRepository,
    file_index: &FileIndexService,
    identifier: &dyn MediaIdentifier,
    importer: &dyn MediaImporter,
    recorded: &RecordedImportService,
) -> AppResult<Vec<ImportFileResult>> {
    let subscription = sub_repo
        .get_by_id(subscription_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("subscription {subscription_id} not found")))?;

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

    let mut seen_ids = HashSet::new();
    let mut search_results = Vec::new();
    for query in &queries {
        for record in file_index
            .search_files(query, RESCAN_FILE_SEARCH_LIMIT)
            .await?
        {
            if record.rank > HIGH_RELEVANCE_MAX_RANK {
                continue;
            }
            if seen_ids.insert(record.id) {
                search_results.push(record);
            }
        }
    }
    if search_results.is_empty() {
        return Ok(Vec::new());
    }

    let search_ids: Vec<i64> = search_results.iter().map(|record| record.id).collect();
    let ready_files = file_index.get_import_ready_files(&search_ids).await?;
    if ready_files.is_empty() {
        return Ok(Vec::new());
    }
    let file_ids: Vec<i64> = ready_files.iter().map(|(id, _, _)| *id).collect();
    let pending: Vec<_> = ready_files
        .into_iter()
        .map(|(_, raw_file, descriptions)| (raw_file, descriptions))
        .collect();

    let source = ImportSource {
        kind: ImportSourceKind::FileIndex,
        raw: subscription_rescan_source(&subscription),
    };

    let metadata_lookup = MetadataLookup::default();
    let mut media_files = Vec::new();
    for (raw_file, descriptions) in pending {
        media_files.extend(metadata_lookup.build_media_files(vec![raw_file], descriptions));
    }
    let identified = identify_rescan_files(identifier, media_files).await;
    let failed = identified.failed;
    let filtered_groups = groups_for_subscription(&subscription, identified.groups);
    let imported_file_ids = media_file_ids(&filtered_groups);

    let outcome = recorded
        .execute(source, || async {
            let imported = if filtered_groups.is_empty() && identified.unmatched.is_empty() {
                Vec::new()
            } else {
                importer
                    .import_groups(filtered_groups, identified.unmatched)
                    .await?
            };
            Ok(append_identify_failures(imported, &failed))
        })
        .await;

    match outcome {
        Ok(imported) => Ok(results_for_files(
            &file_ids,
            &imported,
            &failed,
            &imported_file_ids,
        )),
        Err(err) => {
            tracing::warn!(error = %err, "rescan import failed");
            Ok(results_for_failed_batch(
                &file_ids,
                &failed,
                err.to_string(),
            ))
        }
    }
}

struct RescanIdentify {
    groups: Vec<Media>,
    unmatched: Vec<UnmatchedFile>,
    failed: Vec<(i64, AppError)>,
}

async fn identify_rescan_files(
    identifier: &dyn MediaIdentifier,
    files: Vec<MediaFile>,
) -> RescanIdentify {
    match identifier.identify(files.clone()).await {
        Ok(IdentifyOutcome { groups, unmatched }) => RescanIdentify {
            groups,
            unmatched,
            failed: Vec::new(),
        },
        Err(_) => {
            let mut groups = Vec::new();
            let mut unmatched = Vec::new();
            let mut failed = Vec::new();
            for file in files {
                let Some(file_id) = file.video.id else {
                    continue;
                };
                match identifier.identify(vec![file]).await {
                    Ok(outcome) => {
                        groups.extend(outcome.groups);
                        unmatched.extend(outcome.unmatched);
                    }
                    Err(err) => failed.push((file_id, err)),
                }
            }
            RescanIdentify {
                groups: merge_media_groups(groups),
                unmatched,
                failed,
            }
        }
    }
}

fn merge_media_groups(groups: Vec<Media>) -> Vec<Media> {
    let mut by_id = HashMap::new();
    for group in groups {
        match group {
            Media::Tv { detail, files } => {
                for (season, episodes) in files {
                    for (episode, media_files) in episodes {
                        for file in media_files {
                            insert_tv_media(&mut by_id, detail.clone(), season, episode, file);
                        }
                    }
                }
            }
            Media::Movie { detail, files } => {
                for file in files {
                    insert_movie_media(&mut by_id, detail.clone(), file);
                }
            }
        }
    }
    by_id.into_values().collect()
}

fn subscription_rescan_source(subscription: &SubscriptionRecord) -> String {
    let title = subscription
        .title_en
        .as_deref()
        .filter(|title| !title.is_empty())
        .or(subscription
            .title_zh
            .as_deref()
            .filter(|title| !title.is_empty()))
        .unwrap_or("untitled");
    format!("subscription:{}:{title}", subscription.id)
}

fn groups_for_subscription(subscription: &SubscriptionRecord, groups: Vec<Media>) -> Vec<Media> {
    groups
        .into_iter()
        .filter(|media| {
            media_matches_subscription(media, subscription.tmdb_id, &subscription.media_type)
        })
        .collect()
}

fn media_file_ids(groups: &[Media]) -> HashSet<i64> {
    let mut ids = HashSet::new();
    for group in groups {
        match group {
            Media::Movie { files, .. } => {
                for file in files {
                    if let Some(id) = file.video.id {
                        ids.insert(id);
                    }
                }
            }
            Media::Tv { files, .. } => {
                for episodes in files.values() {
                    for media_files in episodes.values() {
                        for file in media_files {
                            if let Some(id) = file.video.id {
                                ids.insert(id);
                            }
                        }
                    }
                }
            }
        }
    }
    ids
}

fn append_identify_failures(
    mut imported: Vec<ImportedMedia>,
    failed: &[(i64, AppError)],
) -> Vec<ImportedMedia> {
    imported.extend(failed.iter().map(|(file_id, _err)| ImportedMedia::Movie {
        title: format!("file {file_id}"),
        year: String::new(),
        size: 0,
        cost: Duration::ZERO,
        has_failed: true,
    }));
    imported
}

fn results_for_files(
    file_ids: &[i64],
    imported: &[ImportedMedia],
    failed: &[(i64, AppError)],
    imported_file_ids: &HashSet<i64>,
) -> Vec<ImportFileResult> {
    let failed_by_id: HashMap<i64, String> = failed
        .iter()
        .map(|(id, err)| (*id, err.to_string()))
        .collect();
    let sample = imported.iter().find(|item| {
        !matches!(
            item,
            ImportedMedia::Skipped { .. }
                | ImportedMedia::Movie {
                    has_failed: true,
                    ..
                }
        )
    });
    file_ids
        .iter()
        .map(|file_id| {
            if let Some(message) = failed_by_id.get(file_id) {
                ImportFileResult::failed(*file_id, message.clone())
            } else if imported_file_ids.contains(file_id) {
                if let Some(item) = sample {
                    ImportFileResult::from_imported(*file_id, item)
                } else {
                    ImportFileResult::skipped(*file_id, "no media matched")
                }
            } else {
                ImportFileResult::skipped(*file_id, "no media matched")
            }
        })
        .collect()
}

fn results_for_failed_batch(
    file_ids: &[i64],
    failed: &[(i64, AppError)],
    fallback: String,
) -> Vec<ImportFileResult> {
    let failed_by_id: HashMap<i64, String> = failed
        .iter()
        .map(|(id, err)| (*id, err.to_string()))
        .collect();
    file_ids
        .iter()
        .map(|file_id| {
            let message = failed_by_id
                .get(file_id)
                .cloned()
                .unwrap_or_else(|| fallback.clone());
            ImportFileResult::failed(*file_id, message)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        import::ImportedMedia,
        import::identify::IdentifyOutcome,
        import::{MediaIdentifier, MediaImporter},
        ports::{
            FileIndexRecordInput, FileIndexRepository, FileLocationRecord, FileSearchRecord,
            ImportRecordCreate, ImportRecordFilter, ImportRecordFinalize, ImportRecordPage,
            ImportRecordPaging, ImportRecordRepository, ImportRecordView, SubscriptionCreateInput,
            SubscriptionRecord, SubscriptionRepository,
        },
    };
    use crate::domain::{
        import::inner::{Media, MediaFile},
        import::{MovieDetail, TvDetail},
        import_record::{ImportSourceKind, ImportStatus, RecordSummary, SummaryItem},
        subscription::SubscriptionMediaType,
    };
    use crate::error::AppError;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct FakeSubRepo {
        records: Arc<Mutex<Vec<SubscriptionRecord>>>,
    }

    #[async_trait::async_trait]
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
    struct FakeFileRepo {
        records: Arc<Mutex<Vec<FileSearchRecord>>>,
    }

    #[async_trait::async_trait]
    impl FileIndexRepository for FakeFileRepo {
        async fn record_files(&self, _inputs: &[FileIndexRecordInput]) -> AppResult<()> {
            Ok(())
        }
        async fn search_files(
            &self,
            _keyword: &str,
            limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            let mut records = self.records.lock().unwrap().clone();
            records.sort_by_key(|record| (record.rank, record.id));
            records.truncate(limit as usize);
            Ok(records)
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

    #[async_trait::async_trait]
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

    #[async_trait::async_trait]
    impl MediaIdentifier for FakeIdentifier {
        async fn identify(&self, _files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: vec![],
                unmatched: vec![],
            })
        }
    }

    struct MovieIdentifier {
        tmdb_id: u32,
        title: String,
    }

    #[async_trait::async_trait]
    impl MediaIdentifier for MovieIdentifier {
        async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: vec![Media::Movie {
                    detail: MovieDetail {
                        id: self.tmdb_id,
                        title: self.title.clone(),
                        adult: false,
                        genres: vec![],
                        original_language: "en".into(),
                        original_title: self.title.clone(),
                        origin_country: vec![],
                        release_date: "2010-07-16".into(),
                    },
                    files,
                }],
                unmatched: vec![],
            })
        }
    }

    struct FakeImporter {
        make_result: Box<dyn Fn() -> AppResult<Vec<ImportedMedia>> + Send + Sync>,
    }

    #[async_trait::async_trait]
    impl MediaImporter for FakeImporter {
        async fn import_groups(
            &self,
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
            rank: 0,
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
        let identifier = FakeIdentifier;
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };

        let err = rescan_subscription(
            999,
            &sub_repo,
            &file_index,
            &identifier,
            &importer,
            &recorded,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn no_files_found_returns_empty() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo::default();
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());
        let identifier = FakeIdentifier;
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };

        let results =
            rescan_subscription(1, &sub_repo, &file_index, &identifier, &importer, &recorded)
                .await
                .unwrap();

        assert!(results.is_empty());
        assert!(import_repo.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn happy_path_returns_import_results() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![sample_record(1)])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let identifier = MovieIdentifier {
            tmdb_id: 27205,
            title: "Inception".into(),
        };
        let importer = movie_importer();

        let results =
            rescan_subscription(1, &sub_repo, &file_index, &identifier, &importer, &recorded)
                .await
                .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "succeeded");
        assert_eq!(results[0].title.as_deref(), Some("Inception"));
        assert_eq!(results[0].year.as_deref(), Some("2010"));
    }

    struct GroupingIdentifier {
        tmdb_id: u32,
        name: String,
    }

    #[async_trait::async_trait]
    impl MediaIdentifier for GroupingIdentifier {
        async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            let mut season_map: BTreeMap<u32, BTreeMap<u32, Vec<MediaFile>>> = BTreeMap::new();
            for (index, file) in files.into_iter().enumerate() {
                let season = file.metadata.season_number.unwrap_or(1);
                let episode = file.metadata.episode_number.unwrap_or(index as u32 + 1);
                season_map
                    .entry(season)
                    .or_default()
                    .entry(episode)
                    .or_default()
                    .push(file);
            }
            Ok(IdentifyOutcome {
                groups: vec![Media::Tv {
                    detail: TvDetail {
                        id: self.tmdb_id,
                        name: self.name.clone(),
                        first_air_date: "2008-01-20".into(),
                        number_of_episodes: 7,
                        number_of_seasons: 1,
                        origin_country: vec![],
                        original_language: "en".into(),
                        original_name: self.name.clone(),
                        genres: vec![],
                        seasons: vec![],
                    },
                    files: season_map,
                }],
                unmatched: vec![],
            })
        }
    }

    struct TvFromGroupsImporter;

    #[async_trait::async_trait]
    impl MediaImporter for TvFromGroupsImporter {
        async fn import_groups(
            &self,
            groups: Vec<Media>,
            _unmatched: Vec<crate::application::import::identify::UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Ok(groups
                .into_iter()
                .filter_map(|group| match group {
                    Media::Tv { detail, files } => {
                        let season = files.keys().next().copied().unwrap_or(1);
                        let mut episodes: Vec<u32> = files
                            .values()
                            .flat_map(|episodes| episodes.keys().copied())
                            .collect();
                        episodes.sort_unstable();
                        Some(ImportedMedia::Tv {
                            name: detail.name,
                            year: "2008".into(),
                            season,
                            episodes: episodes.clone(),
                            missing_episodes: vec![],
                            max_episode_number: episodes.iter().copied().max().unwrap_or(0),
                            total_size: 2_000,
                            number_of_episodes: 7,
                            cost: Duration::from_millis(20),
                            has_failed: false,
                            failed_episodes: vec![],
                        })
                    }
                    Media::Movie { .. } => None,
                })
                .collect())
        }
    }

    fn tv_record(id: i64, name: &str) -> FileSearchRecord {
        FileSearchRecord {
            id,
            size: 1000,
            hash_type: "md5".into(),
            hash_value: format!("hash-{id}"),
            locations: vec![FileLocationRecord {
                file_name: format!("{name}.S01E{id:02}.mkv"),
                file_path: format!("/tv/{name}"),
                descriptions: vec!["desc".into()],
            }],
            rank: 0,
        }
    }

    #[tokio::test]
    async fn tv_rescan_writes_one_import_record_for_multiple_episodes() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 1396,
                media_type: SubscriptionMediaType::Tv,
                title_zh: None,
                title_en: Some("Breaking Bad".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![
                tv_record(1, "Breaking.Bad"),
                tv_record(2, "Breaking.Bad"),
            ])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());
        let identifier = GroupingIdentifier {
            tmdb_id: 1396,
            name: "Breaking Bad".into(),
        };

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &identifier,
            &TvFromGroupsImporter,
            &recorded,
        )
        .await
        .unwrap();

        let created = import_repo.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].source_kind, ImportSourceKind::FileIndex);
        assert_eq!(created[0].source, "subscription:1:Breaking Bad");

        let finalized = import_repo.finalized.lock().unwrap();
        assert_eq!(finalized.len(), 1);
        let summary: RecordSummary = serde_json::from_str(&finalized[0].1.summary_json).unwrap();
        match &summary.items[..] {
            [SummaryItem::Tv { name, episodes, .. }] => {
                assert_eq!(name, "Breaking Bad");
                let nums: Vec<u32> = episodes.iter().map(|e| e.episode).collect();
                assert_eq!(nums, vec![1, 2]);
            }
            other => panic!("expected one tv summary, got {other:?}"),
        }

        let mut ids: Vec<i64> = results.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);
        assert!(results.iter().all(|r| r.status == "succeeded"));
    }

    struct IsolateIdentifier {
        tmdb_id: u32,
        name: String,
    }

    #[async_trait::async_trait]
    impl MediaIdentifier for IsolateIdentifier {
        async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            if files.iter().any(|file| file.video.name.contains("BAD")) {
                if files.len() > 1 {
                    return Err(AppError::ExternalService(
                        "batch identify failed".into(),
                        false,
                    ));
                }
                return Err(AppError::ExternalService("bad file".into(), false));
            }
            GroupingIdentifier {
                tmdb_id: self.tmdb_id,
                name: self.name.clone(),
            }
            .identify(files)
            .await
        }
    }

    #[tokio::test]
    async fn one_identify_failure_does_not_abort_remaining_files() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 1396,
                media_type: SubscriptionMediaType::Tv,
                title_zh: None,
                title_en: Some("Breaking Bad".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![
                tv_record(1, "Breaking.Bad"),
                FileSearchRecord {
                    id: 2,
                    size: 1000,
                    hash_type: "md5".into(),
                    hash_value: "hash-2".into(),
                    locations: vec![FileLocationRecord {
                        file_name: "BAD.S01E02.mkv".into(),
                        file_path: "/tv/Breaking.Bad".into(),
                        descriptions: vec!["desc".into()],
                    }],
                    rank: 0,
                },
            ])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &IsolateIdentifier {
                tmdb_id: 1396,
                name: "Breaking Bad".into(),
            },
            &TvFromGroupsImporter,
            &recorded,
        )
        .await
        .unwrap();

        assert_eq!(import_repo.created.lock().unwrap().len(), 1);
        assert_eq!(
            import_repo.finalized.lock().unwrap()[0].1.status,
            ImportStatus::PartiallyFailed
        );
        let succeeded: Vec<i64> = results
            .iter()
            .filter(|r| r.status == "succeeded")
            .map(|r| r.id)
            .collect();
        let failed: Vec<i64> = results
            .iter()
            .filter(|r| r.status == "failed")
            .map(|r| r.id)
            .collect();
        assert_eq!(succeeded, vec![1]);
        assert_eq!(failed, vec![2]);
    }

    struct MultiShowIdentifier;

    #[async_trait::async_trait]
    impl MediaIdentifier for MultiShowIdentifier {
        async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            let mut matched = Vec::new();
            let mut other: BTreeMap<u32, BTreeMap<u32, Vec<MediaFile>>> = BTreeMap::new();
            for file in files {
                if file.video.name.contains("Other") {
                    other
                        .entry(1)
                        .or_insert_with(BTreeMap::new)
                        .entry(1)
                        .or_default()
                        .push(file);
                } else {
                    matched.push(file);
                }
            }
            let mut groups = GroupingIdentifier {
                tmdb_id: 1396,
                name: "Breaking Bad".into(),
            }
            .identify(matched)
            .await?
            .groups;
            if !other.is_empty() {
                groups.push(Media::Tv {
                    detail: TvDetail {
                        id: 999,
                        name: "Other Show".into(),
                        first_air_date: "2020-01-01".into(),
                        number_of_episodes: 1,
                        number_of_seasons: 1,
                        origin_country: vec![],
                        original_language: "en".into(),
                        original_name: "Other Show".into(),
                        genres: vec![],
                        seasons: vec![],
                    },
                    files: other,
                });
            }
            Ok(IdentifyOutcome {
                groups,
                unmatched: vec![],
            })
        }
    }

    #[tokio::test]
    async fn rescan_imports_only_the_target_subscription() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 1396,
                media_type: SubscriptionMediaType::Tv,
                title_zh: None,
                title_en: Some("Breaking Bad".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![
                tv_record(1, "Breaking.Bad"),
                tv_record(2, "Other.Show"),
            ])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &MultiShowIdentifier,
            &TvFromGroupsImporter,
            &recorded,
        )
        .await
        .unwrap();

        let by_id: std::collections::HashMap<i64, _> =
            results.into_iter().map(|r| (r.id, r)).collect();
        assert_eq!(by_id[&1].status, "succeeded");
        assert_eq!(by_id[&1].title.as_deref(), Some("Breaking Bad"));
        assert_eq!(by_id[&2].status, "skipped");

        let finalized = import_repo.finalized.lock().unwrap();
        let summary: RecordSummary = serde_json::from_str(&finalized[0].1.summary_json).unwrap();
        assert_eq!(summary.items.len(), 1);
        match &summary.items[0] {
            SummaryItem::Tv { name, .. } => assert_eq!(name, "Breaking Bad"),
            other => panic!("expected tv summary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn movie_rescan_writes_one_import_record_for_multiple_files() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![sample_record(1), sample_record(2)])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &MovieIdentifier {
                tmdb_id: 27205,
                title: "Inception".into(),
            },
            &movie_importer(),
            &recorded,
        )
        .await
        .unwrap();

        assert_eq!(import_repo.created.lock().unwrap().len(), 1);
        assert_eq!(
            import_repo.created.lock().unwrap()[0].source,
            "subscription:1:Inception"
        );
        let mut ids: Vec<i64> = results.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);
        assert!(results.iter().all(|r| r.status == "succeeded"));
    }

    #[tokio::test]
    async fn empty_titles_return_empty_without_import_record() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: Some("".into()),
                title_en: Some("".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![sample_record(1)])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &FakeIdentifier,
            &movie_importer(),
            &recorded,
        )
        .await
        .unwrap();

        assert!(results.is_empty());
        assert!(import_repo.created.lock().unwrap().is_empty());
    }

    fn ranked_record(
        id: i64,
        rank: i64,
        file_name: &str,
        descriptions: Vec<String>,
    ) -> FileSearchRecord {
        FileSearchRecord {
            id,
            size: 1000,
            hash_type: "md5".into(),
            hash_value: format!("hash-{id}"),
            locations: vec![FileLocationRecord {
                file_name: file_name.into(),
                file_path: format!("/files/{id}"),
                descriptions,
            }],
            rank,
        }
    }

    #[tokio::test]
    async fn rescan_imports_filename_phrase_hits() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![ranked_record(
                3,
                0,
                "Inception.2010.mkv",
                vec!["share notes".into()],
            )])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &MovieIdentifier {
                tmdb_id: 27205,
                title: "Inception".into(),
            },
            &movie_importer(),
            &recorded,
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 3);
        assert_eq!(results[0].status, "succeeded");
    }

    #[tokio::test]
    async fn rescan_skips_description_only_and_partial_token_hits() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Love Is Blind".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(vec![
                ranked_record(1, 2, "unrelated.mkv", vec!["Love Is Blind".into()]),
                ranked_record(2, 2, "Love.Blind.mkv", vec![]),
                ranked_record(3, 0, "Love.Is.Blind.S09E11.mkv", vec![]),
                ranked_record(4, 1, "Love.Night.Is.Blind.mkv", vec![]),
            ])),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &MovieIdentifier {
                tmdb_id: 27205,
                title: "Love Is Blind".into(),
            },
            &movie_importer(),
            &recorded,
        )
        .await
        .unwrap();

        let mut ids: Vec<i64> = results.iter().map(|result| result.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![3, 4]);
        assert!(results.iter().all(|result| result.status == "succeeded"));
    }

    #[tokio::test]
    async fn rescan_truncates_qualified_hits_by_relevance_not_id() {
        let sub_repo = FakeSubRepo::default();
        SubscriptionRepository::create(
            &sub_repo,
            &SubscriptionCreateInput {
                tmdb_id: 27205,
                media_type: SubscriptionMediaType::Movie,
                title_zh: None,
                title_en: Some("Inception".into()),
                year: None,
                poster_path: None,
                overview: None,
            },
        )
        .await
        .unwrap();

        let mut records = (1..=100)
            .map(|id| ranked_record(id, 1, "Inception.cut.mkv", vec![]))
            .collect::<Vec<_>>();
        records.push(ranked_record(200, 0, "Inception.2010.1080p.mkv", vec![]));
        records.push(ranked_record(
            201,
            2,
            "unrelated.mkv",
            vec!["Inception".into()],
        ));

        let file_repo = FakeFileRepo {
            records: Arc::new(Mutex::new(records)),
        };
        let file_index = FileIndexService::new(file_repo);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo);
        let results = rescan_subscription(
            1,
            &sub_repo,
            &file_index,
            &MovieIdentifier {
                tmdb_id: 27205,
                title: "Inception".into(),
            },
            &movie_importer(),
            &recorded,
        )
        .await
        .unwrap();

        let mut ids: Vec<i64> = results.iter().map(|result| result.id).collect();
        ids.sort_unstable();
        assert_eq!(results.len(), 100);
        assert!(ids.contains(&200));
        assert!(!ids.contains(&100));
        assert!(!ids.contains(&201));
        assert!(results.iter().all(|result| result.status == "succeeded"));
    }
}
