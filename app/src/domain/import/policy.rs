use std::collections::{BTreeMap, HashMap};

use tracing::info;

use super::{
    MovieDetail, TvDetail,
    inner::{Media, MediaFile},
};
use crate::domain::media::Metadata;
use crate::domain::share::RawFile;
use crate::error::{AppError, AppResult};

pub(crate) fn should_skip_existing_media(
    existing_files: &[MediaFile],
    media_file: &MediaFile,
) -> bool {
    !existing_files.is_empty() && !need_overwrite_existing_files(existing_files, media_file)
}

pub(crate) fn group_video_and_subtitle_files(
    raw_files: Vec<(Box<Metadata>, RawFile)>,
) -> Vec<MediaFile> {
    if raw_files.is_empty() {
        return Vec::new();
    }

    let (video_files, subtitle_files): (Vec<_>, Vec<_>) = raw_files
        .into_iter()
        .partition(|(metadata, _)| metadata.is_video());

    if video_files.is_empty() {
        return Vec::new();
    }

    if subtitle_files.is_empty() {
        return video_files
            .into_iter()
            .map(|(metadata, raw_file)| MediaFile {
                metadata,
                video: raw_file,
                subtitles: Vec::new(),
            })
            .collect();
    }

    let mut media_files_map: HashMap<String, MediaFile> = video_files
        .into_iter()
        .map(|(metadata, raw_file)| {
            let file_stem = match raw_file.name.rfind('.') {
                Some(i) => raw_file.name[..i].to_owned(),
                None => raw_file.name.to_owned(),
            };
            (
                file_stem,
                MediaFile {
                    metadata,
                    video: raw_file,
                    subtitles: Vec::new(),
                },
            )
        })
        .collect();

    for (_, subtitle_file) in subtitle_files {
        for (file_stem, media_file) in &mut media_files_map {
            if subtitle_file.name.starts_with(file_stem) {
                media_file.subtitles.push(subtitle_file);
                break;
            }
        }
    }

    media_files_map.into_values().collect()
}

pub(crate) fn resolve_tv_episode_slot(file: &MediaFile, tv_info: &TvDetail) -> Option<(u32, u32)> {
    let season_number = match file.metadata.season_number {
        Some(season_number) => season_number,
        None => {
            if tv_info.number_of_seasons == 1 {
                1
            } else {
                info!(
                    "Multi season tv, but no season number found in file: {}",
                    file.video.name
                );
                return None;
            }
        }
    };

    let episode_number = match file.metadata.episode_number {
        Some(episode_number) => episode_number,
        None => {
            info!("No episode number found in file: {}", file.video.name);
            return None;
        }
    };

    Some((season_number, episode_number))
}

pub(crate) fn insert_tv_media<'a>(
    grouped_files: &mut HashMap<u32, Media<'a>>,
    tv_info: TvDetail,
    season_number: u32,
    episode_number: u32,
    file: &'a MediaFile,
) {
    let entry = grouped_files
        .entry(tv_info.id)
        .or_insert_with(|| Media::Tv {
            detail: tv_info,
            files: BTreeMap::new(),
        });
    if let Media::Tv { files, .. } = entry {
        files
            .entry(season_number)
            .or_insert_with(BTreeMap::new)
            .entry(episode_number)
            .or_insert_with(Vec::new)
            .push(file);
    }
}

pub(crate) fn insert_movie_media<'a>(
    grouped_files: &mut HashMap<u32, Media<'a>>,
    movie_info: MovieDetail,
    file: &'a MediaFile,
) {
    let entry = grouped_files
        .entry(movie_info.id)
        .or_insert_with(|| Media::Movie {
            detail: movie_info,
            files: Vec::new(),
        });
    if let Media::Movie { files, .. } = entry {
        files.push(file);
    }
}

pub(crate) fn format_video_file_name(name_prefix: &str, file: &MediaFile) -> String {
    if file.video.name.starts_with(name_prefix) {
        return file.video.name.to_owned();
    }

    let mut parts = vec![];
    if !file.metadata.resolution.is_empty() {
        parts.push(file.metadata.resolution.as_str());
    }
    if !file.metadata.frame_rate.is_empty() {
        parts.push(file.metadata.frame_rate.as_str());
    }
    if !file.metadata.quality.is_empty() {
        parts.push(file.metadata.quality.as_str());
    }
    if !file.metadata.hdr.is_empty() {
        parts.push(file.metadata.hdr.as_str());
    }
    if !file.metadata.video_codec.is_empty() {
        parts.push(file.metadata.video_codec.as_str());
    }
    if !file.metadata.audio_codec.is_empty() {
        parts.push(file.metadata.audio_codec.as_str());
    }

    let prefix = format!("{}{}", name_prefix, parts.join("."));
    if file.metadata.release_group.is_empty() {
        format!(
            "{}{}",
            prefix.trim_end_matches("."),
            file.metadata.extension
        )
    } else {
        format!(
            "{}-{}{}",
            prefix.trim_end_matches("."),
            file.metadata.release_group,
            file.metadata.extension
        )
    }
}

pub(crate) fn need_overwrite_existing_files(
    existing_files: &[MediaFile],
    media_file: &MediaFile,
) -> bool {
    existing_files
        .iter()
        .all(|file| file.video.size < media_file.video.size)
}

pub(crate) fn collect_replaced_media_files<'a>(
    existing_files: &'a [MediaFile],
    saved_filename: &Option<String>,
) -> Vec<&'a MediaFile> {
    let Some(saved_filename) = saved_filename else {
        return Vec::new();
    };

    existing_files
        .iter()
        .filter(|file| file.video.name != *saved_filename)
        .collect()
}

pub(crate) fn select_largest_media_file<'a>(
    media_files: &'a [&MediaFile],
    context: &str,
) -> AppResult<&'a MediaFile> {
    media_files
        .iter()
        .max_by(|a, b| a.video.size.cmp(&b.video.size))
        .copied()
        .ok_or_else(|| AppError::NotFound(format!("no video file found when transfer {}", context)))
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::share::{Etag, RawFile},
        domain::media::Metadata,
    };

    use super::*;

    fn create_mock_media_file(size: u64) -> MediaFile {
        MediaFile {
            metadata: Box::new(Metadata::default()),
            video: RawFile {
                id: None,
                name: "test.mkv".to_string(),
                etag: Etag::Md5("etag".to_string()),
                size,
                path: "/path".to_string(),
            },
            subtitles: Vec::new(),
        }
    }

    fn create_media_file_with_metadata(name: &str, metadata: Metadata) -> MediaFile {
        MediaFile {
            metadata: Box::new(metadata),
            video: RawFile {
                id: None,
                name: name.to_string(),
                etag: Etag::Md5("etag".to_string()),
                size: 1000,
                path: "/path".to_string(),
            },
            subtitles: Vec::new(),
        }
    }

    #[test]
    fn test_format_video_file_name_already_has_prefix() {
        let prefix = "The Matrix.1999.";
        let file = create_media_file_with_metadata(
            "The Matrix.1999.BluRay.1080p.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "1080p".to_string(),
                quality: "BluRay".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "The Matrix.1999.BluRay.1080p.mkv");
    }

    #[test]
    fn test_format_video_file_name_with_all_metadata() {
        let prefix = "Breaking Bad.2008.S01E01.";
        let file = create_media_file_with_metadata(
            "original_name.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                frame_rate: "60fps".to_string(),
                quality: "WEB-DL".to_string(),
                hdr: "HDR10".to_string(),
                video_codec: "H265".to_string(),
                audio_codec: "DTS".to_string(),
                release_group: "RARBG".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(
            result,
            "Breaking Bad.2008.S01E01.2160p.60fps.WEB-DL.HDR10.H265.DTS-RARBG.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_no_release_group() {
        let prefix = "Inception.2010.";
        let file = create_media_file_with_metadata(
            "original.mp4",
            Metadata {
                extension: ".mp4".to_string(),
                resolution: "1080p".to_string(),
                frame_rate: "24fps".to_string(),
                quality: "BluRay".to_string(),
                video_codec: "H264".to_string(),
                audio_codec: "AAC".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Inception.2010.1080p.24fps.BluRay.H264.AAC.mp4");
    }

    #[test]
    fn test_format_video_file_name_minimal_metadata() {
        let prefix = "Movie.2020.";
        let file = create_media_file_with_metadata(
            "file.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "720p".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Movie.2020.720p.mkv");
    }

    #[test]
    fn test_format_video_file_name_no_metadata() {
        let prefix = "Show.2021.";
        let file = create_media_file_with_metadata(
            "video.avi",
            Metadata {
                extension: ".avi".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Show.2021.avi");
    }

    #[test]
    fn test_format_video_file_name_hdr_variants() {
        let prefix = "HDR Movie.2022.";
        let file = create_media_file_with_metadata(
            "original.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                quality: "WEB-DL".to_string(),
                hdr: "HDR10+".to_string(),
                video_codec: "H265".to_string(),
                release_group: "NTb".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "HDR Movie.2022.2160p.WEB-DL.HDR10+.H265-NTb.mkv");

        let file_dv = create_media_file_with_metadata(
            "original.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                quality: "BluRay".to_string(),
                hdr: "DV".to_string(),
                video_codec: "H265".to_string(),
                audio_codec: "Atmos".to_string(),
                release_group: "GROUP".to_string(),
                ..Default::default()
            },
        );
        let result_dv = format_video_file_name(prefix, &file_dv);
        assert_eq!(
            result_dv,
            "HDR Movie.2022.2160p.BluRay.DV.H265.Atmos-GROUP.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_special_characters_in_prefix() {
        let prefix = "Star Wars: Episode IV.1977.";
        let file = create_media_file_with_metadata(
            "file.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "1080p".to_string(),
                quality: "BluRay".to_string(),
                video_codec: "H264".to_string(),
                release_group: "YTS".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(
            result,
            "Star Wars: Episode IV.1977.1080p.BluRay.H264-YTS.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_partial_metadata() {
        let prefix = "Series.2023.S02E05.";
        let file = create_media_file_with_metadata(
            "ep.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                frame_rate: "30fps".to_string(),
                video_codec: "H264".to_string(),
                release_group: "AMZN".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Series.2023.S02E05.30fps.H264-AMZN.mkv");
    }

    #[test]
    fn test_format_video_file_name_different_extensions() {
        let prefix = "Video.2024.";

        let file_mp4 = create_media_file_with_metadata(
            "file.mp4",
            Metadata {
                extension: ".mp4".to_string(),
                resolution: "1080p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_mp4),
            "Video.2024.1080p.mp4"
        );

        let file_avi = create_media_file_with_metadata(
            "file.avi",
            Metadata {
                extension: ".avi".to_string(),
                resolution: "720p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_avi),
            "Video.2024.720p.avi"
        );

        let file_webm = create_media_file_with_metadata(
            "file.webm",
            Metadata {
                extension: ".webm".to_string(),
                resolution: "480p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_webm),
            "Video.2024.480p.webm"
        );
    }

    #[test]
    fn test_need_overwrite_existing_files() {
        let existing_files_1 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_1 = create_mock_media_file(300);
        assert!(need_overwrite_existing_files(
            &existing_files_1,
            &new_file_1
        ));

        let existing_files_2 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_2 = create_mock_media_file(50);
        assert!(!need_overwrite_existing_files(
            &existing_files_2,
            &new_file_2
        ));

        let existing_files_3 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_3 = create_mock_media_file(200);
        assert!(!need_overwrite_existing_files(
            &existing_files_3,
            &new_file_3
        ));

        let existing_files_4 = vec![];
        let new_file_4 = create_mock_media_file(100);
        assert!(need_overwrite_existing_files(
            &existing_files_4,
            &new_file_4
        ));
    }

    #[test]
    fn test_should_skip_existing_media_only_when_existing_is_not_smaller() {
        let incoming = create_mock_media_file(100);
        let smaller_existing = vec![create_mock_media_file(99)];
        let larger_existing = vec![create_mock_media_file(101)];

        assert!(!should_skip_existing_media(&[], &incoming));
        assert!(!should_skip_existing_media(&smaller_existing, &incoming));
        assert!(should_skip_existing_media(&larger_existing, &incoming));
    }

    #[test]
    fn test_collect_replaced_media_files_returns_empty_without_saved_filename() {
        let existing_files = vec![create_mock_media_file(100), create_mock_media_file(200)];

        let files = collect_replaced_media_files(&existing_files, &None);

        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_replaced_media_files_excludes_saved_filename() {
        let existing_files = vec![
            create_media_file_with_metadata("kept.mkv", Metadata::default()),
            create_media_file_with_metadata("old1.mkv", Metadata::default()),
            create_media_file_with_metadata("old2.mkv", Metadata::default()),
        ];

        let files = collect_replaced_media_files(&existing_files, &Some("kept.mkv".to_string()));

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].video.name, "old1.mkv");
        assert_eq!(files[1].video.name, "old2.mkv");
    }

    #[test]
    fn test_select_largest_media_file_returns_largest() {
        let small = create_mock_media_file(100);
        let large = create_mock_media_file(300);
        let medium = create_mock_media_file(200);
        let media_files = vec![&small, &large, &medium];

        let selected = select_largest_media_file(&media_files, "movie test").unwrap();

        assert_eq!(selected.video.size, 300);
    }

    #[test]
    fn test_select_largest_media_file_errors_on_empty_input() {
        let media_files: Vec<&MediaFile> = Vec::new();

        let error = select_largest_media_file(&media_files, "movie test").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("no video file found when transfer movie test")
        );
    }
}
