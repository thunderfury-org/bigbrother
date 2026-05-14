use std::path::Path;

use crate::{
    domain::{
        import::source::{ResourceJson, parse_files_from_fslink, parse_files_from_json},
        share::RawFile,
    },
    error::AppResult,
};

pub struct ShareFileParser;

impl ShareFileParser {
    pub fn parse_fslink(fslink: &str) -> AppResult<Vec<RawFile>> {
        let resource = parse_fslink_resource(fslink)?;
        Ok(raw_files_from_resource(&resource))
    }

    pub fn parse_json_bytes(content: Vec<u8>) -> AppResult<Vec<RawFile>> {
        let resource = parse_files_from_json(content)?;
        Ok(raw_files_from_resource(&resource))
    }
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

fn raw_files_from_resource(resource: &ResourceJson) -> Vec<RawFile> {
    resource
        .files
        .iter()
        .map(|file| {
            let full_path = format!("{}/{}", resource.common_path, file.path);
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

#[cfg(test)]
mod tests {
    use super::ShareFileParser;
    use crate::{domain::share::Etag, error::AppError};
    use base64::{Engine, engine::general_purpose};

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
}
