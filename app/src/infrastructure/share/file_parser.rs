use crate::{
    domain::{
        import::source::{
            parse_fslink_to_raw_files, parse_json_to_raw_files, parse_json_to_raw_files_with_context,
        },
        share::RawFile,
    },
    error::AppResult,
};

pub struct ShareFileParser;

impl ShareFileParser {
    pub fn parse_fslink(fslink: &str) -> AppResult<Vec<RawFile>> {
        parse_fslink_to_raw_files(fslink)
    }

    pub fn parse_json_bytes(content: Vec<u8>) -> AppResult<Vec<RawFile>> {
        parse_json_to_raw_files(content)
    }

    #[allow(dead_code)]
    pub fn parse_json_bytes_with_context(
        content: Vec<u8>,
        fallback_common_path: &str,
    ) -> AppResult<Vec<RawFile>> {
        parse_json_to_raw_files_with_context(content, fallback_common_path)
    }
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
