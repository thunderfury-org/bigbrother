use crate::application::import::metadata::MetadataLookup;
use crate::domain::import::inner::{Etag, RawFile};

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

#[test]
fn parse_media_metadata_keeps_movie_audio_channels_out_of_tv_path_merge() {
    let mut lookup = MetadataLookup::default();

    let metadata = lookup.parse_media_metadata(
        "The.Hobbit.An.Unexpected.Journey.2012.EXTENDED.2160p.BluRay.REMUX.HDR.DV.HEVC.DTS-HD.MA.TrueHD.7.1.Atmos.mkv",
        "/Library/Season 1",
    );

    assert!(!metadata.is_tv_episode());
    assert_eq!(metadata.season_number, None);
    assert_eq!(metadata.episode_number, None);
}

#[test]
fn parse_media_metadata_uses_tv_path_merge_only_for_explicit_tv_episode() {
    let mut lookup = MetadataLookup::default();

    let metadata = lookup.parse_media_metadata("Show.S01E01.1080p.WEB-DL.mkv", "/Library/Show");

    assert!(metadata.is_tv_episode());
    assert_eq!(metadata.season_number, Some(1));
    assert_eq!(metadata.episode_number, Some(1));
}
