use crate::domain::share::FileHash;

use super::TransferWorkflow;
use crate::error::AppResult;
use tracing::error;

impl TransferWorkflow {
    pub(super) async fn transfer_raw_file(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        hash: &FileHash,
    ) -> AppResult<Option<i64>> {
        self.library_gateway
            .upload(parent_dir_id, file_name, hash, size)
            .await
    }

    pub(super) async fn transfer_raw_file_with_logging(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        hash: &FileHash,
    ) -> AppResult<Option<i64>> {
        self.transfer_raw_file(parent_dir_id, file_name, size, hash)
            .await
            .inspect_err(|error| {
                error!("Failed to transfer file {}, error: {}", file_name, error);
            })
    }
}
