use std::collections::{HashMap, HashSet};

use crate::{
    application::{
        file_index::FileIndexService,
        import::{
            ImportedMedia, MediaIdentifier, MediaImporter, MetadataLookup,
            identify::{IdentifyOutcome, UnmatchedFile},
        },
        recorded_import::{RecordedImportService, import_outcome_from},
    },
    domain::{
        import::inner::{Media, MediaFile},
        import::policy::{insert_movie_media, insert_tv_media},
        import_record::{ImportSource, ImportSourceKind, RecordSummary, summarize},
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RecordSummary>,
}

impl ImportFileResult {
    pub(crate) fn skipped(id: i64, reason: &str) -> Self {
        Self {
            id,
            status: "skipped".into(),
            error: Some(reason.into()),
            ..Default::default()
        }
    }

    pub(crate) fn failed(id: i64, message: String) -> Self {
        Self {
            id,
            status: "failed".into(),
            error: Some(message),
            ..Default::default()
        }
    }

    pub(crate) fn from_imported(id: i64, imported: &[ImportedMedia]) -> Self {
        let outcomes = imported.iter().map(import_outcome_from).collect::<Vec<_>>();
        let (summary, status) = summarize(&outcomes);
        let (title, year, size) = summary.display_fields();
        Self {
            id,
            status: status.as_str().to_owned(),
            title,
            year,
            size,
            error: None,
            summary: Some(summary),
        }
    }
}

pub struct FileIndexImportService {
    file_index: FileIndexService,
}

impl FileIndexImportService {
    pub fn new(file_index: FileIndexService) -> Self {
        Self { file_index }
    }

    pub async fn import_from_fingerprints(
        &self,
        ids: &[i64],
        identifier: &dyn MediaIdentifier,
        importer: &dyn MediaImporter,
        recorded: &RecordedImportService,
    ) -> AppResult<Vec<ImportFileResult>> {
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

        let names: HashMap<i64, String> = ready_files
            .iter()
            .map(|(id, raw_file, _)| (*id, raw_file.name.clone()))
            .collect();
        let metadata_lookup = MetadataLookup::default();
        let mut media_files = Vec::new();
        for (_, raw_file, descriptions) in &ready_files {
            media_files.extend(
                metadata_lookup.build_media_files(vec![raw_file.clone()], descriptions.clone()),
            );
        }

        let identified = identify_files_with_fallback(identifier, media_files).await;
        let mut assigned = HashSet::new();
        let mut by_id: HashMap<i64, ImportFileResult> = HashMap::new();

        for (file_id, err) in identified.failed {
            assigned.insert(file_id);
            let name = names
                .get(&file_id)
                .cloned()
                .unwrap_or_else(|| format!("file {file_id}"));
            let source = ImportSource {
                kind: ImportSourceKind::FileIndex,
                raw: format!("file_index:{file_id}:{name}"),
            };
            let message = err.to_string();
            let _ = recorded.execute(source, || async { Err(err) }).await;
            by_id.insert(file_id, ImportFileResult::failed(file_id, message));
        }

        for group in identified.groups {
            let file_ids = media_file_ids(std::slice::from_ref(&group));
            assigned.extend(file_ids.iter().copied());
            let source = source_for_media(&group);
            match recorded
                .execute(source, || async {
                    importer.import_groups(vec![group], Vec::new()).await
                })
                .await
            {
                Ok(imported) => {
                    for file_id in file_ids {
                        let result = if imported.is_empty() {
                            ImportFileResult::skipped(file_id, "no media matched")
                        } else {
                            ImportFileResult::from_imported(file_id, &imported)
                        };
                        by_id.insert(file_id, result);
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "import from file index failed");
                    let message = err.to_string();
                    for file_id in file_ids {
                        by_id.insert(file_id, ImportFileResult::failed(file_id, message.clone()));
                    }
                }
            }
        }

        for (file_id, _, _) in &ready_files {
            if assigned.contains(file_id) {
                continue;
            }
            by_id.insert(
                *file_id,
                ImportFileResult::skipped(*file_id, "no media matched"),
            );
        }

        for (file_id, _, _) in ready_files {
            results.push(
                by_id
                    .remove(&file_id)
                    .unwrap_or_else(|| ImportFileResult::skipped(file_id, "no media matched")),
            );
        }

        Ok(results)
    }
}

pub(crate) struct IdentifiedFiles {
    pub groups: Vec<Media>,
    pub unmatched: Vec<UnmatchedFile>,
    pub failed: Vec<(i64, AppError)>,
}

pub(crate) async fn identify_files_with_fallback(
    identifier: &dyn MediaIdentifier,
    files: Vec<MediaFile>,
) -> IdentifiedFiles {
    match identifier.identify(files.clone()).await {
        Ok(IdentifyOutcome { groups, unmatched }) => IdentifiedFiles {
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
            IdentifiedFiles {
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

pub(crate) fn media_file_ids(groups: &[Media]) -> HashSet<i64> {
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

fn source_for_media(group: &Media) -> ImportSource {
    let (tmdb_id, title) = match group {
        Media::Movie { detail, .. } => (detail.id, detail.title.as_str()),
        Media::Tv { detail, .. } => (detail.id, detail.name.as_str()),
    };
    ImportSource {
        kind: ImportSourceKind::FileIndex,
        raw: format!("file_index:{tmdb_id}:{title}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::import::identify::IdentifyOutcome;
    use crate::application::import::{MediaIdentifier, MediaImporter};
    use crate::application::{
        import::ImportedMedia,
        ports::{
            FileIndexRecordInput, FileIndexRepository, FileLocationRecord, FileSearchRecord,
            ImportRecordCreate, ImportRecordFilter, ImportRecordFinalize, ImportRecordPage,
            ImportRecordPaging, ImportRecordRepository, ImportRecordView,
        },
    };
    use crate::domain::{
        import::inner::{Media, MediaFile},
        import::{MovieDetail, TvDetail},
        import_record::{ImportSourceKind, ImportStatus, SummaryItem},
    };
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

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
                    detail: movie_detail(self.tmdb_id, &self.title),
                    files,
                }],
                unmatched: vec![],
            })
        }
    }

    struct MappedMovieIdentifier;

    #[async_trait::async_trait]
    impl MediaIdentifier for MappedMovieIdentifier {
        async fn identify(&self, files: Vec<MediaFile>) -> AppResult<IdentifyOutcome> {
            Ok(IdentifyOutcome {
                groups: files
                    .into_iter()
                    .map(|file| {
                        let id = file.video.id.unwrap_or(0);
                        let (tmdb_id, title) = match id {
                            1 => (10, "Alpha"),
                            2 => (20, "Beta"),
                            _ => (id as u32, "Other"),
                        };
                        Media::Movie {
                            detail: movie_detail(tmdb_id, title),
                            files: vec![file],
                        }
                    })
                    .collect(),
                unmatched: vec![],
            })
        }
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

    struct IsolateIdentifier {
        tmdb_id: u32,
        title: String,
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
            MovieIdentifier {
                tmdb_id: self.tmdb_id,
                title: self.title.clone(),
            }
            .identify(files)
            .await
        }
    }

    struct FakeImporter {
        make_result: Box<dyn Fn() -> AppResult<Vec<ImportedMedia>> + Send + Sync>,
    }

    #[async_trait::async_trait]
    impl MediaImporter for FakeImporter {
        async fn import_groups(
            &self,
            _groups: Vec<Media>,
            _unmatched: Vec<UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            (self.make_result)()
        }
    }

    struct FromGroupsImporter;

    #[async_trait::async_trait]
    impl MediaImporter for FromGroupsImporter {
        async fn import_groups(
            &self,
            groups: Vec<Media>,
            _unmatched: Vec<UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            Ok(groups
                .into_iter()
                .map(|group| match group {
                    Media::Movie { detail, files } => ImportedMedia::Movie {
                        title: detail.title,
                        year: "2024".into(),
                        size: files.iter().map(|file| file.video.size).sum(),
                        cost: Duration::from_secs(1),
                        has_failed: false,
                    },
                    Media::Tv { detail, files } => {
                        let season = files.keys().next().copied().unwrap_or(1);
                        let mut episodes: Vec<u32> = files
                            .values()
                            .flat_map(|episodes| episodes.keys().copied())
                            .collect();
                        episodes.sort_unstable();
                        ImportedMedia::Tv {
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
                        }
                    }
                })
                .collect())
        }
    }

    struct FailTitleImporter {
        fail_title: &'static str,
    }

    #[async_trait::async_trait]
    impl MediaImporter for FailTitleImporter {
        async fn import_groups(
            &self,
            groups: Vec<Media>,
            _unmatched: Vec<UnmatchedFile>,
        ) -> AppResult<Vec<ImportedMedia>> {
            let mut imported = Vec::new();
            for group in groups {
                match group {
                    Media::Movie { detail, files: _ } if detail.title == self.fail_title => {
                        return Err(AppError::Network("timeout".into(), true));
                    }
                    Media::Movie { detail, files } => imported.push(ImportedMedia::Movie {
                        title: detail.title,
                        year: "2024".into(),
                        size: files.iter().map(|file| file.video.size).sum(),
                        cost: Duration::from_secs(1),
                        has_failed: false,
                    }),
                    Media::Tv { .. } => {}
                }
            }
            Ok(imported)
        }
    }

    fn movie_detail(id: u32, title: &str) -> MovieDetail {
        MovieDetail {
            id,
            title: title.into(),
            adult: false,
            genres: vec![],
            original_language: "en".into(),
            original_title: title.into(),
            origin_country: vec![],
            release_date: "2024-01-01".into(),
        }
    }

    fn sample_record(id: i64) -> FileSearchRecord {
        FileSearchRecord {
            id,
            size: 1000,
            hash_type: "md5".into(),
            hash_value: format!("hash-{id}"),
            locations: vec![FileLocationRecord {
                file_name: format!("Movie.{id}.1080p.mkv"),
                file_path: format!("/movies/{id}"),
                descriptions: vec!["desc1".into()],
            }],
            rank: 0,
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

    fn bad_record(id: i64) -> FileSearchRecord {
        FileSearchRecord {
            id,
            size: 1000,
            hash_type: "md5".into(),
            hash_value: format!("hash-{id}"),
            locations: vec![FileLocationRecord {
                file_name: "BAD.mkv".into(),
                file_path: "/movies/bad".into(),
                descriptions: vec!["desc".into()],
            }],
            rank: 0,
        }
    }

    fn service_with_records(records: Vec<FileSearchRecord>) -> FileIndexImportService {
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

    fn movie_identifier() -> MovieIdentifier {
        MovieIdentifier {
            tmdb_id: 42,
            title: "Movie".into(),
        }
    }

    #[tokio::test]
    async fn empty_ids_returns_error() {
        let service = service_with_records(vec![]);
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let err = service
            .import_from_fingerprints(&[], &FakeIdentifier, &importer, &recorded)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn no_matching_records_returns_error() {
        let service = service_with_records(vec![]);
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let err = service
            .import_from_fingerprints(&[999], &FakeIdentifier, &importer, &recorded)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
    }

    #[tokio::test]
    async fn successful_movie_import() {
        let service = service_with_records(vec![sample_record(1)]);
        let importer = movie_importer();
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &movie_identifier(), &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].status, "succeeded");
        assert_eq!(results[0].title.as_deref(), Some("Movie"));
        assert_eq!(results[0].year.as_deref(), Some("2024"));
        assert_eq!(results[0].size, Some(1_000_000));
        match &results[0].summary.as_ref().expect("movie summary").items[..] {
            [
                SummaryItem::Movie {
                    title,
                    year,
                    size,
                    succeeded,
                    ..
                },
            ] => {
                assert_eq!(title, "Movie");
                assert_eq!(year, "2024");
                assert_eq!(*size, 1_000_000);
                assert!(*succeeded);
            }
            other => panic!("expected movie summary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn skipped_item_reports_skipped_status() {
        let service = service_with_records(vec![sample_record(1)]);
        let importer = FakeImporter {
            make_result: Box::new(|| {
                Ok(vec![ImportedMedia::Skipped {
                    count: 1,
                    files: vec!["test.mkv".into()],
                }])
            }),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &movie_identifier(), &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
        let summary = results[0].summary.as_ref().expect("skipped summary");
        assert_eq!(summary.skipped_files, vec!["test.mkv"]);
    }

    #[tokio::test]
    async fn empty_import_result_reports_skipped() {
        let service = service_with_records(vec![sample_record(1)]);
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &movie_identifier(), &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
        assert_eq!(results[0].error.as_deref(), Some("no media matched"));
        assert!(results[0].summary.is_none());
    }

    #[tokio::test]
    async fn unmatched_files_skip_without_import_record() {
        let service = service_with_records(vec![sample_record(1)]);
        let importer = FakeImporter {
            make_result: Box::new(|| Ok(vec![])),
        };
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = service
            .import_from_fingerprints(&[1], &FakeIdentifier, &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "skipped");
        assert_eq!(results[0].error.as_deref(), Some("no media matched"));
        assert!(results[0].summary.is_none());
        assert!(import_repo.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn transfer_error_reports_failed() {
        let service = service_with_records(vec![sample_record(1)]);
        let importer = FakeImporter {
            make_result: Box::new(|| Err(AppError::Network("timeout".into(), true))),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());

        let results = service
            .import_from_fingerprints(&[1], &movie_identifier(), &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[0].status, "failed");
        assert!(results[0].error.as_deref().unwrap().contains("timeout"));
        assert!(results[0].summary.is_none());
    }

    #[tokio::test]
    async fn same_tv_episodes_create_one_import_record() {
        let service = service_with_records(vec![
            tv_record(1, "Breaking.Bad"),
            tv_record(2, "Breaking.Bad"),
        ]);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());
        let identifier = GroupingIdentifier {
            tmdb_id: 1396,
            name: "Breaking Bad".into(),
        };

        let results = service
            .import_from_fingerprints(&[1, 2], &identifier, &FromGroupsImporter, &recorded)
            .await
            .unwrap();

        let created = import_repo.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].source_kind, ImportSourceKind::FileIndex);
        assert_eq!(created[0].source, "file_index:1396:Breaking Bad");

        let mut ids: Vec<i64> = results.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2]);
        assert!(results.iter().all(|r| r.status == "succeeded"));
        assert!(
            results
                .iter()
                .all(|r| r.title.as_deref() == Some("Breaking Bad"))
        );
        for result in &results {
            match &result.summary.as_ref().expect("tv summary").items[..] {
                [
                    SummaryItem::Tv {
                        name,
                        season,
                        episodes,
                        ..
                    },
                ] => {
                    assert_eq!(name, "Breaking Bad");
                    assert_eq!(*season, 1);
                    assert_eq!(episodes.len(), 2);
                }
                other => panic!("expected tv summary, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn tv_partial_failure_reports_partially_failed_summary() {
        let service = service_with_records(vec![tv_record(1, "Show")]);
        let importer = FakeImporter {
            make_result: Box::new(|| {
                Ok(vec![ImportedMedia::Tv {
                    name: "Show".into(),
                    year: "2025".into(),
                    season: 1,
                    episodes: vec![1],
                    missing_episodes: vec![],
                    max_episode_number: 2,
                    total_size: 2048,
                    number_of_episodes: 2,
                    cost: Duration::from_millis(30),
                    has_failed: true,
                    failed_episodes: vec![2],
                }])
            }),
        };
        let recorded = RecordedImportService::new(FakeImportRepo::default());
        let identifier = GroupingIdentifier {
            tmdb_id: 99,
            name: "Show".into(),
        };

        let results = service
            .import_from_fingerprints(&[1], &identifier, &importer, &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "partially_failed");
        match &results[0].summary.as_ref().expect("tv summary").items[..] {
            [SummaryItem::Tv { name, episodes, .. }] => {
                assert_eq!(name, "Show");
                assert_eq!(episodes.len(), 2);
                assert!(episodes.iter().any(|ep| ep.episode == 1 && ep.succeeded));
                assert!(episodes.iter().any(|ep| ep.episode == 2 && !ep.succeeded));
            }
            other => panic!("expected tv summary, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mixed_movies_create_one_record_each() {
        let service = service_with_records(vec![sample_record(1), sample_record(2)]);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = service
            .import_from_fingerprints(
                &[1, 2],
                &MappedMovieIdentifier,
                &FromGroupsImporter,
                &recorded,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        let mut created: Vec<String> = import_repo
            .created
            .lock()
            .unwrap()
            .iter()
            .map(|record| record.source.clone())
            .collect();
        created.sort();
        assert_eq!(
            created,
            vec![
                "file_index:10:Alpha".to_string(),
                "file_index:20:Beta".to_string()
            ]
        );

        let by_id: std::collections::HashMap<_, _> =
            results.into_iter().map(|r| (r.id, r)).collect();
        assert_eq!(by_id[&1].status, "succeeded");
        assert_eq!(by_id[&1].title.as_deref(), Some("Alpha"));
        assert_eq!(by_id[&2].status, "succeeded");
        assert_eq!(by_id[&2].title.as_deref(), Some("Beta"));
    }

    #[tokio::test]
    async fn missing_ids_are_skipped_and_valid_files_import() {
        let service = service_with_records(vec![sample_record(1)]);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());

        let results = service
            .import_from_fingerprints(&[1, 999], &movie_identifier(), &movie_importer(), &recorded)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 999);
        assert_eq!(results[0].status, "skipped");
        assert_eq!(results[1].id, 1);
        assert_eq!(results[1].status, "succeeded");
        assert_eq!(import_repo.created.lock().unwrap().len(), 1);
        assert_eq!(
            import_repo.created.lock().unwrap()[0].source,
            "file_index:42:Movie"
        );
    }

    #[tokio::test]
    async fn one_media_transfer_failure_does_not_abort_other_media() {
        let service = service_with_records(vec![sample_record(1), sample_record(2)]);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());
        let importer = FailTitleImporter {
            fail_title: "Alpha",
        };

        let results = service
            .import_from_fingerprints(&[1, 2], &MappedMovieIdentifier, &importer, &recorded)
            .await
            .unwrap();

        let by_id: std::collections::HashMap<_, _> =
            results.into_iter().map(|r| (r.id, r)).collect();
        assert_eq!(by_id[&1].status, "failed");
        assert!(by_id[&1].error.as_deref().unwrap().contains("timeout"));
        assert_eq!(by_id[&2].status, "succeeded");
        assert_eq!(by_id[&2].title.as_deref(), Some("Beta"));

        let created = import_repo.created.lock().unwrap();
        assert_eq!(created.len(), 2);
        let finalized = import_repo.finalized.lock().unwrap();
        let statuses: Vec<_> = finalized.iter().map(|(_, update)| update.status).collect();
        assert!(statuses.contains(&ImportStatus::Failed));
        assert!(statuses.contains(&ImportStatus::Succeeded));
    }

    #[tokio::test]
    async fn identify_failure_records_failed_file_and_imports_rest() {
        let service = service_with_records(vec![sample_record(1), bad_record(2)]);
        let import_repo = FakeImportRepo::default();
        let recorded = RecordedImportService::new(import_repo.clone());
        let identifier = IsolateIdentifier {
            tmdb_id: 42,
            title: "Movie".into(),
        };

        let results = service
            .import_from_fingerprints(&[1, 2], &identifier, &movie_importer(), &recorded)
            .await
            .unwrap();

        let by_id: std::collections::HashMap<_, _> =
            results.into_iter().map(|r| (r.id, r)).collect();
        assert_eq!(by_id[&1].status, "succeeded");
        assert_eq!(by_id[&2].status, "failed");
        assert!(by_id[&2].error.as_deref().unwrap().contains("bad file"));

        let mut sources: Vec<String> = import_repo
            .created
            .lock()
            .unwrap()
            .iter()
            .map(|record| record.source.clone())
            .collect();
        sources.sort();
        assert_eq!(
            sources,
            vec![
                "file_index:2:BAD.mkv".to_string(),
                "file_index:42:Movie".to_string()
            ]
        );
    }
}
