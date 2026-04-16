use crate::domain::import::{
    inner::MediaFile,
    share_collect::{
        collect_pan115_directory_entries, collect_pan123_directory_entries,
        collect_pan189_directory_entries,
    },
    share_walk::ShareTraversal,
};

use super::ShareImportUseCase;
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::error::AppResult;

impl<L, S, M, F> ShareImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn list_files_from_pan123_share(
        &mut self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_source()
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(self
            .metadata_lookup_mut()
            .build_media_files(traversal.into_raw_files()))
    }

    pub(super) async fn list_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let share_info = self
            .share_source()
            .get_pan189_share_info(share_code)
            .await?;

        let mut traversal =
            ShareTraversal::new((share_info.file_id, share_info.file_name.to_owned()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source()
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;

            traversal.extend(collect_pan189_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        Ok(self
            .metadata_lookup_mut()
            .build_media_files(traversal.into_raw_files()))
    }

    pub(super) async fn list_files_from_pan115_share(
        &mut self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_source()
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(self
            .metadata_lookup_mut()
            .build_media_files(traversal.into_raw_files()))
    }
}
