use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedBigbrotherStrm {
    pub file_id: i64,
}

#[derive(Debug, Clone)]
pub struct BigbrotherStrmMatcher {
    advertise_base_url: String,
    strm_path_prefix: String,
}

impl BigbrotherStrmMatcher {
    pub fn new(advertise_base_url: impl Into<String>, strm_path_prefix: impl Into<String>) -> Self {
        Self {
            advertise_base_url: advertise_base_url.into().trim_end_matches('/').to_owned(),
            strm_path_prefix: normalize_prefix(strm_path_prefix.into().as_str()),
        }
    }

    pub fn parse(&self, raw: &str) -> Option<ParsedBigbrotherStrm> {
        let url = parse_url_like(raw, &self.advertise_base_url).ok()?;
        let base = Url::parse(self.advertise_base_url.as_str()).ok()?;
        if url.scheme() != base.scheme()
            || url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
        {
            return None;
        }

        if !path_matches_prefix(url.path(), self.strm_path_prefix.as_str()) {
            return None;
        }

        let file_id = url
            .query_pairs()
            .find_map(|(key, value)| (key == "file_id").then_some(value))
            .and_then(|value| value.parse::<i64>().ok())?;

        Some(ParsedBigbrotherStrm { file_id })
    }
}

pub fn emby_token_query(raw_url: &str) -> Option<String> {
    let url = parse_url_like(raw_url, "http://localhost").ok()?;
    url.query_pairs().find_map(|(key, value)| {
        let lower = key.to_ascii_lowercase();
        (lower == "api_key" || lower == "x-emby-token").then(|| format!("{key}={value}"))
    })
}

pub fn media_source_ids_match(left: &str, right: &str) -> bool {
    strip_media_source_prefix(left) == strip_media_source_prefix(right)
}

fn strip_media_source_prefix(value: &str) -> &str {
    value.strip_prefix("mediasource_").unwrap_or(value)
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim();
    let prefixed = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    prefixed.trim_end_matches('/').to_owned()
}

fn path_matches_prefix(path: &str, prefix: &str) -> bool {
    path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

fn parse_url_like(raw: &str, base_url: &str) -> Result<Url, url::ParseError> {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)
    } else {
        Url::parse(base_url)?.join(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher() -> BigbrotherStrmMatcher {
        BigbrotherStrmMatcher::new("http://bb.example:3100", "/d")
    }

    #[test]
    fn parses_absolute_bigbrother_strm_url() {
        let parsed = matcher()
            .parse("http://bb.example:3100/d/movies/Inception.mkv?file_id=42")
            .unwrap();

        assert_eq!(parsed.file_id, 42);
    }

    #[test]
    fn parses_proxy_local_bigbrother_strm_path() {
        let parsed = matcher()
            .parse("/d/shows/Show.S01E01.mkv?file_id=99")
            .unwrap();

        assert_eq!(parsed.file_id, 99);
    }

    #[test]
    fn rejects_non_bigbrother_url() {
        assert!(
            matcher()
                .parse("https://example.com/d/movie.mkv?file_id=42")
                .is_none()
        );
    }

    #[test]
    fn rejects_uppercase_scheme_non_bigbrother_url() {
        assert!(
            matcher()
                .parse("HTTP://evil.example/d/movie.mkv?file_id=42")
                .is_none()
        );
    }

    #[test]
    fn rejects_protocol_relative_non_bigbrother_url() {
        assert!(
            matcher()
                .parse("//evil.example/d/movie.mkv?file_id=42")
                .is_none()
        );
    }

    #[test]
    fn rejects_paths_outside_strm_prefix_boundary() {
        assert!(matcher().parse("/download/movie.mkv?file_id=42").is_none());
        assert!(matcher().parse("/d2/movie.mkv?file_id=42").is_none());
    }

    #[test]
    fn rejects_invalid_file_id() {
        assert!(matcher().parse("/d/movie.mkv?file_id=abc").is_none());
    }

    #[test]
    fn preserves_emby_token_query_case_insensitively() {
        assert_eq!(
            emby_token_query("/Videos/1/stream?DeviceId=x&api_KEY=abc"),
            Some("api_KEY=abc".to_string())
        );
        assert_eq!(
            emby_token_query("/Videos/1/stream?X-Emby-Token=def"),
            Some("X-Emby-Token=def".to_string())
        );
        assert_eq!(emby_token_query("/Videos/1/stream?DeviceId=x"), None);
    }

    #[test]
    fn media_source_ids_match_with_optional_prefix() {
        assert!(media_source_ids_match("mediasource_42", "42"));
        assert!(media_source_ids_match("42", "mediasource_42"));
        assert!(!media_source_ids_match("42", "43"));
    }
}
