mod upload;

use crate::domain::import::inner::{MediaFile, RawFile};
use crate::domain::import::policy::format_video_file_name;

use super::{
    TransferWorkflow,
    transfer_support::{
        build_subtitle_transfer_plan, log_file_not_saved, log_file_saved, remote_child_path,
    },
};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::error::AppResult;
use tracing::info;

impl<L, M, F> TransferWorkflow<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
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
                &media_file.video.etag,
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

    async fn create_strm_file(
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
        Ok(())
    }

    async fn transfer_subtitle_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        raw_file: &RawFile,
        file_name: &str,
    ) -> AppResult<bool> {
        let res = self
            .transfer_raw_file_with_logging(parent_dir_id, file_name, raw_file.size, &raw_file.etag)
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

    async fn finish_video_transfer(
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

    async fn finish_subtitle_transfer(
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
        Ok(true)
    }
}
