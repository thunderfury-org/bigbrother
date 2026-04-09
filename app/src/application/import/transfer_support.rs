use std::collections::HashMap;
use std::time::Duration;

use tracing::info;

use crate::domain::import::{
    inner::{MediaFile, RawFile},
    paths::get_year_from_date,
    policy::{
        SeasonTransferState, collect_replaced_media_files, get_max_episode_number,
        get_missing_episodes, get_number_of_episodes_in_season, need_overwrite_existing_files,
    },
};

use super::{ImportedMedia, TvDetail};

pub(super) fn should_skip_existing_media(
    existing_files: &[MediaFile],
    media_file: &MediaFile,
) -> bool {
    !existing_files.is_empty() && !need_overwrite_existing_files(existing_files, media_file)
}

pub(super) fn build_imported_tv_result(
    detail: &TvDetail,
    season_number: u32,
    state: SeasonTransferState,
    existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    cost: Duration,
) -> ImportedMedia {
    let max_episode_number = get_max_episode_number(&state.episodes, existing_episode_files);

    ImportedMedia::Tv {
        name: detail.name.to_owned(),
        year: get_year_from_date(detail.first_air_date.as_str()).to_owned(),
        season: season_number,
        missing_episodes: get_missing_episodes(
            max_episode_number,
            &state.episodes,
            existing_episode_files,
        ),
        episodes: state.episodes,
        max_episode_number,
        total_size: state.total_size,
        number_of_episodes: get_number_of_episodes_in_season(detail, season_number),
        cost,
        _has_failed: state.has_failed,
    }
}

pub(super) fn build_local_cleanup_paths(
    local_parent_path: &str,
    media_file: &MediaFile,
) -> Vec<String> {
    let mut paths = vec![format!(
        "{}/{}.strm",
        local_parent_path,
        media_file
            .video
            .name
            .trim_end_matches(media_file.metadata.extension.as_str())
    )];
    paths.extend(
        media_file
            .subtitles
            .iter()
            .map(|subtitle| format!("{}/{}", local_parent_path, subtitle.name)),
    );
    paths
}

pub(super) fn renamed_subtitle_file_name(
    raw_file: &RawFile,
    source_video_name: &str,
    target_video_name: &str,
    extension: &str,
) -> String {
    raw_file.name.replace(
        source_video_name.trim_end_matches(extension),
        target_video_name.trim_end_matches(extension),
    )
}

pub(super) fn files_pending_cleanup<'a>(
    existing_files: Option<&'a [MediaFile]>,
    saved_filename: Option<&str>,
) -> Vec<&'a MediaFile> {
    let Some(existing_files) = existing_files else {
        return Vec::new();
    };
    let Some(saved_filename) = saved_filename else {
        return Vec::new();
    };

    collect_replaced_media_files(existing_files, &Some(saved_filename.to_string()))
}

pub(super) fn collect_library_file_ids(files: &[&MediaFile]) -> Vec<i64> {
    let mut file_ids = Vec::new();

    for media_file in files {
        if let Some(id) = media_file.video.id {
            file_ids.push(id);
        }
        file_ids.extend(
            media_file
                .subtitles
                .iter()
                .filter_map(|subtitle| subtitle.id),
        );
    }

    file_ids
}

pub(super) fn existing_season_dir_id(
    season_dir: &str,
    season_dir_ids: &HashMap<String, i64>,
) -> Option<i64> {
    season_dir_ids.get(season_dir).copied()
}

pub(super) fn log_file_not_saved(file_name: &str) {
    info!("File {} not saved in library", file_name);
}

pub(super) fn log_file_saved(file_name: &str, file_id: i64) {
    info!("File {} saved in library, file id: {}", file_name, file_id);
}

pub(super) fn remote_child_path(parent_path: &str, file_name: &str) -> String {
    format!("{}/{}", parent_path, file_name)
}

pub(super) fn build_subtitle_transfer_plan<'a>(
    media_file: &'a MediaFile,
    video_file_name: &str,
) -> Vec<(&'a RawFile, String)> {
    media_file
        .subtitles
        .iter()
        .map(|subtitle| {
            (
                subtitle,
                renamed_subtitle_file_name(
                    subtitle,
                    &media_file.video.name,
                    video_file_name,
                    media_file.metadata.extension.as_str(),
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::import::{Genre, Season},
        domain::import::inner::Etag,
        domain::media::Metadata,
    };

    fn create_media_file(name: &str, size: u64) -> MediaFile {
        MediaFile {
            metadata: Box::new(Metadata {
                extension: ".mkv".into(),
                ..Default::default()
            }),
            video: RawFile {
                id: Some(1),
                name: name.into(),
                etag: Etag::Md5("etag".into()),
                size,
                path: "/remote/path".into(),
            },
            subtitles: Vec::new(),
        }
    }

    fn create_subtitle(name: &str) -> RawFile {
        RawFile {
            id: Some(2),
            name: name.into(),
            etag: Etag::Md5("etag".into()),
            size: 10,
            path: "/remote/path".into(),
        }
    }

    fn create_tv_detail() -> TvDetail {
        TvDetail {
            id: 1,
            name: "Test Show".into(),
            first_air_date: "2024-01-01".into(),
            number_of_episodes: 10,
            number_of_seasons: 1,
            origin_country: vec![],
            original_language: "en".into(),
            original_name: "Test Show".into(),
            genres: vec![Genre {
                id: 1,
                name: "Drama".into(),
            }],
            seasons: vec![Season {
                id: 11,
                name: "Season 1".into(),
                season_number: 1,
                episode_count: 8,
            }],
        }
    }

    #[test]
    fn should_skip_existing_media_only_when_existing_is_not_smaller() {
        let incoming = create_media_file("incoming.mkv", 100);
        let smaller_existing = vec![create_media_file("existing.mkv", 99)];
        let larger_existing = vec![create_media_file("existing.mkv", 101)];

        assert!(!should_skip_existing_media(&[], &incoming));
        assert!(!should_skip_existing_media(&smaller_existing, &incoming));
        assert!(should_skip_existing_media(&larger_existing, &incoming));
    }

    #[test]
    fn build_imported_tv_result_merges_existing_episode_presence() {
        let detail = create_tv_detail();
        let state = SeasonTransferState {
            has_failed: false,
            total_size: 2048,
            episodes: vec![1, 3],
        };
        let existing_episode_files =
            HashMap::from([(2, vec![create_media_file("S01E02.mkv", 100)])]);

        let imported = build_imported_tv_result(
            &detail,
            1,
            state,
            &existing_episode_files,
            Duration::from_secs(3),
        );

        assert!(matches!(
            imported,
            ImportedMedia::Tv {
                season: 1,
                max_episode_number: 3,
                total_size: 2048,
                number_of_episodes: 8,
                missing_episodes,
                ..
            } if missing_episodes.is_empty()
        ));
    }

    #[test]
    fn renamed_subtitle_file_name_tracks_video_rename() {
        let subtitle = create_subtitle("Show.S01E01.zh.srt");

        let renamed = renamed_subtitle_file_name(
            &subtitle,
            "Show.S01E01.mkv",
            "Test.Show.2024.S01E01.1080p.mkv",
            ".mkv",
        );

        assert_eq!(renamed, "Test.Show.2024.S01E01.1080p.zh.srt");
    }

    #[test]
    fn build_local_cleanup_paths_includes_strm_and_subtitles() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.subtitles = vec![
            create_subtitle("Show.S01E01.zh.srt"),
            create_subtitle("Show.S01E01.en.ass"),
        ];

        let paths = build_local_cleanup_paths("/local/show", &media);

        assert_eq!(
            paths,
            vec![
                "/local/show/Show.S01E01.strm".to_string(),
                "/local/show/Show.S01E01.zh.srt".to_string(),
                "/local/show/Show.S01E01.en.ass".to_string(),
            ]
        );
    }

    #[test]
    fn files_pending_cleanup_returns_only_replaced_files() {
        let kept = create_media_file("kept.mkv", 100);
        let replaced = create_media_file("replaced.mkv", 90);
        let existing = vec![kept, replaced];

        let files = files_pending_cleanup(Some(&existing), Some("kept.mkv"));

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].video.name, "replaced.mkv");
    }

    #[test]
    fn files_pending_cleanup_returns_empty_without_inputs() {
        let existing = vec![create_media_file("kept.mkv", 100)];

        assert!(files_pending_cleanup(None, Some("kept.mkv")).is_empty());
        assert!(files_pending_cleanup(Some(&existing), None).is_empty());
    }

    #[test]
    fn collect_library_file_ids_includes_video_and_subtitles() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.video.id = Some(11);
        media.subtitles = vec![
            RawFile {
                id: Some(21),
                ..create_subtitle("Show.S01E01.zh.srt")
            },
            RawFile {
                id: None,
                ..create_subtitle("Show.S01E01.en.ass")
            },
        ];

        let ids = collect_library_file_ids(&[&media]);

        assert_eq!(ids, vec![11, 21]);
    }

    #[test]
    fn existing_season_dir_id_returns_matching_entry() {
        let season_dir_ids = HashMap::from([
            ("Season 01".to_string(), 101),
            ("Season 02".to_string(), 202),
        ]);

        assert_eq!(
            existing_season_dir_id("Season 01", &season_dir_ids),
            Some(101)
        );
        assert_eq!(existing_season_dir_id("Season 03", &season_dir_ids), None);
    }

    #[test]
    fn remote_child_path_joins_parent_and_name() {
        assert_eq!(
            remote_child_path("/remote/show", "episode01.mkv"),
            "/remote/show/episode01.mkv"
        );
    }

    #[test]
    fn build_subtitle_transfer_plan_renames_each_subtitle() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.subtitles = vec![
            create_subtitle("Show.S01E01.zh.srt"),
            create_subtitle("Show.S01E01.en.ass"),
        ];

        let plan = build_subtitle_transfer_plan(&media, "Test.Show.2024.S01E01.1080p.mkv");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].1, "Test.Show.2024.S01E01.1080p.zh.srt");
        assert_eq!(plan[1].1, "Test.Show.2024.S01E01.1080p.en.ass");
    }
}
