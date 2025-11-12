use std::path::Path;

use serde::Deserialize;
use tracing::info;

use super::{
    ImportSummary, Importer,
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

impl Importer {
    pub async fn import_from_fslink(&mut self, fslink: &str) -> AppResult<ImportSummary> {
        info!("import from fslink");

        let mut resource = ResourceJson::default();

        let mut fslink = fslink.find("$").map(|i| &fslink[i + 1..]).unwrap_or(fslink);
        if let Some(i) = fslink.find("%") {
            resource.common_path = fslink[..i].to_owned();
            fslink = &fslink[i + 1..];
        }
        resource.files = self.parse_files_from_fslink(fslink)?;

        self.import_from_resource_json(&resource).await
    }

    fn parse_files_from_fslink(&mut self, fslink: &str) -> AppResult<Vec<ResourceFile>> {
        let split = fslink.split("$");
        let mut files = Vec::new();
        for s in split {
            let parts = s.split("#").collect::<Vec<_>>();
            if parts.len() != 3 {
                return Err(AppError::Error(format!("invalid fslink: {}", s)));
            }

            let size = match parts[1].parse::<u64>() {
                Ok(size) => size,
                Err(_) => {
                    return Err(AppError::Error(format!("invalid fslink: {}, size is not u64", s)));
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

    pub async fn import_from_json(&mut self, json: Vec<u8>) -> AppResult<ImportSummary> {
        let resource: ResourceJson = serde_json::from_slice(&json)?;
        self.import_from_resource_json(&resource).await
    }

    async fn import_from_resource_json(&mut self, resource: &ResourceJson) -> AppResult<ImportSummary> {
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

            self.summary.total += 1;
            let metadata = self.parse_media_metadata(name, parent_path);
            if metadata.unknown_type() {
                self.summary.skipped += 1;
                continue;
            }

            all_files.push((
                metadata,
                RawFile {
                    id: None,
                    name: name.to_owned(),
                    etag: file.etag.to_owned(),
                    size: file.size,
                    path: parent_path.to_owned(),
                },
            ));
        }

        self.convert_share_raw_file_to_media_file(all_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS_LINK: &str = "123FSLinkV2$0645d6c4f5494410cb115d84246f27d2#1035390787#Test.2020.S01E197.2160p.WEB-DL.H265.AAC 2.0 {tmdb-101172}.mkv";

    #[test]
    fn test_is_fslink() {
        assert!(is_fslink(FS_LINK));
    }

    #[test]
    fn test_parse_files_from_fslink() {
        let mut importer = Importer::default();
        let files = importer
            .parse_files_from_fslink(FS_LINK.trim_start_matches("123FSLinkV2$"))
            .unwrap();
        let media_files = importer.list_files_from_json(&ResourceJson {
            common_path: "".to_owned(),
            files,
        });
        assert_eq!(media_files.len(), 1);
    }
}
