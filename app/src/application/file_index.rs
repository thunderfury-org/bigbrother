use sha2::{Digest, Sha256};

use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    domain::share::RawFile,
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
        let inputs = files
            .into_iter()
            .filter_map(|file| to_record_input(file, description.clone()))
            .collect::<Vec<_>>();

        self.repo.record_files(&inputs).await
    }

    pub async fn search_files(
        &self,
        keyword: &str,
        limit: u64,
    ) -> AppResult<Vec<FileSearchRecord>> {
        self.repo.search_files(keyword.trim(), limit).await
    }
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
    }

    #[tokio::test]
    async fn record_raw_files_filters_files_without_hash_or_size() {
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
                        name: "missing.mkv".into(),
                        hash: FileHash::Md5(" ".into()),
                        size: 10,
                        path: "/a".into(),
                    },
                    RawFile {
                        id: Some(3),
                        name: "movie.mkv".into(),
                        hash: FileHash::Md5(" ABCDEF ".into()),
                        size: 20,
                        path: " / ".into(),
                    },
                    RawFile {
                        id: Some(4),
                        name: "episode.mkv".into(),
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
}
