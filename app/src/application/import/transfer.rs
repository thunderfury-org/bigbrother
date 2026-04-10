use std::collections::{BTreeMap, HashMap};

use tracing::info;

use crate::domain::import::{
    inner::{Media, MediaFile, TransferEpisodeArgs},
    paths::{
        get_movie_base_name, get_movie_path_in_library, get_tv_base_name, get_tv_path_in_library,
        get_year_from_date,
    },
    policy::{SeasonTransferState, accumulate_episode_transfer_result, select_largest_media_file},
};

use super::{
    ImportedMedia, MovieDetail, TransferImportUseCase, TvDetail,
    transfer_support::{build_imported_tv_result, should_skip_existing_media},
};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::{error::AppResult, log_time};

impl<L, M, F> TransferImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) async fn transfer_media_files(
        &mut self,
        media_files: &[MediaFile],
    ) -> AppResult<Vec<ImportedMedia>> {
        let mut results = Vec::with_capacity(media_files.len());

        let medias = self.workflow_mut().group_media_files(media_files).await?;
        info!("Grouped into {} media items", medias.len());

        for media in &medias {
            match media {
                Media::Movie { detail, files } => {
                    if let Some(imported) = self.transfer_movie(detail, files).await? {
                        results.push(imported);
                    }
                }
                Media::Tv { detail, files } => {
                    results.extend(self.transfer_tv(detail, files).await?);
                }
            }
        }

        Ok(results)
    }

    async fn transfer_movie(
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

    async fn transfer_tv(
        &mut self,
        detail: &TvDetail,
        files: &BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>,
    ) -> AppResult<Vec<ImportedMedia>> {
        log_time!(format!("transfer tv {}", get_tv_base_name(detail)));

        let remote_path = self.workflow().local().remote_library_path();
        let tv_path = get_tv_path_in_library(remote_path, detail);
        let tv_dir_id = self
            .workflow()
            .get_or_create_dir_in_library(tv_path.as_str())
            .await?;
        let season_dir_ids = self
            .workflow()
            .library_gateway()
            .list_library_dir_ids(tv_dir_id)
            .await?;

        let mut results = Vec::new();
        for (season_number, season_files) in files {
            results.push(
                self.transfer_season(
                    detail,
                    season_number,
                    season_files,
                    &tv_path,
                    tv_dir_id,
                    &season_dir_ids,
                )
                .await?,
            );
        }

        Ok(results)
    }

    async fn transfer_season(
        &mut self,
        detail: &TvDetail,
        season_number: &u32,
        season_files: &BTreeMap<u32, Vec<&MediaFile>>,
        tv_path: &str,
        tv_dir_id: i64,
        season_dir_ids: &HashMap<String, i64>,
    ) -> AppResult<ImportedMedia> {
        log_time!(format!(
            "transfer tv {} season {:02}",
            get_tv_base_name(detail),
            season_number
        ));
        let start_time = std::time::Instant::now();

        let season_dir = format!("Season {:02}", season_number);
        let (season_dir_id, existing_episode_files) = self
            .workflow_mut()
            .resolve_season_target(
                detail,
                season_number,
                tv_dir_id,
                season_dir.as_str(),
                season_dir_ids,
            )
            .await?;

        let mut state = SeasonTransferState::default();

        let season_full_path = format!("{}/{}", tv_path, season_dir);
        for (episode_number, files) in season_files {
            let res = self
                .transfer_episode(&TransferEpisodeArgs {
                    detail,
                    season_number: *season_number,
                    episode_number: *episode_number,
                    files,
                    season_full_path: &season_full_path,
                    season_dir_id,
                    existing_episode_files: &existing_episode_files,
                })
                .await?;
            accumulate_episode_transfer_result(&mut state, *episode_number, res);
        }

        Ok(build_imported_tv_result(
            detail,
            *season_number,
            state,
            &existing_episode_files,
            start_time.elapsed(),
        ))
    }

    async fn transfer_episode(
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

        // save episode file
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
            // transfer failed
            None => Ok(Some((false, 0))),
        }
    }
}
