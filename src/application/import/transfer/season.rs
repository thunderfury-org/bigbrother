use std::collections::{BTreeMap, HashMap};

use crate::application::import::transfer_support::build_imported_tv_result;
use crate::application::import_ports::{ImportLocalStore, LibraryGateway};
use crate::domain::import::{
    inner::{MediaFile, TransferEpisodeArgs},
    paths::get_tv_base_name,
};
use crate::{error::AppResult, log_time};

use super::{ImportedMedia, TransferWorkflow};
use crate::application::import::TvDetail;
use crate::application::import::transfer_support::{
    SeasonTransferState, accumulate_episode_transfer_result,
};

impl<L, F> TransferWorkflow<L, F>
where
    L: LibraryGateway,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_season(
        &mut self,
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
        for (episode_number, files) in season_files {
            let res = self
                .transfer_episode(&TransferEpisodeArgs {
                    detail,
                    season_number: *season_number,
                    episode_number: *episode_number,
                    files,
                    season_full_path: &season_full_path,
                    season_dir_id,
                    existing_episode_files: &existing_episode_files,
                })
                .await?;
            accumulate_episode_transfer_result(&mut state, *episode_number, res);
        }

        Ok(build_imported_tv_result(
            detail,
            *season_number,
            state,
            &existing_episode_files,
            start_time.elapsed(),
        ))
    }
}
