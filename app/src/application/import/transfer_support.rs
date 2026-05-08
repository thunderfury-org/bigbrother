mod cleanup;
mod paths;

use std::collections::HashMap;
use std::time::Duration;

use tracing::info;

use crate::domain::import::{inner::MediaFile, paths::get_year_from_date};

use super::{ImportedMedia, TvDetail};
pub(super) use cleanup::{collect_library_file_ids, existing_season_dir_id, files_pending_cleanup};
pub(super) use paths::{
    build_local_cleanup_paths, build_subtitle_transfer_plan, remote_child_path,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SeasonTransferState {
    pub(super) has_failed: bool,
    pub(super) total_size: u64,
    pub(super) episodes: Vec<u32>,
    pub(super) failed_episodes: Vec<u32>,
}

pub(super) fn accumulate_episode_transfer_result(
    state: &mut SeasonTransferState,
    episode_number: u32,
    result: Option<(bool, u64)>,
) {
    let Some((success, size)) = result else {
        return;
    };

    if success {
        state.total_size += size;
        state.episodes.push(episode_number);
    } else {
        state.has_failed = true;
        state.failed_episodes.push(episode_number);
    }
}

fn get_number_of_episodes_in_season(detail: &TvDetail, season_number: u32) -> u32 {
    detail
        .seasons
        .iter()
        .find(|season| season.season_number == season_number)
        .map(|season| season.episode_count)
        .unwrap_or_default()
}

fn get_max_episode_number(
    episodes: &[u32],
    existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
) -> u32 {
    std::cmp::max(
        *episodes.iter().max().unwrap_or(&0),
        *existing_episode_files.keys().max().unwrap_or(&0),
    )
}

fn get_missing_episodes(
    max_episode_number: u32,
    episodes: &[u32],
    existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
) -> Vec<u32> {
    let mut existing_episodes: std::collections::HashSet<u32> = episodes.iter().copied().collect();
    for episode in existing_episode_files.keys() {
        existing_episodes.insert(*episode);
    }

    let mut missing_episodes = Vec::new();
    for episode_number in 1..=max_episode_number {
        if !existing_episodes.contains(&episode_number) {
            missing_episodes.push(episode_number);
        }
    }
    missing_episodes
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
        has_failed: state.has_failed,
        failed_episodes: state.failed_episodes,
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
