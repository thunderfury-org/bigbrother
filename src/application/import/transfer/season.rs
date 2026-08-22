use std::collections::{BTreeMap, HashMap};

use futures::stream::{FuturesUnordered, StreamExt};

use crate::application::import::transfer_support::build_imported_tv_result;
use crate::domain::import::{
    inner::{MediaFile, TransferEpisodeArgs},
    paths::get_tv_base_name,
};
use crate::{error::AppResult, log_time};

use super::{ImportedMedia, TransferWorkflow};
use crate::application::import::transfer_support::{
    SeasonTransferState, accumulate_episode_transfer_result,
};
use crate::domain::import::TvDetail;

const EPISODE_TRANSFER_CONCURRENCY: usize = 4;

impl TransferWorkflow {
    pub(super) async fn transfer_season(
        &self,
        detail: &TvDetail,
        season_number: &u32,
        season_files: &BTreeMap<u32, Vec<&MediaFile>>,
        tv_path: &str,
        tv_dir_id: i64,
        season_dir_ids: &HashMap<String, i64>,
    ) -> AppResult<ImportedMedia> {
        log_time!(format!(
            "transfer tv {} season {:02}",
            get_tv_base_name(detail),
            season_number
        ));
        let start_time = std::time::Instant::now();

        let season_dir = format!("Season {:02}", season_number);
        let (season_dir_id, existing_episode_files) = self
            .resolve_season_target(
                detail,
                season_number,
                tv_dir_id,
                season_dir.as_str(),
                season_dir_ids,
            )
            .await?;

        let mut state = SeasonTransferState::default();

        let season_full_path = format!("{}/{}", tv_path, season_dir);
        let episode_results = self
            .transfer_season_episodes(
                detail,
                *season_number,
                season_files,
                &season_full_path,
                season_dir_id,
                &existing_episode_files,
            )
            .await?;
        for (episode_number, res) in episode_results {
            accumulate_episode_transfer_result(&mut state, episode_number, res);
        }

        Ok(build_imported_tv_result(
            detail,
            *season_number,
            state,
            &existing_episode_files,
            start_time.elapsed(),
        ))
    }

    async fn transfer_season_episodes(
        &self,
        detail: &TvDetail,
        season_number: u32,
        season_files: &BTreeMap<u32, Vec<&MediaFile>>,
        season_full_path: &str,
        season_dir_id: i64,
        existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    ) -> AppResult<Vec<(u32, Option<(bool, u64)>)>> {
        let episodes: Vec<(u32, Vec<&MediaFile>)> = season_files
            .iter()
            .map(|(&episode_number, files)| (episode_number, files.clone()))
            .collect();

        let mut next_index = 0;
        let mut in_flight = FuturesUnordered::new();
        let mut results = Vec::with_capacity(episodes.len());
        let mut first_err = None;

        loop {
            while first_err.is_none()
                && next_index < episodes.len()
                && in_flight.len() < EPISODE_TRANSFER_CONCURRENCY
            {
                let (episode_number, files) = &episodes[next_index];
                let episode_number = *episode_number;
                let files = files.as_slice();
                next_index += 1;
                in_flight.push(async move {
                    let result = self
                        .transfer_episode(&TransferEpisodeArgs {
                            detail,
                            season_number,
                            episode_number,
                            files,
                            season_full_path,
                            season_dir_id,
                            existing_episode_files,
                        })
                        .await;
                    (episode_number, result)
                });
            }

            match in_flight.next().await {
                Some((episode_number, Ok(value))) => results.push((episode_number, value)),
                Some((_, Err(err))) => {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                }
                None => break,
            }
        }

        if let Some(err) = first_err {
            return Err(err);
        }

        results.sort_unstable_by_key(|(episode_number, _)| *episode_number);
        Ok(results)
    }
}
