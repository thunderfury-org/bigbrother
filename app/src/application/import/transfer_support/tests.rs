use super::*;
use crate::{
    application::import::transfer_support::paths::renamed_subtitle_file_name,
    application::import::{Genre, Season},
    domain::import::inner::{Etag, RawFile},
    domain::import::policy::should_skip_existing_media,
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
    let existing_episode_files = HashMap::from([(2, vec![create_media_file("S01E02.mkv", 100)])]);

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
fn accumulate_episode_transfer_result_ignores_none() {
    let mut state = SeasonTransferState::default();

    accumulate_episode_transfer_result(&mut state, 3, None);

    assert_eq!(state, SeasonTransferState::default());
}

#[test]
fn accumulate_episode_transfer_result_records_success() {
    let mut state = SeasonTransferState::default();

    accumulate_episode_transfer_result(&mut state, 3, Some((true, 1024)));

    assert_eq!(
        state,
        SeasonTransferState {
            has_failed: false,
            total_size: 1024,
            episodes: vec![3],
        }
    );
}

#[test]
fn accumulate_episode_transfer_result_records_failure_without_episode() {
    let mut state = SeasonTransferState::default();

    accumulate_episode_transfer_result(&mut state, 3, Some((false, 1024)));

    assert_eq!(
        state,
        SeasonTransferState {
            has_failed: true,
            total_size: 0,
            episodes: vec![],
        }
    );
}

#[test]
fn get_number_of_episodes_in_season_returns_matching_count() {
    assert_eq!(get_number_of_episodes_in_season(&create_tv_detail(), 1), 8);
}

#[test]
fn get_number_of_episodes_in_season_returns_zero_when_missing() {
    assert_eq!(get_number_of_episodes_in_season(&create_tv_detail(), 3), 0);
}

#[test]
fn get_max_episode_number_considers_imported_and_existing() {
    let existing = HashMap::from([(6, vec![create_media_file("S01E06.mkv", 100)])]);

    assert_eq!(get_max_episode_number(&[1, 2, 4], &existing), 6);
}

#[test]
fn get_missing_episodes_merges_imported_and_existing() {
    let existing = HashMap::from([(4, vec![create_media_file("S01E04.mkv", 100)])]);

    assert_eq!(get_missing_episodes(5, &[1, 3], &existing), vec![2, 5]);
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
