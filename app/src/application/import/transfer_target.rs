use std::collections::HashMap;

use super::{Importer, TvDetail, inner::MediaFile, transfer_support::existing_season_dir_id};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::error::AppResult;
use tracing::info;

impl<L, S, M, F> Importer<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn resolve_season_target(
        &mut self,
        detail: &TvDetail,
        season_number: &u32,
        tv_dir_id: i64,
        season_dir: &str,
        season_dir_ids: &HashMap<String, i64>,
    ) -> AppResult<(i64, HashMap<u32, Vec<MediaFile>>)> {
        match existing_season_dir_id(season_dir, season_dir_ids) {
            Some(id) => Ok((id, self.list_episode_files_in_library(id).await?)),
            None => {
                let id = self
                    .library_gateway
                    .mkdir_library_dir(tv_dir_id, season_dir)
                    .await?;
                info!(
                    "Tv series {} season {} folder {} created in library, id: {}",
                    detail.name, season_number, season_dir, id
                );
                Ok((id, HashMap::new()))
            }
        }
    }
}
