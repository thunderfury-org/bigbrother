use crate::application::import::transfer_support::should_skip_existing_media;
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::domain::import::{
    inner::TransferEpisodeArgs, paths::get_year_from_date, policy::select_largest_media_file,
};
use crate::error::AppResult;

use super::TransferImportUseCase;

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_episode(
        &self,
        args: &TransferEpisodeArgs<'_>,
    ) -> AppResult<Option<(bool, u64)>> {
        let media_file = select_largest_media_file(
            args.files,
            format!(
                "tv series {} season {} episode {}",
                args.detail.name, args.season_number, args.episode_number
            )
            .as_str(),
        )?;

        if args
            .existing_episode_files
            .get(&args.episode_number)
            .is_some_and(|existing_files| should_skip_existing_media(existing_files, media_file))
        {
            return Ok(None);
        }

        let name_prefix = format!(
            "{}.{}.S{:02}E{:02}.",
            args.detail.name,
            get_year_from_date(args.detail.first_air_date.as_str()),
            args.season_number,
            args.episode_number
        );
        let saved_filename = self
            .workflow()
            .transfer_media_file(
                args.season_full_path,
                args.season_dir_id,
                name_prefix.as_str(),
                media_file,
            )
            .await?;

        match saved_filename {
            Some(name) => {
                self.workflow()
                    .cleanup_replaced_episode_files(
                        args.season_full_path,
                        args.existing_episode_files.get(&args.episode_number),
                        name.as_str(),
                    )
                    .await?;

                Ok(Some((true, media_file.video.size)))
            }
            None => Ok(Some((false, 0))),
        }
    }
}
