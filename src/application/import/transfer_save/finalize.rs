use super::TransferWorkflow;
use crate::application::import::transfer_support::{log_file_saved, remote_child_path};
use crate::application::ports::LibraryMediaUpdateKind;
use crate::error::AppResult;
use tracing::info;

impl TransferWorkflow {
    pub(super) async fn create_strm_file(
        &self,
        remote_file_path: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<()> {
        let local_file_path = self.local.local_strm_path(remote_file_path, extension);
        self.local
            .write_strm_file(remote_file_path, extension, file_id)
            .await?;
        info!("Strm file {} created", local_file_path);
        self.queue_library_update(local_file_path, LibraryMediaUpdateKind::Created)
            .await;
        Ok(())
    }

    pub(super) async fn finish_video_transfer(
        &self,
        parent_path: &str,
        video_file_name: &str,
        extension: &str,
        file_id: i64,
    ) -> AppResult<Option<String>> {
        log_file_saved(video_file_name, file_id);
        self.create_strm_file(
            remote_child_path(parent_path, video_file_name).as_str(),
            extension,
            file_id,
        )
        .await?;
        Ok(Some(video_file_name.to_owned()))
    }

    pub(super) async fn finish_subtitle_transfer(
        &self,
        parent_path: &str,
        file_name: &str,
        file_id: i64,
    ) -> AppResult<bool> {
        log_file_saved(file_name, file_id);
        let local_file_path = self
            .local()
            .local_path_for_remote(remote_child_path(parent_path, file_name).as_str());
        self.library_gateway()
            .download_library_file(file_id, local_file_path.as_str())
            .await?;
        info!("Subtitle file {} downloaded", local_file_path);
        self.queue_library_update(local_file_path, LibraryMediaUpdateKind::Created)
            .await;
        Ok(true)
    }
}
