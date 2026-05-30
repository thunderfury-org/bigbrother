use sha2::{Digest, Sha256};
use tracing::info;

use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    domain::media::{FileType, Metadata},
    domain::share::{FileHash, RawFile},
    error::AppResult,
};

#[derive(Clone)]
pub struct FileIndexService<R> {
    repo: R,
}

impl<R> FileIndexService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R> FileIndexService<R>
where
    R: FileIndexRepository,
{
    pub async fn record_raw_files(
        &self,
        files: Vec<RawFile>,
        description: Option<String>,
    ) -> AppResult<()> {
        let description = normalize_optional_text(description);
        let raw_file_count = files.len();
        let inputs = files
            .into_iter()
            .filter_map(|file| to_record_input(file, description.clone()))
            .collect::<Vec<_>>();
        info!(
            raw_file_count,
            file_index_record_count = inputs.len(),
            has_description = description.is_some(),
            "Prepared file index records from raw files"
        );

        self.repo.record_files(&inputs).await
    }

    pub async fn search_files(
        &self,
        keyword: &str,
        limit: u64,
    ) -> AppResult<Vec<FileSearchRecord>> {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return Ok(Vec::new());
        }
        self.repo.search_files(keyword, limit).await
    }

    pub async fn get_import_ready_files(
        &self,
        ids: &[i64],
    ) -> AppResult<Vec<(i64, RawFile, Vec<String>)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = self.repo.get_records_by_ids(ids).await?;
        Ok(records
            .into_iter()
            .filter_map(|record| {
                let (raw, descs) = fingerprint_to_raw_files(&record)?;
                Some((record.id, raw, descs))
            })
            .collect())
    }
}

fn fingerprint_to_raw_files(record: &FileSearchRecord) -> Option<(RawFile, Vec<String>)> {
    let best = select_richest_location(&record.locations)?;
    let hash = match record.hash_type.as_str() {
        "sha1" => FileHash::Sha1(record.hash_value.clone()),
        _ => FileHash::Md5(record.hash_value.clone()),
    };
    let all_descriptions = collect_unique_descriptions(&record.locations);
    let raw = RawFile {
        id: None,
        name: best.file_name.clone(),
        hash,
        size: record.size,
        path: best.file_path.clone(),
    };
    Some((raw, all_descriptions))
}

fn select_richest_location(
    locations: &[crate::application::ports::FileLocationRecord],
) -> Option<&crate::application::ports::FileLocationRecord> {
    locations.iter().max_by_key(|loc| {
        let meta = Metadata::parse(&loc.file_name);
        metadata_richness(&meta)
    })
}

fn metadata_richness(meta: &Metadata) -> usize {
    let mut count = 0;
    if !meta.titles.is_empty() {
        count += 1;
    }
    if !meta.year.is_empty() {
        count += 1;
    }
    if !meta.resolution.is_empty() {
        count += 1;
    }
    if !meta.quality.is_empty() {
        count += 1;
    }
    if !meta.video_codec.is_empty() {
        count += 1;
    }
    if !meta.audio_codec.is_empty() {
        count += 1;
    }
    if meta.season_number.is_some() {
        count += 1;
    }
    if meta.episode_number.is_some() {
        count += 1;
    }
    count
}

fn collect_unique_descriptions(
    locations: &[crate::application::ports::FileLocationRecord],
) -> Vec<String> {
    let mut seen = Vec::new();
    for loc in locations {
        for desc in &loc.descriptions {
            if !seen.iter().any(|existing: &String| existing == desc) {
                seen.push(desc.clone());
            }
        }
    }
    seen
}

pub fn location_hash(file_path: &str, file_name: &str) -> String {
    hash_hex(format!(
        "v1\0{}\0{}",
        normalize_file_path(file_path),
        file_name.trim()
    ))
}

pub fn description_hash(description: &str) -> String {
    hash_hex(description.trim())
}

fn to_record_input(file: RawFile, description: Option<String>) -> Option<FileIndexRecordInput> {
    if file.size == 0 {
        return None;
    }

    if FileType::from_file_name(&file.name) == FileType::Unknown {
        return None;
    }

    let hash_type = file.hash.hash_type().to_owned();
    let hash_value = normalize_hash(file.hash.hash_value())?;

    Some(FileIndexRecordInput {
        size: file.size,
        hash_type,
        hash_value,
        file_name: file.name.trim().to_owned(),
        file_path: normalize_file_path(&file.path),
        description,
    })
}

fn normalize_hash(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_file_path(value: &str) -> String {
    match value.trim() {
        "/" => String::new(),
        value => value.to_owned(),
    }
}

fn hash_hex(value: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(value.as_ref());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{application::ports::FileLocationRecord, domain::share::FileHash};

    #[derive(Clone, Default)]
    struct FakeRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
        records_by_id: Arc<Mutex<Vec<FileSearchRecord>>>,
    }

    impl FileIndexRepository for FakeRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
            self.recorded.lock().unwrap().extend_from_slice(files);
            Ok(())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(vec![FileSearchRecord {
                id: 1,
                size: 1,
                hash_type: "md5".into(),
                hash_value: "abc".into(),
                locations: vec![FileLocationRecord {
                    file_name: "movie.mkv".into(),
                    file_path: "/Movies".into(),
                    descriptions: vec!["desc".into()],
                }],
            }])
        }

        async fn get_records_by_ids(&self, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
            let records = self.records_by_id.lock().unwrap();
            Ok(records
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn record_raw_files_filters_non_media_files_and_files_without_hash_or_size() {
        let repo = FakeRepo::default();
        let service = FileIndexService::new(repo.clone());

        service
            .record_raw_files(
                vec![
                    RawFile {
                        id: Some(1),
                        name: "zero.mkv".into(),
                        hash: FileHash::Md5("abc".into()),
                        size: 0,
                        path: "/a".into(),
                    },
                    RawFile {
                        id: Some(2),
                        name: "poster.jpg".into(),
                        hash: FileHash::Md5("abc".into()),
                        size: 10,
                        path: "/a".into(),
                    },
                    RawFile {
                        id: Some(3),
                        name: "missing.mkv".into(),
                        hash: FileHash::Md5(" ".into()),
                        size: 10,
                        path: "/a".into(),
                    },
                    RawFile {
                        id: Some(4),
                        name: "movie.mkv".into(),
                        hash: FileHash::Md5(" ABCDEF ".into()),
                        size: 20,
                        path: " / ".into(),
                    },
                    RawFile {
                        id: Some(5),
                        name: "episode.srt".into(),
                        hash: FileHash::Sha1(" 0123456789012345678901234567890123456789 ".into()),
                        size: 30,
                        path: "/Shows".into(),
                    },
                ],
                Some(" desc ".into()),
            )
            .await
            .unwrap();

        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].hash_type, "md5");
        assert_eq!(recorded[0].hash_value, "abcdef");
        assert_eq!(recorded[0].file_path, "");
        assert_eq!(recorded[0].description.as_deref(), Some("desc"));
        assert_eq!(recorded[1].hash_type, "sha1");
        assert_eq!(
            recorded[1].hash_value,
            "0123456789012345678901234567890123456789"
        );
    }

    #[tokio::test]
    async fn search_files_delegates_to_repo() {
        let service = FileIndexService::new(FakeRepo::default());
        let results = service.search_files("movie", 10).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hash_type, "md5");
        assert_eq!(results[0].locations.len(), 1);
    }

    #[tokio::test]
    async fn search_files_returns_empty_for_empty_keyword() {
        let service = FileIndexService::new(FakeRepo::default());
        let results = service.search_files("", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_files_returns_empty_for_whitespace_keyword() {
        let service = FileIndexService::new(FakeRepo::default());
        let results = service.search_files("   \t ", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn hashes_location_with_version_and_null_separators() {
        assert_eq!(
            location_hash("/path", "file.mkv"),
            location_hash(" /path ", " file.mkv ")
        );
        assert_ne!(
            location_hash("/pa", "thfile"),
            location_hash("/path", "file")
        );
        assert_eq!(
            location_hash("/", "file.mkv"),
            location_hash("", "file.mkv")
        );
    }

    #[test]
    fn hashes_description_after_trim() {
        assert_eq!(description_hash(" hello "), description_hash("hello"));
        assert_ne!(description_hash("hello"), description_hash("Hello"));
    }

    #[tokio::test]
    async fn get_import_ready_files_selects_richest_location() {
        let repo = FakeRepo {
            records_by_id: Arc::new(Mutex::new(vec![FileSearchRecord {
                id: 42,
                size: 1000,
                hash_type: "md5".into(),
                hash_value: "abcdef".into(),
                locations: vec![
                    FileLocationRecord {
                        file_name: "abc123.mkv".into(),
                        file_path: "/raw".into(),
                        descriptions: vec!["some desc".into()],
                    },
                    FileLocationRecord {
                        file_name: "Movie.X.2024.1080p.BluRay.mkv".into(),
                        file_path: "/movies".into(),
                        descriptions: vec!["another desc".into()],
                    },
                ],
            }])),
            ..Default::default()
        };
        let service = FileIndexService::new(repo);

        let results = service.get_import_ready_files(&[42]).await.unwrap();
        assert_eq!(results.len(), 1);

        let (id, raw, descriptions) = &results[0];
        assert_eq!(*id, 42);
        assert_eq!(raw.name, "Movie.X.2024.1080p.BluRay.mkv");
        assert_eq!(raw.path, "/movies");
        assert_eq!(raw.size, 1000);
        assert!(raw.id.is_none());
        assert!(matches!(&raw.hash, FileHash::Md5(v) if v == "abcdef"));
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions.contains(&"some desc".into()));
        assert!(descriptions.contains(&"another desc".into()));
    }

    #[tokio::test]
    async fn get_import_ready_files_returns_empty_for_empty_ids() {
        let service = FileIndexService::new(FakeRepo::default());
        let results = service.get_import_ready_files(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn get_import_ready_files_handles_sha1_hash() {
        let repo = FakeRepo {
            records_by_id: Arc::new(Mutex::new(vec![FileSearchRecord {
                id: 1,
                size: 2000,
                hash_type: "sha1".into(),
                hash_value: "aabbccdd".into(),
                locations: vec![FileLocationRecord {
                    file_name: "video.mkv".into(),
                    file_path: "/path".into(),
                    descriptions: vec![],
                }],
            }])),
            ..Default::default()
        };
        let service = FileIndexService::new(repo);

        let results = service.get_import_ready_files(&[1]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1);
        assert!(matches!(&results[0].1.hash, FileHash::Sha1(v) if v == "aabbccdd"));
        assert!(results[0].2.is_empty());
    }
}
