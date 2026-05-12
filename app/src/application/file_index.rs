use sha2::{Digest, Sha256};

use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    domain::import::inner::{Etag, RawFile},
    error::AppResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenFileHash {
    Md5(String),
    Sha1(String),
    #[allow(dead_code)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenFile {
    pub size: u64,
    pub hash: SeenFileHash,
    pub file_name: String,
    pub file_path: String,
}

impl SeenFile {
    pub fn from_raw_file(file: &RawFile) -> Self {
        let hash = match &file.etag {
            Etag::Md5(value) => SeenFileHash::Md5(value.clone()),
            Etag::Sha1(value) => SeenFileHash::Sha1(value.clone()),
        };

        Self {
            size: file.size,
            hash,
            file_name: file.name.clone(),
            file_path: file.path.clone(),
        }
    }
}

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
    pub async fn record_seen_files(
        &self,
        files: Vec<SeenFile>,
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

fn to_record_input(file: SeenFile, description: Option<String>) -> Option<FileIndexRecordInput> {
    if file.size == 0 {
        return None;
    }

    let (md5, sha1) = match file.hash {
        SeenFileHash::Md5(value) => (normalize_hash(value), None),
        SeenFileHash::Sha1(value) => (None, normalize_hash(value)),
        SeenFileHash::Unknown(value) => match normalize_hash(value) {
            Some(hash) if hash.len() == 32 => (Some(hash), None),
            Some(hash) if hash.len() == 40 => (None, Some(hash)),
            _ => (None, None),
        },
    };

    if md5.is_none() && sha1.is_none() {
        return None;
    }

    Some(FileIndexRecordInput {
        size: file.size,
        md5,
        sha1,
        file_name: file.file_name.trim().to_owned(),
        file_path: normalize_file_path(&file.file_path),
        description,
    })
}

fn normalize_hash(value: String) -> Option<String> {
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
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn record_seen_files_filters_files_without_hash_or_size() {
        let repo = FakeRepo::default();
        let service = FileIndexService::new(repo.clone());

        service
            .record_seen_files(
                vec![
                    SeenFile {
                        size: 0,
                        hash: SeenFileHash::Md5("abc".into()),
                        file_name: "zero.mkv".into(),
                        file_path: "/a".into(),
                    },
                    SeenFile {
                        size: 10,
                        hash: SeenFileHash::Unknown(String::new()),
                        file_name: "missing.mkv".into(),
                        file_path: "/a".into(),
                    },
                    SeenFile {
                        size: 20,
                        hash: SeenFileHash::Md5(" ABCDEF ".into()),
                        file_name: "movie.mkv".into(),
                        file_path: " / ".into(),
                    },
                    SeenFile {
                        size: 30,
                        hash: SeenFileHash::Sha1(
                            " 0123456789012345678901234567890123456789 ".into(),
                        ),
                        file_name: "episode.mkv".into(),
                        file_path: "/Shows".into(),
                    },
                ],
                Some(" desc ".into()),
            )
            .await
            .unwrap();

        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].md5.as_deref(), Some("abcdef"));
        assert_eq!(recorded[0].sha1, None);
        assert_eq!(recorded[0].file_path, "");
        assert_eq!(recorded[0].description.as_deref(), Some("desc"));
        assert_eq!(
            recorded[1].sha1.as_deref(),
            Some("0123456789012345678901234567890123456789")
        );
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
