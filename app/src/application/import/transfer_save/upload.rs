use crate::domain::import::inner::Etag;

use super::TransferWorkflow;
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::error::AppResult;
use tracing::error;

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_raw_file(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        etag: &Etag,
    ) -> AppResult<Option<i64>> {
        Ok(match &etag {
            Etag::Md5(etag) => {
                self.library_gateway
                    .fast_upload_md5(parent_dir_id, file_name, etag, size)
                    .await?
            }
            Etag::Sha1(sha1) => {
                self.library_gateway
                    .fast_upload_sha1(parent_dir_id, file_name, sha1, size)
                    .await?
            }
        })
    }

    pub(super) async fn transfer_raw_file_with_logging(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        etag: &Etag,
    ) -> AppResult<Option<i64>> {
        self.transfer_raw_file(parent_dir_id, file_name, size, etag)
            .await
            .inspect_err(|error| {
                error!("Failed to transfer file {}, error: {}", file_name, error);
            })
    }
}
