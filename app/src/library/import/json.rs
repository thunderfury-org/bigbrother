use std::path::Path;

use serde::Deserialize;
use tracing::info;

use super::{
    ImportedMedia, Importer, LibraryGateway, ShareSource,
    group::group_video_and_subtitle_files,
    inner::{MediaFile, RawFile},
};
use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
struct ResourceFile {
    pub path: String,
    pub etag: String,
    pub size: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResourceJson {
    #[serde(rename = "commonPath")]
    pub common_path: String,
    pub files: Vec<ResourceFile>,
}

pub fn is_fslink(content: &str) -> bool {
    let prefix = ["123FSLinkV2$", "123FLCPV2$"];
    prefix.iter().any(|p| content.starts_with(p))
}

fn parse_files_from_fslink(fslink: &str) -> AppResult<Vec<ResourceFile>> {
    let split = fslink.split("$");
    let mut files = Vec::new();
    for s in split {
        let parts = s.split("#").collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(AppError::InvalidParameter(format!("invalid fslink: {}", s)));
        }

        let size = match parts[1].parse::<u64>() {
            Ok(size) => size,
            Err(_) => {
                return Err(AppError::InvalidParameter(format!(
                    "invalid fslink: {}, size is not u64",
                    s
                )));
            }
        };

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
    // Try the object format first: { "commonPath": "...", "files": [...] }
    if let Ok(resource) = serde_json::from_slice::<ResourceJson>(&json) {
        return Ok(resource);
    }

    info!("Failed to parse JSON as object format, trying array-of-arrays format");

    // Fallback: array-of-arrays format [[etag, size, path], ...]
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

impl<L, S, M> Importer<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: super::MetadataCatalog,
{
    pub async fn import_from_fslink(&mut self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from fslink");

        let mut resource = ResourceJson::default();

        let mut fslink = fslink.find("$").map(|i| &fslink[i + 1..]).unwrap_or(fslink);
        if let Some(i) = fslink.find("%") {
            resource.common_path = fslink[..i].to_owned();
            fslink = &fslink[i + 1..];
        }
        resource.files = parse_files_from_fslink(fslink)?;

        self.import_from_resource_json(&resource).await
    }

    pub async fn import_from_json(&mut self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from JSON");
        let resource: ResourceJson = parse_files_from_json(json)?;
        self.import_from_resource_json(&resource).await
    }

    async fn import_from_resource_json(
        &mut self,
        resource: &ResourceJson,
    ) -> AppResult<Vec<ImportedMedia>> {
        let media_files = self.list_files_from_json(resource);
        self.transfer_media_files(&media_files).await
    }

    fn list_files_from_json(&mut self, resource: &ResourceJson) -> Vec<MediaFile> {
        let mut all_files = Vec::new();

        for file in &resource.files {
            let _path = format!("{}/{}", &resource.common_path, &file.path);
            let path = Path::new(_path.as_str());
            let parent_path = path
                .parent()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();
            let name = path
                .file_name()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();

            let metadata = self.parse_media_metadata(name, parent_path);
            if metadata.unknown_type() {
                continue;
            }

            all_files.push((
                metadata,
                RawFile {
                    id: None,
                    name: name.to_owned(),
                    etag: file.etag.as_str().into(),
                    size: file.size,
                    path: parent_path.to_owned(),
                },
            ));
        }

        group_video_and_subtitle_files(all_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fslink_v2() {
        assert!(is_fslink("123FSLinkV2$some_content"));
    }

    #[test]
    fn test_is_fslink_flcp() {
        assert!(is_fslink("123FLCPV2$some_content"));
    }

    #[test]
    fn test_is_fslink_invalid() {
        assert!(!is_fslink("invalid_link"));
        assert!(!is_fslink(""));
        assert!(!is_fslink("123FSLink$"));
    }

    #[test]
    fn test_parse_single_file() {
        let fslink = "0645d6c4f5494410cb115d84246f27d2#1035390787#Test.2020.S01E197.mkv";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].etag, "0645d6c4f5494410cb115d84246f27d2");
        assert_eq!(result[0].size, 1035390787);
        assert_eq!(result[0].path, "Test.2020.S01E197.mkv");
    }

    #[test]
    fn test_parse_multiple_files() {
        let fslink = "hash1#1024#file1.mp4$hash2#2048#file2.mkv$hash3#4096#file3.srt";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 3);

        assert_eq!(result[0].etag, "hash1");
        assert_eq!(result[0].size, 1024);
        assert_eq!(result[0].path, "file1.mp4");

        assert_eq!(result[1].etag, "hash2");
        assert_eq!(result[1].size, 2048);
        assert_eq!(result[1].path, "file2.mkv");

        assert_eq!(result[2].etag, "hash3");
        assert_eq!(result[2].size, 4096);
        assert_eq!(result[2].path, "file3.srt");
    }

    #[test]
    fn test_parse_file_with_special_characters() {
        let fslink = "abc123#999#Test.2020.S01E197.2160p.WEB-DL.H265.AAC 2.0 {tmdb-101172}.mkv";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].path,
            "Test.2020.S01E197.2160p.WEB-DL.H265.AAC 2.0 {tmdb-101172}.mkv"
        );
    }

    #[test]
    fn test_parse_file_with_path() {
        let fslink = "etag123#5000#folder/subfolder/movie.mp4";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "folder/subfolder/movie.mp4");
    }

    #[test]
    fn test_parse_invalid_format_too_few_parts() {
        let fslink = "hash1#1024";
        let result = parse_files_from_fslink(fslink);

        assert!(result.is_err());
        if let Err(AppError::InvalidParameter(msg)) = result {
            assert!(msg.contains("invalid fslink"));
        } else {
            panic!("Expected InvalidParameter error");
        }
    }

    #[test]
    fn test_parse_invalid_format_too_many_parts() {
        let fslink = "hash1#1024#file.mp4#extra";
        let result = parse_files_from_fslink(fslink);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_size_not_number() {
        let fslink = "hash1#notanumber#file.mp4";
        let result = parse_files_from_fslink(fslink);

        assert!(result.is_err());
        if let Err(AppError::InvalidParameter(msg)) = result {
            assert!(msg.contains("size is not u64"));
        } else {
            panic!("Expected InvalidParameter error with size message");
        }
    }

    #[test]
    fn test_parse_invalid_size_negative() {
        let fslink = "hash1#-1#file.mp4";
        let result = parse_files_from_fslink(fslink);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_size_float() {
        let fslink = "hash1#1024.5#file.mp4";
        let result = parse_files_from_fslink(fslink);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_zero_size() {
        let fslink = "hash1#0#file.mp4";
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].size, 0);
    }

    #[test]
    fn test_parse_large_size() {
        let fslink = "hash1#18446744073709551615#file.mp4"; // u64::MAX
        let result = parse_files_from_fslink(fslink).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].size, u64::MAX);
    }

    #[test]
    fn test_parse_mixed_valid_invalid() {
        // First file is valid, second is invalid
        let fslink = "hash1#1024#file1.mp4$invalid";
        let result = parse_files_from_fslink(fslink);

        // Should fail on the invalid part
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_fields() {
        let fslink = "##file.mp4";
        let result = parse_files_from_fslink(fslink);

        // Empty size should fail to parse
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_hash_in_filename() {
        // Filename contains # character - will be split incorrectly
        let fslink = "etag123#1024#file#with#hash.mp4";
        let result = parse_files_from_fslink(fslink);

        // Should fail because it splits into more than 3 parts
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_from_json_1() {
        let json = r#"
        {
            "commonPath": "/media",
            "files": [
                {
                    "path": "movie.mp4",
                    "etag": "etag123",
                    "size": 1024
                },
                {
                    "path": "subtitle.srt",
                    "etag": "etag456",
                    "size": 2048
                }
            ]
        }
        "#;

        let resource = parse_files_from_json(json.as_bytes().to_vec()).unwrap();
        assert_eq!(resource.common_path, "/media");
        assert_eq!(resource.files.len(), 2);
        assert_eq!(resource.files[0].path, "movie.mp4");
        assert_eq!(resource.files[0].etag, "etag123");
        assert_eq!(resource.files[0].size, 1024);
        assert_eq!(resource.files[1].path, "subtitle.srt");
        assert_eq!(resource.files[1].etag, "etag456");
        assert_eq!(resource.files[1].size, 2048);
    }

    #[test]
    fn test_parse_from_json_2() {
        let json = r#"
        [
            [
                "ffd9a9bc7616540fcd741afee223a12b",
                1766233377,
                "3 h.264.mkv"
            ]
        ]
        "#;

        let resource = parse_files_from_json(json.as_bytes().to_vec()).unwrap();
        assert_eq!(resource.common_path, "");
        assert_eq!(resource.files.len(), 1);
        assert_eq!(resource.files[0].path, "3 h.264.mkv");
        assert_eq!(resource.files[0].etag, "ffd9a9bc7616540fcd741afee223a12b");
        assert_eq!(resource.files[0].size, 1766233377);
    }
}
