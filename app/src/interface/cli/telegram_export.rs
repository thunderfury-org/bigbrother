use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    application::file_index::FileIndexService,
    application::ports::FileIndexRepository,
    error::{AppError, AppResult},
    infrastructure::{
        repo::file_index::SeaOrmFileIndexRepository,
        services::ShareResolverRuntimeService,
        share::{file_parser::ShareFileParser, resolver::ShareResolver},
    },
    interface::telegram::{
        export::{ExportRoot, extract_media_sources, message_description},
        file_index::MediaSource,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelegramExportSourceStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramExportStateEntry {
    pub source_type: String,
    pub source_value: String,
    pub description: Option<String>,
    pub status: TelegramExportSourceStatus,
    pub error: Option<String>,
    pub attempt_count: u64,
    pub first_seen_at: String,
    pub last_attempt_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TelegramExportState {
    pub version: u64,
    pub entries: Vec<TelegramExportStateEntry>,
}

pub type TelegramExportIndexRuntimeRunner =
    TelegramExportIndexRunner<ShareResolverRuntimeService, SeaOrmFileIndexRepository>;

#[derive(Clone)]
pub struct TelegramExportIndexRunner<R, Repo> {
    share_resolver: R,
    file_index_service: FileIndexService<Repo>,
}

impl<R, Repo> TelegramExportIndexRunner<R, Repo> {
    pub fn new(share_resolver: R, repo: Repo) -> Self {
        Self {
            share_resolver,
            file_index_service: FileIndexService::new(repo),
        }
    }
}

impl<R, Repo> TelegramExportIndexRunner<R, Repo>
where
    R: ShareResolver,
    Repo: FileIndexRepository,
{
    pub async fn run(&self, input: &str, state_file: &str, retry_all: bool) -> AppResult<()> {
        let content = fs::read_to_string(input)?;
        let root: ExportRoot = serde_json::from_str(&content).map_err(|err| {
            AppError::InvalidParameter(format!("invalid telegram export json: {err}"))
        })?;
        let mut state = load_state(state_file)?;

        merge_sources_into_state(&root, &mut state);
        let mut dirty = false;
        let mut processed_since_flush = 0usize;
        const STATE_FLUSH_INTERVAL: usize = 10;

        for index in 0..state.entries.len() {
            let should_process = {
                let entry = &state.entries[index];
                retry_all || !matches!(entry.status, TelegramExportSourceStatus::Succeeded)
            };
            if !should_process {
                continue;
            }

            let entry = &mut state.entries[index];
            entry.attempt_count += 1;
            entry.last_attempt_at = now_string();

            let result = match entry.source_type.as_str() {
                "url" => {
                    self.process_url(&entry.source_value, entry.description.clone())
                        .await
                }
                "fslink" => {
                    self.process_fslink(&entry.source_value, entry.description.clone())
                        .await
                }
                other => Err(AppError::InvalidParameter(format!(
                    "unsupported source type: {other}"
                ))),
            };

            match result {
                Ok(()) => {
                    entry.status = TelegramExportSourceStatus::Succeeded;
                    entry.error = None;
                }
                Err(err) => {
                    entry.status = TelegramExportSourceStatus::Failed;
                    entry.error = Some(err.to_string());
                }
            }

            dirty = true;
            processed_since_flush += 1;
            if processed_since_flush >= STATE_FLUSH_INTERVAL {
                save_state(state_file, &state)?;
                dirty = false;
                processed_since_flush = 0;
            }
        }

        if dirty {
            save_state(state_file, &state)?;
        }

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

fn merge_sources_into_state(root: &ExportRoot, state: &mut TelegramExportState) {
    let mut by_key = state
        .entries
        .drain(..)
        .map(|entry| (state_key(&entry.source_type, &entry.source_value), entry))
        .collect::<HashMap<_, _>>();

    for msg in &root.messages {
        let description = message_description(msg);
        for source in extract_media_sources(msg) {
            let (source_type, source_value) = match source {
                MediaSource::ShareUrl(url) => ("url".to_owned(), url),
                MediaSource::Fslink(fslink) => ("fslink".to_owned(), fslink),
                MediaSource::TgDocument { .. } => continue,
            };
            let key = state_key(&source_type, &source_value);
            by_key
                .entry(key)
                .or_insert_with(|| TelegramExportStateEntry {
                    source_type,
                    source_value,
                    description: description.clone(),
                    status: TelegramExportSourceStatus::Failed,
                    error: None,
                    attempt_count: 0,
                    first_seen_at: now_string(),
                    last_attempt_at: now_string(),
                });
        }
    }

    let mut entries = by_key.into_values().collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        a.source_type
            .cmp(&b.source_type)
            .then(a.source_value.cmp(&b.source_value))
    });
    state.version = 1;
    state.entries = entries;
}

fn load_state(path: &str) -> AppResult<TelegramExportState> {
    if !Path::new(path).exists() {
        return Ok(TelegramExportState::default());
    }

    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|err| AppError::InvalidParameter(format!("invalid telegram export state: {err}")))
}

fn save_state(path: &str, state: &TelegramExportState) -> AppResult<()> {
    let content = serde_json::to_vec_pretty(state)?;
    fs::write(path, content)?;
    Ok(())
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
        application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
        domain::share::{FileHash, RawFile},
    };
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

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
    struct SpyRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
    }

    impl FileIndexRepository for SpyRepo {
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

    fn temp_path(name: &str) -> PathBuf {
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

        let mut state = TelegramExportState::default();
        merge_sources_into_state(&root, &mut state);

        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].description.as_deref(), Some("desc one"));
        assert!(matches!(
            state.entries[0].status,
            TelegramExportSourceStatus::Failed
        ));
    }

    #[tokio::test]
    async fn skips_succeeded_entries_by_default_and_retries_failed() {
        let input = temp_path("telegram-export-input");
        let state_file = temp_path("telegram-export-state");
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
        save_state(
            state_file.to_str().unwrap(),
            &TelegramExportState {
                version: 1,
                entries: vec![TelegramExportStateEntry {
                    source_type: "url".into(),
                    source_value: "https://pan.quark.cn/s/share-id?pwd=abc".into(),
                    description: Some("desc".into()),
                    status: TelegramExportSourceStatus::Succeeded,
                    error: None,
                    attempt_count: 2,
                    first_seen_at: "2026-01-01T00:00:00Z".into(),
                    last_attempt_at: "2026-01-01T00:00:00Z".into(),
                }],
            },
        )
        .unwrap();

        let repo = SpyRepo::default();
        let runner = TelegramExportIndexRunner::new(FakeResolver::default(), repo.clone());
        runner
            .run(input.to_str().unwrap(), state_file.to_str().unwrap(), false)
            .await
            .unwrap();

        assert!(repo.recorded.lock().unwrap().is_empty());

        fs::remove_file(input).unwrap();
        fs::remove_file(state_file).unwrap();
    }

    #[tokio::test]
    async fn processes_fslink_and_records_file_index_with_description() {
        let input = temp_path("telegram-export-fslink-input");
        let state_file = temp_path("telegram-export-fslink-state");
        fs::write(
            &input,
            r#"{"messages":[{"id":1,"text":"资源说明\n123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv"}]}"#,
        )
        .unwrap();

        let repo = SpyRepo::default();
        let runner = TelegramExportIndexRunner::new(FakeResolver::default(), repo.clone());
        runner
            .run(input.to_str().unwrap(), state_file.to_str().unwrap(), false)
            .await
            .unwrap();

        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].file_name, "movie.mkv");
        assert_eq!(recorded[0].description.as_deref(), Some("资源说明"));

        fs::remove_file(input).unwrap();
        fs::remove_file(state_file).unwrap();
    }

    #[tokio::test]
    async fn retry_all_reprocesses_succeeded_entries() {
        let input = temp_path("telegram-export-retry-all-input");
        let state_file = temp_path("telegram-export-retry-all-state");
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
        save_state(
            state_file.to_str().unwrap(),
            &TelegramExportState {
                version: 1,
                entries: vec![TelegramExportStateEntry {
                    source_type: "url".into(),
                    source_value: "https://pan.quark.cn/s/share-id?pwd=abc".into(),
                    description: Some("desc".into()),
                    status: TelegramExportSourceStatus::Succeeded,
                    error: None,
                    attempt_count: 1,
                    first_seen_at: "2026-01-01T00:00:00Z".into(),
                    last_attempt_at: "2026-01-01T00:00:00Z".into(),
                }],
            },
        )
        .unwrap();

        let repo = SpyRepo::default();
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
            repo.clone(),
        );
        runner
            .run(input.to_str().unwrap(), state_file.to_str().unwrap(), true)
            .await
            .unwrap();

        assert_eq!(repo.recorded.lock().unwrap().len(), 1);

        fs::remove_file(input).unwrap();
        fs::remove_file(state_file).unwrap();
    }
}
