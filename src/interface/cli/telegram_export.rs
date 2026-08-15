use std::{collections::HashSet, fs, io::BufReader, time::Duration};

use time::OffsetDateTime;
use tokio::time::sleep;
use tracing::info;

use crate::{
    application::file_index::FileIndexService,
    application::ports::{
        FileIndexRepository, ShareResolver, TelegramExportStateRecord,
        TelegramExportStateRepository,
    },
    error::{AppError, AppResult},
    infrastructure::share::file_parser::ShareFileParser,
    interface::telegram::{
        export::{ExportRoot, extract_media_sources, message_description},
        file_index::MediaSource,
    },
};

pub type TelegramExportIndexRuntimeRunner = TelegramExportIndexRunner;

const STATUS_PENDING: &str = "pending";
const STATUS_SUCCEEDED: &str = "succeeded";
const STATUS_FAILED: &str = "failed";
const STATUS_PERMANENT_FAILED: &str = "permanent_failed";

#[derive(Clone)]
pub struct TelegramExportIndexRunner {
    share_resolver: crate::application::ports::ShareResolverHandle,
    file_index_service: FileIndexService,
    state_repo: crate::application::ports::TelegramExportStateRepo,
}

impl TelegramExportIndexRunner {
    pub fn new(
        share_resolver: impl ShareResolver + Send + Sync + 'static,
        file_repo: impl FileIndexRepository + Send + Sync + 'static,
        state_repo: impl TelegramExportStateRepository + Send + Sync + 'static,
    ) -> Self {
        Self {
            share_resolver: std::sync::Arc::new(share_resolver),
            file_index_service: FileIndexService::new(file_repo),
            state_repo: std::sync::Arc::new(state_repo),
        }
    }
}

impl TelegramExportIndexRunner {
    pub async fn run(&self, input: &str, delay_ms: u64, retry_all: bool) -> AppResult<()> {
        info!(input = %input, delay_ms, retry_all, "starting telegram export file-index run");

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
                        status: STATUS_PENDING.into(),
                        error: None,
                        attempt_count: 0,
                        first_seen_at: now_string(),
                        last_attempt_at: now_string(),
                    });

                if record.description.is_none() {
                    record.description = description.clone();
                }

                if !retry_all
                    && (record.status == STATUS_SUCCEEDED
                        || record.status == STATUS_PERMANENT_FAILED)
                {
                    info!(
                        source_type = %record.source_type,
                        source_value = %record.source_value,
                        status = %record.status,
                        "skipping terminal source"
                    );
                    succeeded += 1;
                    continue;
                }

                record.attempt_count += 1;
                record.status = STATUS_PENDING.into();
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
                        record.status = STATUS_SUCCEEDED.into();
                        record.error = None;
                        succeeded += 1;
                        info!(
                            source_type = %record.source_type,
                            source_value = %record.source_value,
                            "source succeeded"
                        );
                    }
                    Err(err) => {
                        record.status = classify_error_status(&err).into();
                        record.error = Some(err.to_string());
                        failed += 1;
                        info!(
                            source_type = %record.source_type,
                            source_value = %record.source_value,
                            status = %record.status,
                            error = %err,
                            "source failed"
                        );
                    }
                }

                self.state_repo.upsert(&record).await?;
                if record.source_type == "url" && delay_ms > 0 {
                    info!(delay_ms, "sleeping after url source");
                    sleep(Duration::from_millis(delay_ms)).await;
                }
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
        // Unparseable fslinks can't recover on retry; mark them as non-retryable here so
        // classify_error_status keeps its simple "ExternalService(_, false) == permanent" rule.
        let raw_files = ShareFileParser::parse_fslink(fslink)
            .map_err(|err| AppError::ExternalService(err.to_string(), false))?;
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
                    status: STATUS_PENDING.into(),
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

fn classify_error_status(err: &AppError) -> &'static str {
    if matches!(err, AppError::ExternalService(_, false)) {
        STATUS_PERMANENT_FAILED
    } else {
        STATUS_FAILED
    }
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

        async fn get_records_by_ids(&self, _ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
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
                        "text": "desc one\nhttps://115.com/s/share-id?rc=abc"
                    },
                    {
                        "id": 2,
                        "text": "desc two\nhttps://115.com/s/share-id?rc=abc"
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
        assert_eq!(values[0].status, STATUS_PENDING);
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
                        "text": "desc\nhttps://115.com/s/share-id?rc=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        state_repo
            .upsert(&TelegramExportStateRecord {
                source_type: "url".into(),
                source_value: "https://115.com/s/share-id?rc=abc".into(),
                description: Some("desc".into()),
                status: STATUS_SUCCEEDED.into(),
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
        runner.run(input.to_str().unwrap(), 0, false).await.unwrap();

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
        runner.run(input.to_str().unwrap(), 0, false).await.unwrap();

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
                        "text": "desc\nhttps://115.com/s/share-id?rc=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        state_repo
            .upsert(&TelegramExportStateRecord {
                source_type: "url".into(),
                source_value: "https://115.com/s/share-id?rc=abc".into(),
                description: Some("desc".into()),
                status: STATUS_SUCCEEDED.into(),
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
                    "https://115.com/s/share-id?rc=abc".into(),
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
        runner.run(input.to_str().unwrap(), 0, true).await.unwrap();

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
                        "text": "desc\nhttps://115.com/s/share-id?rc=abc"
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
        runner.run(input.to_str().unwrap(), 0, false).await.unwrap();

        let state = state_repo.records.lock().unwrap();
        assert_eq!(state.len(), 1);
        let record = state.values().next().unwrap();
        assert_eq!(record.status, STATUS_FAILED);
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|e| e.contains("unsupported share url"))
        );
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn fslink_parse_failure_marks_record_permanent_failed() {
        let input = temp_path("telegram-export-fslink-invalid");
        fs::write(
            &input,
            r#"{"messages":[{"id":1,"text":"123FSLinkV2$1tR77Cnb9Rax8VlFVVtSeh#"}]}"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        let runner = TelegramExportIndexRunner::new(
            FakeResolver::default(),
            SpyFileRepo::default(),
            state_repo.clone(),
        );
        runner.run(input.to_str().unwrap(), 0, false).await.unwrap();

        let state = state_repo.records.lock().unwrap();
        let record = state.values().next().expect("expected one record");
        assert_eq!(record.source_type, "fslink");
        assert_eq!(record.status, STATUS_PERMANENT_FAILED);
        assert!(
            record
                .error
                .as_deref()
                .is_some_and(|e| e.contains("invalid fslink"))
        );
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn fslink_path_is_not_delayed() {
        let input = temp_path("telegram-export-fslink-no-delay");
        fs::write(
            &input,
            r#"{"messages":[{"id":1,"text":"123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv"}]}"#,
        )
        .unwrap();

        let runner = TelegramExportIndexRunner::new(
            FakeResolver::default(),
            SpyFileRepo::default(),
            SpyStateRepo::default(),
        );

        let start = std::time::Instant::now();
        runner
            .run(input.to_str().unwrap(), 200, false)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_millis(200));
        fs::remove_file(input).unwrap();
    }

    #[tokio::test]
    async fn skips_permanent_failed_entries_by_default() {
        let input = temp_path("telegram-export-skip-permanent-failed");
        fs::write(
            &input,
            r#"{
                "messages": [
                    {
                        "id": 1,
                        "text": "desc\nhttps://115.com/s/share-id?rc=abc"
                    }
                ]
            }"#,
        )
        .unwrap();

        let state_repo = SpyStateRepo::default();
        state_repo
            .upsert(&TelegramExportStateRecord {
                source_type: "url".into(),
                source_value: "https://115.com/s/share-id?rc=abc".into(),
                description: Some("desc".into()),
                status: STATUS_PERMANENT_FAILED.into(),
                error: Some("external service error: share cancelled, 此分享不存在".into()),
                attempt_count: 1,
                first_seen_at: "2026-01-01T00:00:00Z".into(),
                last_attempt_at: "2026-01-01T00:00:00Z".into(),
            })
            .await
            .unwrap();

        let file_repo = SpyFileRepo::default();
        let runner =
            TelegramExportIndexRunner::new(FakeResolver::default(), file_repo.clone(), state_repo);
        runner.run(input.to_str().unwrap(), 0, false).await.unwrap();

        assert!(file_repo.recorded.lock().unwrap().is_empty());
        fs::remove_file(input).unwrap();
    }

    #[test]
    fn classifies_non_retryable_external_service_as_permanent_failed() {
        let err = AppError::ExternalService("share cancelled, 此分享不存在".into(), false);

        assert_eq!(classify_error_status(&err), STATUS_PERMANENT_FAILED);
    }

    #[test]
    fn keeps_retryable_external_service_as_failed() {
        let err = AppError::ExternalService("too many requests".into(), true);
        assert_eq!(classify_error_status(&err), STATUS_FAILED);
    }

    #[test]
    fn keeps_non_external_service_errors_as_failed() {
        let err = AppError::InvalidParameter("unsupported share url".into());

        assert_eq!(classify_error_status(&err), STATUS_FAILED);
    }
}
