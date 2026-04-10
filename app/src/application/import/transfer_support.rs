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
#[path = "transfer_support/tests.rs"]
mod tests;
