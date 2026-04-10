use crate::domain::import::{
    inner::MediaFile,
    paths::{get_movie_base_name, get_movie_path_in_library, get_year_from_date},
    policy::select_largest_media_file,
};

use super::{ImportedMedia, TransferImportUseCase, should_skip_existing_media};
use crate::application::import::MovieDetail;
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::{error::AppResult, log_time};

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_movie(
        &mut self,
        detail: &MovieDetail,
        media_files: &[&MediaFile],
    ) -> AppResult<Option<ImportedMedia>> {
        log_time!(format!("transfer movie {}", get_movie_base_name(detail)));
        let start_time = std::time::Instant::now();

        let remote_path = self.workflow().local().remote_library_path();
        let movie_path = get_movie_path_in_library(remote_path, detail);
        let movie_dir_id = self
            .workflow()
            .get_or_create_dir_in_library(movie_path.as_str())
            .await?;
        let existing_files = self
            .workflow_mut()
            .list_movie_files_in_library(movie_dir_id)
            .await?;
        let media_file =
            select_largest_media_file(media_files, format!("movie {}", detail.title).as_str())?;

        if should_skip_existing_media(&existing_files, media_file) {
            return Ok(None);
        }

        let name_prefix = format!(
            "{}.{}.",
            detail.title,
            get_year_from_date(detail.release_date.as_str()),
        );
        let saved_filename = self
            .workflow()
            .transfer_media_file(&movie_path, movie_dir_id, name_prefix.as_str(), media_file)
            .await?;
        self.workflow()
            .cleanup_replaced_movie_files(movie_path.as_str(), &existing_files, &saved_filename)
            .await?;
        Ok(Some(ImportedMedia::Movie {
            title: detail.title.to_owned(),
            year: get_year_from_date(detail.release_date.as_str()).to_owned(),
            size: media_file.video.size,
            cost: start_time.elapsed(),
            has_failed: saved_filename.is_none(),
        }))
    }
}
