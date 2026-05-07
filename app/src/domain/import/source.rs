use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, de::Deserializer};
use tracing::info;
use url::Url;

use crate::error::{AppError, AppResult};

pub(crate) enum ShareUrl<'a> {
    Pan123(&'a Url),
    Pan189(&'a Url),
    Pan115(&'a Url),
}

impl<'a> ShareUrl<'a> {
    pub fn from(url: &'a Url) -> Option<Self> {
        if url
            .host_str()
            .is_some_and(|host| host.starts_with("www.123") && host.ends_with(".com"))
            && url.path().starts_with("/s/")
        {
            Some(Self::Pan123(url))
        } else if url.host_str().is_some_and(|host| host == "cloud.189.cn")
            && (url.path().starts_with("/t/") || url.path() == "/web/share")
        {
            Some(Self::Pan189(url))
        } else if url
            .host_str()
            .is_some_and(|host| host == "115.com" || host == "115cdn.com")
            && url.path().starts_with("/s/")
        {
            Some(Self::Pan115(url))
        } else {
            None
        }
    }

    pub fn get_url(&self) -> &Url {
        match self {
            Self::Pan123(url) | Self::Pan189(url) | Self::Pan115(url) => url,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResourceFile {
    pub(crate) path: String,
    #[serde(default, alias = "md5", alias = "sha1")]
    pub(crate) etag: String,
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    pub(crate) size: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ResourceJson {
    #[serde(rename = "commonPath")]
    pub(crate) common_path: String,
    pub(crate) files: Vec<ResourceFile>,
}

pub(crate) fn parse_pan123_share_parts(url: &Url) -> (String, String) {
    let share_key = url
        .path_segments()
        .map(|mut segments| segments.next_back().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    let share_password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_key, share_password)
}

pub(crate) fn parse_pan189_share_code(url: &Url) -> String {
    url.query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| {
            if url.path().starts_with("/t/") {
                url.path_segments()
                    .map(|mut segments| segments.next_back().unwrap_or_default())
                    .unwrap_or_default()
                    .to_owned()
            } else {
                String::new()
            }
        })
}

pub(crate) fn parse_pan115_share_parts(url: &Url) -> (String, String) {
    let share_code = url
        .path_segments()
        .map(|mut segments| segments.next_back().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    let receive_code = url
        .query_pairs()
        .find(|(key, _)| key == "password" || key == "rc")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_code, receive_code)
}

pub(crate) fn is_fslink(content: &str) -> bool {
    ["123FSLinkV2$", "123FLCPV2$"]
        .iter()
        .any(|prefix| content.starts_with(prefix))
}

pub(crate) fn parse_files_from_fslink(fslink: &str) -> AppResult<Vec<ResourceFile>> {
    let mut files = Vec::new();
    for segment in fslink.split('$') {
        let parts = segment.split('#').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(AppError::InvalidParameter(format!(
                "invalid fslink: {}",
                segment
            )));
        }

        let size = parts[1].parse::<u64>().map_err(|_| {
            AppError::InvalidParameter(format!("invalid fslink: {}, size is not u64", segment))
        })?;

        files.push(ResourceFile {
            path: parts[2].to_owned(),
            etag: parts[0].to_owned(),
            size,
        });
    }

    info!("parsed {} files from fslink", files.len());
    Ok(files)
}

pub(crate) fn parse_files_from_json(json: Vec<u8>) -> AppResult<ResourceJson> {
    let json = decode_base64_json_if_needed(json);

    if let Ok(resource) = serde_json::from_slice::<ResourceJson>(&json)
        && (!resource.files.is_empty() || !resource.common_path.is_empty())
    {
        return Ok(resource);
    }

    if let Ok(file) = serde_json::from_slice::<SingleResourceFile>(&json) {
        return Ok(ResourceJson {
            common_path: String::new(),
            files: vec![file.into_resource_file()?],
        });
    }

    info!("Failed to parse JSON as object format, trying array-of-arrays format");

    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_slice(&json)?;
    let mut files = Vec::new();
    for row in rows {
        if row.len() != 3 {
            return Err(AppError::InvalidParameter(format!(
                "invalid json row: expected 3 elements, got {}",
                row.len()
            )));
        }

        let etag = row[0]
            .as_str()
            .ok_or_else(|| AppError::InvalidParameter("etag is not a string".into()))?
            .to_owned();
        let size = row[1]
            .as_u64()
            .ok_or_else(|| AppError::InvalidParameter("size is not a u64".into()))?;
        let path = row[2]
            .as_str()
            .ok_or_else(|| AppError::InvalidParameter("path is not a string".into()))?
            .to_owned();
        files.push(ResourceFile { path, etag, size });
    }

    Ok(ResourceJson {
        common_path: String::new(),
        files,
    })
}

fn deserialize_u64_from_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U64Value {
        Number(u64),
        String(String),
    }

    match U64Value::deserialize(deserializer)? {
        U64Value::Number(value) => Ok(value),
        U64Value::String(value) => value.parse::<u64>().map_err(serde::de::Error::custom),
    }
}

fn decode_base64_json_if_needed(json: Vec<u8>) -> Vec<u8> {
    let trimmed = json
        .iter()
        .copied()
        .skip_while(|byte| byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if trimmed
        .first()
        .is_some_and(|byte| *byte == b'{' || *byte == b'[')
    {
        return json;
    }

    let Ok(text) = std::str::from_utf8(&json) else {
        return json;
    };
    match general_purpose::STANDARD.decode(text.trim()) {
        Ok(decoded) => decoded,
        Err(_) => json,
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SingleResourceFile {
    path: String,
    name: String,
    file_name: String,
    etag: String,
    md5: String,
    sha1: String,
    size: u64,
}

impl SingleResourceFile {
    fn into_resource_file(self) -> AppResult<ResourceFile> {
        let path = first_non_empty([self.path, self.name, self.file_name]);
        let etag = first_non_empty([self.etag, self.md5, self.sha1]);

        if path.is_empty() {
            return Err(AppError::InvalidParameter(
                "invalid cas/json file: path is empty".into(),
            ));
        }
        if etag.is_empty() {
            return Err(AppError::InvalidParameter(
                "invalid cas/json file: etag is empty".into(),
            ));
        }
        if self.size == 0 {
            return Err(AppError::InvalidParameter(
                "invalid cas/json file: size is empty".into(),
            ));
        }

        Ok(ResourceFile {
            path,
            etag,
            size: self.size,
        })
    }
}

fn first_non_empty<const N: usize>(values: [String; N]) -> String {
    values
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_url_recognizes_pan115_links() {
        for url in [
            "https://115cdn.com/s/swfoexi3no3?password=j7b2",
            "https://115.com/s/swfoexi3no3?rc=j7b2",
            "https://115.com/s/swfoexi3no3",
        ] {
            let parsed_url = Url::parse(url).unwrap();
            let parsed = ShareUrl::from(&parsed_url);
            assert!(parsed.is_some());
            assert!(matches!(parsed.unwrap(), ShareUrl::Pan115(_)));
        }
    }

    #[test]
    fn share_url_recognizes_supported_hosts() {
        let pan123 = Url::parse("https://www.123pan.com/s/abc123?pwd=test").unwrap();
        let pan189 = Url::parse("https://cloud.189.cn/t/abc123").unwrap();

        assert!(matches!(ShareUrl::from(&pan123), Some(ShareUrl::Pan123(_))));
        assert!(matches!(ShareUrl::from(&pan189), Some(ShareUrl::Pan189(_))));
    }

    #[test]
    fn share_url_rejects_unknown_host() {
        let url = Url::parse("https://example.com/s/abc123").unwrap();
        assert!(ShareUrl::from(&url).is_none());
    }

    #[test]
    fn parses_share_specific_parts() {
        let pan123 = Url::parse("https://www.123pan.com/s/share123?pwd=pass456").unwrap();
        let pan189_query = Url::parse("https://cloud.189.cn/web/share?code=abc123").unwrap();
        let pan189_path = Url::parse("https://cloud.189.cn/t/pathcode").unwrap();
        let pan115 = Url::parse("https://115.com/s/share115?password=recv").unwrap();
        let pan115_rc = Url::parse("https://115.com/s/share115?rc=recv").unwrap();

        assert_eq!(
            parse_pan123_share_parts(&pan123),
            ("share123".into(), "pass456".into())
        );
        assert_eq!(parse_pan189_share_code(&pan189_query), "abc123");
        assert_eq!(parse_pan189_share_code(&pan189_path), "pathcode");
        assert_eq!(
            parse_pan115_share_parts(&pan115),
            ("share115".into(), "recv".into())
        );
        assert_eq!(
            parse_pan115_share_parts(&pan115_rc),
            ("share115".into(), "recv".into())
        );
    }

    #[test]
    fn detects_fslink_prefixes() {
        assert!(is_fslink("123FSLinkV2$some_content"));
        assert!(is_fslink("123FLCPV2$some_content"));
        assert!(!is_fslink("invalid_link"));
        assert!(!is_fslink(""));
        assert!(!is_fslink("123FSLink$"));
    }

    #[test]
    fn parses_fslink_entries() {
        let fslink = "hash1#1024#file1.mp4$hash2#2048#file2.mkv$hash3#4096#file3.srt";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].etag, "hash1");
        assert_eq!(result[1].size, 2048);
        assert_eq!(result[2].path, "file3.srt");
    }

    #[test]
    fn rejects_invalid_fslink_segments() {
        for fslink in [
            "hash1#1024",
            "hash1#1024#file.mp4#extra",
            "hash1#notanumber#file.mp4",
            "hash1#-1#file.mp4",
            "hash1#1024.5#file.mp4",
            "hash1#1024#file1.mp4$invalid",
            "##file.mp4",
            "etag123#1024#file#with#hash.mp4",
        ] {
            assert!(parse_files_from_fslink(fslink).is_err());
        }
    }

    #[test]
    fn parses_json_resource_formats() {
        let object_json = r#"
        {
            "commonPath": "/media",
            "files": [
                { "path": "movie.mp4", "etag": "etag123", "size": 1024 },
                { "path": "subtitle.srt", "etag": "etag456", "size": 2048 }
            ]
        }
        "#;
        let array_json = r#"
        [
            ["ffd9a9bc7616540fcd741afee223a12b", 1766233377, "3 h.264.mkv"]
        ]
        "#;

        let object_resource = parse_files_from_json(object_json.as_bytes().to_vec()).unwrap();
        let array_resource = parse_files_from_json(array_json.as_bytes().to_vec()).unwrap();

        assert_eq!(object_resource.common_path, "/media");
        assert_eq!(object_resource.files.len(), 2);
        assert_eq!(array_resource.common_path, "");
        assert_eq!(array_resource.files[0].path, "3 h.264.mkv");
    }

    #[test]
    fn parses_json_resource_with_string_sizes() {
        let object_json = r#"
        {
            "commonPath": "",
            "files": [
                {
                    "path": "Pegasus.3.2026.2160p.WEB-DL.60fps.HQ.DV.H.265.DTS5.1-HiveWeb.mkv",
                    "sha1": "A444F38B2CECB8A71AD27A0DD88BED1CF1FA1EB6",
                    "size": "30062779674"
                }
            ]
        }
        "#;

        let resource = parse_files_from_json(object_json.as_bytes().to_vec()).unwrap();

        assert_eq!(resource.common_path, "");
        assert_eq!(resource.files.len(), 1);
        assert_eq!(
            resource.files[0].path,
            "Pegasus.3.2026.2160p.WEB-DL.60fps.HQ.DV.H.265.DTS5.1-HiveWeb.mkv"
        );
        assert_eq!(
            resource.files[0].etag,
            "A444F38B2CECB8A71AD27A0DD88BED1CF1FA1EB6"
        );
        assert_eq!(resource.files[0].size, 30062779674);
    }

    #[test]
    fn parses_single_file_cas_resource() {
        let cas = serde_json::json!({
            "fileName": "Movie.2026.2160p.WEB-DL.mkv",
            "md5": "ffd9a9bc7616540fcd741afee223a12b",
            "size": 1766233377
        });

        let resource = parse_files_from_json(cas.to_string().into_bytes()).unwrap();

        assert_eq!(resource.common_path, "");
        assert_eq!(resource.files.len(), 1);
        assert_eq!(resource.files[0].path, "Movie.2026.2160p.WEB-DL.mkv");
        assert_eq!(resource.files[0].etag, "ffd9a9bc7616540fcd741afee223a12b");
        assert_eq!(resource.files[0].size, 1766233377);
    }

    #[test]
    fn parses_base64_encoded_single_file_cas_resource() {
        let cas = general_purpose::STANDARD.encode(
            serde_json::json!({
                "name": "Movie.2026.2160p.WEB-DL.mkv",
                "md5": "ffd9a9bc7616540fcd741afee223a12b",
                "size": 1766233377
            })
            .to_string(),
        );

        let resource = parse_files_from_json(cas.into_bytes()).unwrap();

        assert_eq!(resource.files.len(), 1);
        assert_eq!(resource.files[0].path, "Movie.2026.2160p.WEB-DL.mkv");
        assert_eq!(resource.files[0].etag, "ffd9a9bc7616540fcd741afee223a12b");
        assert_eq!(resource.files[0].size, 1766233377);
    }

    #[test]
    fn rejects_single_file_cas_without_identity_fields() {
        let cas = serde_json::json!({
            "fileName": "Movie.2026.2160p.WEB-DL.mkv",
            "size": 1766233377
        });

        let error = parse_files_from_json(cas.to_string().into_bytes()).unwrap_err();

        assert!(error.to_string().contains("etag is empty"));
    }
}
