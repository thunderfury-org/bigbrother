#[derive(Debug, Clone)]
pub struct RawFile {
    pub id: Option<i64>,
    pub name: String,
    pub hash: FileHash,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum FileHash {
    Md5(String),
    Sha1(String),
}

impl FileHash {
    pub fn hash_type(&self) -> &'static str {
        match self {
            Self::Md5(_) => "md5",
            Self::Sha1(_) => "sha1",
        }
    }

    pub fn hash_value(&self) -> &str {
        match self {
            Self::Md5(value) | Self::Sha1(value) => value,
        }
    }
}

impl From<&str> for FileHash {
    fn from(s: &str) -> Self {
        if s.len() == 40 {
            Self::Sha1(s.to_lowercase())
        } else {
            Self::Md5(s.to_lowercase())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FileHash, RawFile};

    #[test]
    fn hash_from_str_detects_sha1_and_lowercases() {
        let sha1 = FileHash::from("ABCDEF0123456789ABCDEF0123456789ABCDEF01");
        let md5 = FileHash::from("ABCDEF0123456789ABCDEF0123456789");

        assert!(
            matches!(sha1, FileHash::Sha1(value) if value == "abcdef0123456789abcdef0123456789abcdef01")
        );
        assert!(matches!(md5, FileHash::Md5(value) if value == "abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn raw_file_keeps_import_relevant_fields() {
        let file = RawFile {
            id: Some(1),
            name: "movie.mkv".into(),
            hash: FileHash::from("hash"),
            size: 42,
            path: "/share".into(),
        };

        assert_eq!(file.id, Some(1));
        assert_eq!(file.name, "movie.mkv");
        assert_eq!(file.size, 42);
        assert_eq!(file.path, "/share");
    }
}
