use super::*;
use crate::domain::import::inner::Etag;

fn raw_file(name: &str, path: &str) -> RawFile {
    RawFile {
        id: None,
        name: name.to_string(),
        etag: Etag::Md5("etag".to_string()),
        size: 1,
        path: path.to_string(),
    }
}

#[test]
fn build_media_files_filters_unknown_files() {
    let mut lookup = MetadataLookup::default();

    let media_files = lookup.build_media_files(vec![
        raw_file("Movie.2020.1080p.mkv", ""),
        raw_file("notes.txt", ""),
    ]);

    assert_eq!(media_files.len(), 1);
    assert_eq!(media_files[0].video.name, "Movie.2020.1080p.mkv");
}

#[test]
fn build_media_files_groups_subtitles_after_metadata_lookup() {
    let mut lookup = MetadataLookup::default();

    let media_files = lookup.build_media_files(vec![
        raw_file("Show.S01E01.mkv", "/Show"),
        raw_file("Show.S01E01.zh.srt", "/Show"),
    ]);

    assert_eq!(media_files.len(), 1);
    assert_eq!(media_files[0].subtitles.len(), 1);
    assert_eq!(media_files[0].subtitles[0].name, "Show.S01E01.zh.srt");
}
