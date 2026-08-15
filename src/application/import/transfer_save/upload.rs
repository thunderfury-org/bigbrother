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
        Ok(match &hash {
            FileHash::Md5(hash) => {
                self.library_gateway
                    .fast_upload_md5(parent_dir_id, file_name, hash, size)
                    .await?
            }
            FileHash::Sha1(sha1) => {
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
        hash: &FileHash,
    ) -> AppResult<Option<i64>> {
        self.transfer_raw_file(parent_dir_id, file_name, size, hash)
            .await
            .inspect_err(|error| {
                error!("Failed to transfer file {}, error: {}", file_name, error);
            })
    }
}
