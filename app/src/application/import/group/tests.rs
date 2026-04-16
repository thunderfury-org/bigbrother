use super::*;
use crate::{
    application::import::{Genre, Season, TvDetail},
    domain::import::inner::{Etag, RawFile},
    domain::import::policy::group_video_and_subtitle_files,
    domain::media::{FileType, Metadata},
};

fn create_raw_file(name: &str) -> RawFile {
    RawFile {
        id: None,
        name: name.to_string(),
        etag: Etag::Md5("test".to_string()),
        size: 1024,
        path: format!("/test/{}", name),
    }
}

fn create_video_metadata() -> Box<Metadata> {
    Box::new(Metadata {
        file_type: FileType::Video,
        ..Default::default()
    })
}

fn create_subtitle_metadata() -> Box<Metadata> {
    Box::new(Metadata {
        file_type: FileType::Subtitle,
        ..Default::default()
    })
}

fn create_tv_detail(number_of_seasons: u32) -> TvDetail {
    TvDetail {
        id: 1,
        name: "Test Show".to_string(),
        first_air_date: "2024-01-01".to_string(),
        number_of_episodes: 10,
        number_of_seasons,
        origin_country: vec![],
        original_language: "en".to_string(),
        original_name: "Test Show".to_string(),
        genres: vec![Genre {
            id: 1,
            name: "Drama".to_string(),
        }],
        seasons: vec![Season {
            id: 1,
            name: "Season 1".to_string(),
            episode_count: 10,
            season_number: 1,
        }],
    }
}

fn create_tv_media_file(
    file_name: &str,
    season_number: Option<u32>,
    episode_number: Option<u32>,
) -> MediaFile {
    MediaFile {
        metadata: Box::new(Metadata {
            file_type: FileType::Video,
            season_number,
            episode_number,
            extension: ".mkv".to_string(),
            ..Default::default()
        }),
        video: create_raw_file(file_name),
        subtitles: Vec::new(),
    }
}

#[test]
fn test_group_empty_files() {
    let raw_files = vec![];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_group_no_video_files() {
    let raw_files = vec![
        (create_subtitle_metadata(), create_raw_file("sub1.srt")),
        (create_subtitle_metadata(), create_raw_file("sub2.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_group_only_video_files_no_subtitles() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("video1.mp4")),
        (create_video_metadata(), create_raw_file("video2.mkv")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].subtitles.len(), 0);
    assert_eq!(result[1].subtitles.len(), 0);
}

#[test]
fn test_group_single_video_with_matching_subtitle() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("movie.mp4")),
        (create_subtitle_metadata(), create_raw_file("movie.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].video.name, "movie.mp4");
    assert_eq!(result[0].subtitles.len(), 1);
    assert_eq!(result[0].subtitles[0].name, "movie.srt");
}

#[test]
fn test_group_video_with_multiple_subtitles() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("movie.mp4")),
        (create_subtitle_metadata(), create_raw_file("movie.en.srt")),
        (create_subtitle_metadata(), create_raw_file("movie.zh.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].subtitles.len(), 2);
}

#[test]
fn test_group_multiple_videos_with_matching_subtitles() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("video1.mp4")),
        (create_video_metadata(), create_raw_file("video2.mp4")),
        (create_subtitle_metadata(), create_raw_file("video1.srt")),
        (create_subtitle_metadata(), create_raw_file("video2.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 2);

    let video1_file = result
        .iter()
        .find(|f| f.video.name == "video1.mp4")
        .unwrap();
    let video2_file = result
        .iter()
        .find(|f| f.video.name == "video2.mp4")
        .unwrap();

    assert_eq!(video1_file.subtitles.len(), 1);
    assert_eq!(video1_file.subtitles[0].name, "video1.srt");
    assert_eq!(video2_file.subtitles.len(), 1);
    assert_eq!(video2_file.subtitles[0].name, "video2.srt");
}

#[test]
fn test_group_video_without_matching_subtitle() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("video1.mp4")),
        (create_subtitle_metadata(), create_raw_file("different.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].subtitles.len(), 0);
}

#[test]
fn test_group_subtitle_matches_by_prefix() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("movie.mp4")),
        (
            create_subtitle_metadata(),
            create_raw_file("movie.en.forced.srt"),
        ),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].subtitles.len(), 1);
}

#[test]
fn test_group_video_without_extension() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("videofile")),
        (create_subtitle_metadata(), create_raw_file("videofile.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].video.name, "videofile");
    assert_eq!(result[0].subtitles.len(), 1);
}

#[test]
fn test_group_complex_scenario() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("movie1.mp4")),
        (create_video_metadata(), create_raw_file("movie2.mkv")),
        (create_video_metadata(), create_raw_file("movie3.avi")),
        (create_subtitle_metadata(), create_raw_file("movie1.en.srt")),
        (create_subtitle_metadata(), create_raw_file("movie1.zh.srt")),
        (create_subtitle_metadata(), create_raw_file("movie2.srt")),
        (create_subtitle_metadata(), create_raw_file("unmatched.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 3);

    let movie1 = result
        .iter()
        .find(|f| f.video.name == "movie1.mp4")
        .unwrap();
    let movie2 = result
        .iter()
        .find(|f| f.video.name == "movie2.mkv")
        .unwrap();
    let movie3 = result
        .iter()
        .find(|f| f.video.name == "movie3.avi")
        .unwrap();

    assert_eq!(movie1.subtitles.len(), 2);
    assert_eq!(movie2.subtitles.len(), 1);
    assert_eq!(movie3.subtitles.len(), 0);
}

#[test]
fn test_group_subtitle_matches_only_once() {
    let raw_files = vec![
        (create_video_metadata(), create_raw_file("movie.mp4")),
        (create_subtitle_metadata(), create_raw_file("movie.srt")),
    ];
    let result = group_video_and_subtitle_files(raw_files);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].subtitles.len(), 1);
}

#[test]
fn test_resolve_tv_episode_slot_defaults_single_season_to_one() {
    let file = create_tv_media_file("show.s01e01.mkv", None, Some(1));
    let tv = create_tv_detail(1);

    let slot = resolve_tv_episode_slot(&file, &tv);

    assert_eq!(slot, Some((1, 1)));
}

#[test]
fn test_resolve_tv_episode_slot_rejects_missing_season_for_multi_season_show() {
    let file = create_tv_media_file("show.e01.mkv", None, Some(1));
    let tv = create_tv_detail(3);

    let slot = resolve_tv_episode_slot(&file, &tv);

    assert_eq!(slot, None);
}

#[test]
fn test_resolve_tv_episode_slot_rejects_missing_episode() {
    let file = create_tv_media_file("show.s01.mkv", Some(1), None);
    let tv = create_tv_detail(1);

    let slot = resolve_tv_episode_slot(&file, &tv);

    assert_eq!(slot, None);
}
