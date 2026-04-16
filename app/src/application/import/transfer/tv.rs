use std::collections::BTreeMap;

use crate::domain::import::{
    inner::MediaFile,
    paths::{get_tv_base_name, get_tv_path_in_library},
};

use super::{ImportedMedia, TransferImportUseCase, TvDetail};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::{error::AppResult, log_time};

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_tv(
        &mut self,
        detail: &TvDetail,
        files: &BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>,
    ) -> AppResult<Vec<ImportedMedia>> {
        log_time!(format!("transfer tv {}", get_tv_base_name(detail)));

        let remote_path = self.workflow().local().remote_library_path();
        let tv_path = get_tv_path_in_library(remote_path, detail);
        let tv_dir_id = self
            .workflow()
            .get_or_create_dir_in_library(tv_path.as_str())
            .await?;
        let season_dir_ids = self
            .workflow()
            .library_gateway()
            .list_library_dir_ids(tv_dir_id)
            .await?;

        let mut results = Vec::new();
        for (season_number, season_files) in files {
            results.push(
                self.transfer_season(
                    detail,
                    season_number,
                    season_files,
                    &tv_path,
                    tv_dir_id,
                    &season_dir_ids,
                )
                .await?,
            );
        }

        Ok(results)
    }
}
