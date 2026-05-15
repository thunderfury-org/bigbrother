#[derive(Debug, Clone)]
pub struct RawFile {
    pub id: Option<i64>,
    pub name: String,
    pub etag: Etag,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum Etag {
    Md5(String),
    Sha1(String),
}

impl From<&str> for Etag {
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
    use super::{Etag, RawFile};

    #[test]
    fn etag_from_str_detects_sha1_and_lowercases() {
        let sha1 = Etag::from("ABCDEF0123456789ABCDEF0123456789ABCDEF01");
        let md5 = Etag::from("ABCDEF0123456789ABCDEF0123456789");

        assert!(
            matches!(sha1, Etag::Sha1(value) if value == "abcdef0123456789abcdef0123456789abcdef01")
        );
        assert!(matches!(md5, Etag::Md5(value) if value == "abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn raw_file_keeps_import_relevant_fields() {
        let file = RawFile {
            id: Some(1),
            name: "movie.mkv".into(),
            etag: Etag::from("etag"),
            size: 42,
            path: "/share".into(),
        };

        assert_eq!(file.id, Some(1));
        assert_eq!(file.name, "movie.mkv");
        assert_eq!(file.size, 42);
        assert_eq!(file.path, "/share");
    }
}
