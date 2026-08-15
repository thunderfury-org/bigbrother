mod finalize;
mod upload;

use crate::domain::import::policy::format_video_file_name;
use crate::domain::{import::inner::MediaFile, share::RawFile};

use super::{
    TransferWorkflow,
    transfer_support::{build_subtitle_transfer_plan, log_file_not_saved},
};
use crate::error::AppResult;

impl TransferWorkflow {
    pub(super) async fn transfer_media_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        name_prefix: &str,
        media_file: &MediaFile,
    ) -> AppResult<Option<String>> {
        let video_file_name = format_video_file_name(name_prefix, media_file);

        if !self
            .transfer_subtitles_for_media(parent_path, parent_dir_id, &video_file_name, media_file)
            .await?
        {
            return Ok(None);
        }

        self.transfer_video_file(
            parent_path,
            parent_dir_id,
            video_file_name.as_str(),
            media_file,
        )
        .await
    }

    async fn transfer_subtitles_for_media(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        video_file_name: &str,
        media_file: &MediaFile,
    ) -> AppResult<bool> {
        for (subtitle, subtitle_file_name) in
            build_subtitle_transfer_plan(media_file, video_file_name)
        {
            let success = self
                .transfer_subtitle_file(parent_path, parent_dir_id, subtitle, &subtitle_file_name)
                .await?;
            if !success {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn transfer_video_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        video_file_name: &str,
        media_file: &MediaFile,
    ) -> AppResult<Option<String>> {
        let res = self
            .transfer_raw_file_with_logging(
                parent_dir_id,
                video_file_name,
                media_file.video.size,
                &media_file.video.hash,
            )
            .await?;
        match res {
            Some(id) => {
                self.finish_video_transfer(
                    parent_path,
                    video_file_name,
                    media_file.metadata.extension.as_str(),
                    id,
                )
                .await
            }
            None => {
                log_file_not_saved(video_file_name);
                Ok(None)
            }
        }
    }

    async fn transfer_subtitle_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        raw_file: &RawFile,
        file_name: &str,
    ) -> AppResult<bool> {
        let res = self
            .transfer_raw_file_with_logging(parent_dir_id, file_name, raw_file.size, &raw_file.hash)
            .await?;
        match res {
            Some(id) => {
                self.finish_subtitle_transfer(parent_path, file_name, id)
                    .await
            }
            None => {
                log_file_not_saved(file_name);
                Ok(false)
            }
        }
    }
}
