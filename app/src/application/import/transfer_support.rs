mod cleanup;
mod paths;

use std::collections::HashMap;
use std::time::Duration;

use tracing::info;

use crate::domain::import::{
    inner::MediaFile,
    paths::get_year_from_date,
    policy::{
        SeasonTransferState, get_max_episode_number, get_missing_episodes,
        get_number_of_episodes_in_season, need_overwrite_existing_files,
    },
};

use super::{ImportedMedia, TvDetail};
pub(super) use cleanup::{collect_library_file_ids, existing_season_dir_id, files_pending_cleanup};
pub(super) use paths::{
    build_local_cleanup_paths, build_subtitle_transfer_plan, remote_child_path,
    renamed_subtitle_file_name,
};

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

pub(super) fn log_file_not_saved(file_name: &str) {
    info!("File {} not saved in library", file_name);
}

pub(super) fn log_file_saved(file_name: &str, file_id: i64) {
    info!("File {} saved in library, file id: {}", file_name, file_id);
}

#[cfg(test)]
#[path = "transfer_support/tests.rs"]
mod tests;
