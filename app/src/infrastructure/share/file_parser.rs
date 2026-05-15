use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, de::Deserializer};
use std::path::Path;
use tracing::info;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
};

pub struct ShareFileParser;

impl ShareFileParser {
    pub fn is_fslink(content: &str) -> bool {
        ["123FSLinkV2$", "123FLCPV2$"]
            .iter()
            .any(|prefix| content.starts_with(prefix))
    }

    pub fn parse_fslink(fslink: &str) -> AppResult<Vec<RawFile>> {
        let resource = parse_fslink_resource(fslink)?;
        Ok(raw_files_from_resource_with_context(&resource, ""))
    }

    pub fn parse_json_bytes(content: Vec<u8>) -> AppResult<Vec<RawFile>> {
        Self::parse_json_bytes_with_context(content, "")
    }

    pub fn parse_json_bytes_with_context(
        content: Vec<u8>,
        fallback_common_path: &str,
    ) -> AppResult<Vec<RawFile>> {
        let resource = parse_files_from_json(content)?;
        Ok(raw_files_from_resource_with_context(
            &resource,
            fallback_common_path,
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ResourceFile {
    path: String,
    #[serde(default, alias = "md5", alias = "sha1")]
    etag: String,
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    size: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResourceJson {
    #[serde(rename = "commonPath")]
    common_path: String,
    files: Vec<ResourceFile>,
}

fn parse_files_from_fslink(fslink: &str) -> AppResult<Vec<ResourceFile>> {
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

fn parse_files_from_json(json: Vec<u8>) -> AppResult<ResourceJson> {
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

fn parse_fslink_resource(fslink: &str) -> AppResult<ResourceJson> {
    let mut resource = ResourceJson::default();

    let mut fslink = fslink.find('$').map(|i| &fslink[i + 1..]).unwrap_or(fslink);
    if let Some(i) = fslink.find('%') {
        resource.common_path = fslink[..i].to_owned();
        fslink = &fslink[i + 1..];
    }
    resource.files = parse_files_from_fslink(fslink)?;
    Ok(resource)
}

fn raw_files_from_resource_with_context(
    resource: &ResourceJson,
    fallback_common_path: &str,
) -> Vec<RawFile> {
    let common_path = if resource.common_path.trim().is_empty() {
        fallback_common_path
    } else {
        resource.common_path.as_str()
    };

    resource
        .files
        .iter()
        .map(|file| {
            let full_path = format!("{common_path}/{}", file.path);
            let path = Path::new(full_path.as_str());
            let parent_path = path
                .parent()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();
            let name = path
                .file_name()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();

            RawFile {
                id: None,
                name: name.to_owned(),
                etag: file.etag.as_str().into(),
                size: file.size,
                path: parent_path.to_owned(),
            }
        })
        .collect()
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
    use super::ShareFileParser;
    use crate::{domain::share::Etag, error::AppError};
    use base64::{Engine, engine::general_purpose};

    #[test]
    fn detects_fslink_prefixes() {
        assert!(ShareFileParser::is_fslink("123FSLinkV2$some_content"));
        assert!(ShareFileParser::is_fslink("123FLCPV2$some_content"));
        assert!(!ShareFileParser::is_fslink("invalid_link"));
    }

    #[test]
    fn parses_fslink_with_common_path_into_raw_files() {
        let raw_files = ShareFileParser::parse_fslink(
            "123FSLinkV2$/Media/Movies%MovieHash#1024#Movie.2026.mkv$SubHash#12#Movie.2026.srt",
        )
        .unwrap();

        assert_eq!(raw_files.len(), 2);
        assert_eq!(raw_files[0].name, "Movie.2026.mkv");
        assert_eq!(raw_files[0].path, "/Media/Movies");
        assert!(matches!(raw_files[0].etag, Etag::Md5(ref value) if value == "moviehash"));
        assert_eq!(raw_files[1].name, "Movie.2026.srt");
        assert_eq!(raw_files[1].path, "/Media/Movies");
    }

    #[test]
    fn parses_object_json_with_string_size_into_raw_files() {
        let json = br#"{
            "commonPath": "/shows",
            "files": [
                {
                    "path": "Series/Season 1/E01.mkv",
                    "sha1": "A444F38B2CECB8A71AD27A0DD88BED1CF1FA1EB6",
                    "size": "30062779674"
                }
            ]
        }"#;

        let raw_files = ShareFileParser::parse_json_bytes(json.to_vec()).unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].name, "E01.mkv");
        assert_eq!(raw_files[0].path, "/shows/Series/Season 1");
        assert!(
            matches!(raw_files[0].etag, Etag::Sha1(ref value) if value == "a444f38b2cecb8a71ad27a0dd88bed1cf1fa1eb6")
        );
        assert_eq!(raw_files[0].size, 30062779674);
    }

    #[test]
    fn parses_base64_single_file_cas_into_raw_files() {
        let cas = general_purpose::STANDARD.encode(
            serde_json::json!({
                "fileName": "Movie.2026.2160p.WEB-DL.mkv",
                "md5": "ffd9a9bc7616540fcd741afee223a12b",
                "size": 1766233377
            })
            .to_string(),
        );

        let raw_files = ShareFileParser::parse_json_bytes(cas.into_bytes()).unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].name, "Movie.2026.2160p.WEB-DL.mkv");
        assert_eq!(raw_files[0].path, "/");
        assert!(
            matches!(raw_files[0].etag, Etag::Md5(ref value) if value == "ffd9a9bc7616540fcd741afee223a12b")
        );
        assert_eq!(raw_files[0].size, 1766233377);
    }

    #[test]
    fn rejects_single_file_cas_without_identity_fields() {
        let cas = serde_json::json!({
            "fileName": "Movie.2026.2160p.WEB-DL.mkv",
            "size": 1766233377
        });

        let err = ShareFileParser::parse_json_bytes(cas.to_string().into_bytes()).unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("etag is empty"));
    }

    #[test]
    fn uses_fallback_common_path_for_single_file_cas() {
        let cas = serde_json::json!({
            "fileName": "S01E01.mp4",
            "md5": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "size": 1001
        });

        let raw_files =
            ShareFileParser::parse_json_bytes_with_context(cas.to_string().into_bytes(), "Show")
                .unwrap();

        assert_eq!(raw_files.len(), 1);
        assert_eq!(raw_files[0].name, "S01E01.mp4");
        assert_eq!(raw_files[0].path, "Show");
    }
}
