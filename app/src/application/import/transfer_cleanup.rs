use crate::domain::import::inner::MediaFile;

use super::{
    Importer,
    transfer_support::{
        build_local_cleanup_paths, collect_library_file_ids, files_pending_cleanup,
    },
};
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
    pub(super) async fn cleanup_replaced_movie_files(
        &self,
        movie_path: &str,
        existing_files: &[MediaFile],
        saved_filename: &Option<String>,
    ) -> AppResult<()> {
        self.cleanup_replaced_files(movie_path, Some(existing_files), saved_filename.as_deref())
            .await
    }

    pub(super) async fn cleanup_replaced_episode_files(
        &self,
        season_full_path: &str,
        existing_files: Option<&Vec<MediaFile>>,
        saved_filename: &str,
    ) -> AppResult<()> {
        self.cleanup_replaced_files(
            season_full_path,
            existing_files.map(|files| files.as_slice()),
            Some(saved_filename),
        )
        .await
    }

    async fn cleanup_replaced_files(
        &self,
        remote_parent_path: &str,
        existing_files: Option<&[MediaFile]>,
        saved_filename: Option<&str>,
    ) -> AppResult<()> {
        let files = files_pending_cleanup(existing_files, saved_filename);
        if files.is_empty() {
            return Ok(());
        }

        self.delete_files_in_library(&files).await?;
        self.delete_files_in_local(remote_parent_path, &files).await
    }

    async fn delete_files_in_library(&self, files: &[&MediaFile]) -> AppResult<()> {
        for f in files {
            info!(
                "Deleting file {} from library, file id: {:?}",
                f.video.name, f.video.id
            );
            for s in &f.subtitles {
                info!("Deleting file {} from library, file id: {:?}", s.name, s.id);
            }
        }

        let file_ids = collect_library_file_ids(files);
        self.library_gateway
            .trash_library_files(file_ids.as_slice())
            .await?;
        Ok(())
    }

    async fn delete_files_in_local(
        &self,
        remote_parent_path: &str,
        files: &[&MediaFile],
    ) -> AppResult<()> {
        let local_parent_path = self.local.local_path_for_remote(remote_parent_path);

        for f in files {
            for local_file_path in build_local_cleanup_paths(&local_parent_path, f) {
                info!("Deleting local file {}", local_file_path);
                self.local
                    .remove_local_file_if_exists(local_file_path.as_str())
                    .await?;
            }
        }

        Ok(())
    }
}
