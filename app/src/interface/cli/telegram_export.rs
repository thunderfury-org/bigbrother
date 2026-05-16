use std::{collections::HashSet, fs, io::BufReader};

use time::OffsetDateTime;
use tracing::info;

use crate::{
    application::file_index::FileIndexService,
    application::ports::{
        FileIndexRepository, TelegramExportStateRecord, TelegramExportStateRepository,
    },
    error::{AppError, AppResult},
    infrastructure::{
        repo::{
            file_index::SeaOrmFileIndexRepository,
            telegram_export_state::SeaOrmTelegramExportStateRepository,
        },
        services::ShareResolverRuntimeService,
        share::{file_parser::ShareFileParser, resolver::ShareResolver},
    },
    interface::telegram::{
        export::{ExportRoot, extract_media_sources, message_description},
        file_index::MediaSource,
    },
};

pub type TelegramExportIndexRuntimeRunner = TelegramExportIndexRunner<
    ShareResolverRuntimeService,
    SeaOrmFileIndexRepository,
    SeaOrmTelegramExportStateRepository,
>;

#[derive(Clone)]
pub struct TelegramExportIndexRunner<R, FileRepo, StateRepo> {
    share_resolver: R,
    file_index_service: FileIndexService<FileRepo>,
    state_repo: StateRepo,
}

impl<R, FileRepo, StateRepo> TelegramExportIndexRunner<R, FileRepo, StateRepo> {
    pub fn new(share_resolver: R, file_repo: FileRepo, state_repo: StateRepo) -> Self {
        Self {
            share_resolver,
            file_index_service: FileIndexService::new(file_repo),
            state_repo,
        }
    }
}

impl<R, FileRepo, StateRepo> TelegramExportIndexRunner<R, FileRepo, StateRepo>
where
    R: ShareResolver,
    FileRepo: FileIndexRepository,
    StateRepo: TelegramExportStateRepository,
{
    pub async fn run(&self, input: &str, retry_all: bool) -> AppResult<()> {
        info!(input = %input, retry_all, "starting telegram export file-index run");

        let file = fs::File::open(input)?;
        let reader = BufReader::new(file);
        let root: ExportRoot = serde_json::from_reader(reader).map_err(|err| {
            AppError::InvalidParameter(format!("invalid telegram export json: {err}"))
        })?;
        let mut seen = HashSet::new();
        let mut total = 0usize;
        let mut succeeded = 0usize;
        let mut failed = 0usize;

        for msg in &root.messages {
            let description = message_description(msg);
            for source in extract_media_sources(msg) {
                let (source_type, source_value) = match source {
                    MediaSource::ShareUrl(url) => ("url".to_owned(), url),
                    MediaSource::Fslink(fslink) => ("fslink".to_owned(), fslink),
                    MediaSource::TgDocument { .. } => continue,
                };
                let key = state_key(&source_type, &source_value);
                if !seen.insert(key) {
                    continue;
                }
                total += 1;

                let mut record = self
                    .state_repo
                    .get(&source_type, &source_value)
                    .await?
                    .unwrap_or_else(|| TelegramExportStateRecord {
                        source_type: source_type.clone(),
                        source_value: source_value.clone(),
                        description: description.clone(),
                        status: "pending".into(),
                        error: None,
                        attempt_count: 0,
                        first_seen_at: now_string(),
                        last_attempt_at: now_string(),
                    });

                if record.description.is_none() {
                    record.description = description.clone();
                }

                if !retry_all && record.status == "succeeded" {
                    info!(
                        source_type = %record.source_type,
                        source_value = %record.source_value,
                        "skipping succeeded source"
                    );
                    succeeded += 1;
                    continue;
                }

                record.attempt_count += 1;
                record.status = "pending".into();
                record.error = None;
                record.last_attempt_at = now_string();
                self.state_repo.upsert(&record).await?;
                info!(
                    source_type = %record.source_type,
                    source_value = %record.source_value,
                    attempt_count = record.attempt_count,
                    "processing source"
                );

                let result = match record.source_type.as_str() {
                    "url" => {
                        self.process_url(&record.source_value, record.description.clone())
                            .await
                    }
                    "fslink" => {
                        self.process_fslink(&record.source_value, record.description.clone())
                            .await
                    }
                    other => Err(AppError::InvalidParameter(format!(
                        "unsupported source type: {other}"
                    ))),
                };

                match result {
                    Ok(()) => {
                        record.status = "succeeded".into();
                        record.error = None;
                        succeeded += 1;
                        info!(
                            source_type = %record.source_type,
                            source_value = %record.source_value,
                            "source succeeded"
                        );
                    }
                    Err(err) => {
                        record.status = "failed".into();
                        record.error = Some(err.to_string());
                        failed += 1;
                        info!(
                            source_type = %record.source_type,
                            source_value = %record.source_value,
                            error = %err,
                            "source failed"
                        );
                    }
                }

                self.state_repo.upsert(&record).await?;
            }
        }

        info!(
            total,
            succeeded, failed, "finished telegram export file-index run"
        );

        Ok(())
    }

    async fn process_url(&self, url: &str, description: Option<String>) -> AppResult<()> {
        let raw_files = self
            .share_resolver
            .raw_files_from_url(url)
            .await?
            .ok_or_else(|| AppError::InvalidParameter(format!("unsupported share url: {url}")))?;
        self.file_index_service
            .record_raw_files(raw_files, description)
            .await
    }

    async fn process_fslink(&self, fslink: &str, description: Option<String>) -> AppResult<()> {
        let raw_files = ShareFileParser::parse_fslink(fslink)?;
        self.file_index_service
            .record_raw_files(raw_files, description)
            .await
    }
}

#[cfg(test)]
fn merge_sources_into_state(
    root: &ExportRoot,
    state: &mut std::collections::HashMap<String, TelegramExportStateRecord>,
) {
    for msg in &root.messages {
        let description = message_description(msg);
        for source in extract_media_sources(msg) {
            let (source_type, source_value) = match source {
                MediaSource::ShareUrl(url) => ("url".to_owned(), url),
                MediaSource::Fslink(fslink) => ("fslink".to_owned(), fslink),
                MediaSource::TgDocument { .. } => continue,
            };
            let key = state_key(&source_type, &source_value);
            state
                .entry(key)
                .or_insert_with(|| TelegramExportStateRecord {
                    source_type,
                    source_value,
                    description: description.clone(),
                    status: "pending".into(),
                    error: None,
                    attempt_count: 0,
                    first_seen_at: now_string(),
                    last_attempt_at: now_string(),
                });
        }
    }
}

fn state_key(source_type: &str, source_value: &str) -> String {
    format!("{source_type}\0{source_value}")
}

fn now_string() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::ports::{FileIndexRecordInput, FileSearchRecord},
        domain::share::{FileHash, RawFile},
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeResolver {
        files_by_url: HashMap<String, Vec<RawFile>>,
    }

    impl ShareResolver for FakeResolver {
        async fn raw_files_from_url(&self, url: &str) -> AppResult<Option<Vec<RawFile>>> {
            Ok(self.files_by_url.get(url).cloned())
        }
    }

    #[derive(Clone, Default)]
    struct SpyFileRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
    }

    impl FileIndexRepository for SpyFileRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
            self.recorded.lock().unwrap().extend_from_slice(files);
            Ok(())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct SpyStateRepo {
        records: Arc<Mutex<HashMap<String, TelegramExportStateRecord>>>,
    }

    impl TelegramExportStateRepository for SpyStateRepo {
        async fn get(
            &self,
            source_type: &str,
            source_value: &str,
        ) -> AppResult<Option<TelegramExportStateRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .get(&state_key(source_type, source_value))
                .cloned())
        }

        async fn upsert(&self, record: &TelegramExportStateRecord) -> AppResult<()> {
            self.records.lock().unwrap().insert(
                state_key(&record.source_type, &record.source_value),
                record.clone(),
            );
            Ok(())
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bigbrother-{name}-{}.json", std::process::id()))
    }

    #[test]
    fn merges_sources_deduplicates_and_keeps_first_description() {
        let root: ExportRoot = serde_json::from_str(
            r#"{
                "messages": [
                    {
                        "id": 1,
                        "text": "desc one\nhttps://pan.quark.cn/s/share-id?pwd=abc"
                    },
                    {
                        "id": 2,
                        "text": "desc two\nhttps://pan.quark.cn/s/share-id?pwd=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let mut state = HashMap::new();
        merge_sources_into_state(&root, &mut state);

        let values = state.values().collect::<Vec<_>>();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].description.as_deref(), Some("desc one"));
        assert_eq!(values[0].status, "pending");
    }

    #[tokio::test]
    async fn skips_succeeded_entries_by_default_and_retries_failed() {
        let input = temp_path("telegram-export-input-db");
        fs::write(
            &input,
            r#"{
                "messages": [
                    {
                        "id": 1,
                        "text": "desc\nhttps://pan.quark.cn/s/share-id?pwd=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        state_repo
            .upsert(&TelegramExportStateRecord {
                source_type: "url".into(),
                source_value: "https://pan.quark.cn/s/share-id?pwd=abc".into(),
                description: Some("desc".into()),
                status: "succeeded".into(),
                error: None,
                attempt_count: 2,
                first_seen_at: "2026-01-01T00:00:00Z".into(),
                last_attempt_at: "2026-01-01T00:00:00Z".into(),
            })
            .await
            .unwrap();

        let file_repo = SpyFileRepo::default();
        let runner =
            TelegramExportIndexRunner::new(FakeResolver::default(), file_repo.clone(), state_repo);
        runner.run(input.to_str().unwrap(), false).await.unwrap();

        assert!(file_repo.recorded.lock().unwrap().is_empty());
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn processes_fslink_and_records_file_index_with_description() {
        let input = temp_path("telegram-export-fslink-input-db");
        fs::write(
            &input,
            r#"{"messages":[{"id":1,"text":"资源说明\n123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv"}]}"#,
        )
        .unwrap();

        let file_repo = SpyFileRepo::default();
        let runner = TelegramExportIndexRunner::new(
            FakeResolver::default(),
            file_repo.clone(),
            SpyStateRepo::default(),
        );
        runner.run(input.to_str().unwrap(), false).await.unwrap();

        let recorded = file_repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].file_name, "movie.mkv");
        assert_eq!(recorded[0].description.as_deref(), Some("资源说明"));
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn retry_all_reprocesses_succeeded_entries() {
        let input = temp_path("telegram-export-retry-all-input-db");
        fs::write(
            &input,
            r#"{
                "messages": [
                    {
                        "id": 1,
                        "text": "desc\nhttps://pan.quark.cn/s/share-id?pwd=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        state_repo
            .upsert(&TelegramExportStateRecord {
                source_type: "url".into(),
                source_value: "https://pan.quark.cn/s/share-id?pwd=abc".into(),
                description: Some("desc".into()),
                status: "succeeded".into(),
                error: None,
                attempt_count: 1,
                first_seen_at: "2026-01-01T00:00:00Z".into(),
                last_attempt_at: "2026-01-01T00:00:00Z".into(),
            })
            .await
            .unwrap();

        let file_repo = SpyFileRepo::default();
        let runner = TelegramExportIndexRunner::new(
            FakeResolver {
                files_by_url: HashMap::from([(
                    "https://pan.quark.cn/s/share-id?pwd=abc".into(),
                    vec![RawFile {
                        id: None,
                        name: "movie.mkv".into(),
                        hash: FileHash::Md5("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                        size: 100,
                        path: "/".into(),
                    }],
                )]),
            },
            file_repo.clone(),
            state_repo,
        );
        runner.run(input.to_str().unwrap(), true).await.unwrap();

        assert_eq!(file_repo.recorded.lock().unwrap().len(), 1);
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn stores_error_message_when_url_processing_fails() {
        let input = temp_path("telegram-export-error-input-db");
        fs::write(
            &input,
            r#"{
                "messages": [
                    {
                        "id": 1,
                        "text": "desc\nhttps://pan.quark.cn/s/share-id?pwd=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        let runner = TelegramExportIndexRunner::new(
            FakeResolver::default(),
            SpyFileRepo::default(),
            state_repo.clone(),
        );
        runner.run(input.to_str().unwrap(), false).await.unwrap();

        let state = state_repo.records.lock().unwrap();
        assert_eq!(state.len(), 1);
        let record = state.values().next().unwrap();
        assert_eq!(record.status, "failed");
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|e| e.contains("unsupported share url"))
        );
        fs::remove_file(input).unwrap();
    }
}
