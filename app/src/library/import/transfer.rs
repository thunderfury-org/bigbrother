use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use tracing::{error, info};

use super::{
    ImportedMedia, Importer, MovieDetail, TvDetail,
    inner::{Etag, Media, MediaFile, RawFile, TransferEpisodeArgs},
    library,
    policy::{
        SeasonTransferState, accumulate_episode_transfer_result, collect_replaced_media_files,
        format_video_file_name, get_max_episode_number, get_missing_episodes,
        get_number_of_episodes_in_season, need_overwrite_existing_files, select_largest_media_file,
    },
};
use crate::application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource};
use crate::{error::AppResult, log_time};

fn should_skip_existing_media(existing_files: &[MediaFile], media_file: &MediaFile) -> bool {
    !existing_files.is_empty() && !need_overwrite_existing_files(existing_files, media_file)
}

fn build_imported_tv_result(
    detail: &TvDetail,
    season_number: u32,
    state: SeasonTransferState,
    existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    cost: Duration,
) -> ImportedMedia {
    let max_episode_number = get_max_episode_number(&state.episodes, existing_episode_files);

    ImportedMedia::Tv {
        name: detail.name.to_owned(),
        year: library::get_year_from_date(detail.first_air_date.as_str()).to_owned(),
        season: season_number,
        missing_episodes: get_missing_episodes(
            max_episode_number,
            &state.episodes,
            existing_episode_files,
        ),
        episodes: state.episodes,
        max_episode_number,
        total_size: state.total_size,
        number_of_episodes: get_number_of_episodes_in_season(detail, season_number),
        cost,
        _has_failed: state.has_failed,
    }
}

fn build_local_cleanup_paths(local_parent_path: &str, media_file: &MediaFile) -> Vec<String> {
    let mut paths = vec![format!(
        "{}/{}.strm",
        local_parent_path,
        media_file
            .video
            .name
            .trim_end_matches(media_file.metadata.extension.as_str())
    )];
    paths.extend(
        media_file
            .subtitles
            .iter()
            .map(|subtitle| format!("{}/{}", local_parent_path, subtitle.name)),
    );
    paths
}

fn renamed_subtitle_file_name(
    raw_file: &RawFile,
    source_video_name: &str,
    target_video_name: &str,
    extension: &str,
) -> String {
    raw_file.name.replace(
        source_video_name.trim_end_matches(extension),
        target_video_name.trim_end_matches(extension),
    )
}

fn files_pending_cleanup<'a>(
    existing_files: Option<&'a [MediaFile]>,
    saved_filename: Option<&str>,
) -> Vec<&'a MediaFile> {
    let Some(existing_files) = existing_files else {
        return Vec::new();
    };
    let Some(saved_filename) = saved_filename else {
        return Vec::new();
    };

    collect_replaced_media_files(existing_files, &Some(saved_filename.to_string()))
}

fn collect_library_file_ids(files: &[&MediaFile]) -> Vec<i64> {
    let mut file_ids = Vec::new();

    for media_file in files {
        if let Some(id) = media_file.video.id {
            file_ids.push(id);
        }
        file_ids.extend(
            media_file
                .subtitles
                .iter()
                .filter_map(|subtitle| subtitle.id),
        );
    }

    file_ids
}

fn existing_season_dir_id(season_dir: &str, season_dir_ids: &HashMap<String, i64>) -> Option<i64> {
    season_dir_ids.get(season_dir).copied()
}

fn log_file_not_saved(file_name: &str) {
    info!("File {} not saved in library", file_name);
}

fn log_file_saved(file_name: &str, file_id: i64) {
    info!("File {} saved in library, file id: {}", file_name, file_id);
}

fn remote_child_path(parent_path: &str, file_name: &str) -> String {
    format!("{}/{}", parent_path, file_name)
}

fn build_subtitle_transfer_plan<'a>(
    media_file: &'a MediaFile,
    video_file_name: &str,
) -> Vec<(&'a RawFile, String)> {
    media_file
        .subtitles
        .iter()
        .map(|subtitle| {
            (
                subtitle,
                renamed_subtitle_file_name(
                    subtitle,
                    &media_file.video.name,
                    video_file_name,
                    media_file.metadata.extension.as_str(),
                ),
            )
        })
        .collect()
}

impl<L, S, M> Importer<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
{
    pub(super) async fn transfer_media_files(
        &mut self,
        media_files: &[MediaFile],
    ) -> AppResult<Vec<ImportedMedia>> {
        let mut results = Vec::with_capacity(media_files.len());

        let medias = self.group_media_files(media_files).await?;
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
        log_time!(format!(
            "transfer movie {}",
            library::get_movie_base_name(detail)
        ));
        let start_time = std::time::Instant::now();

        let remote_path = self.library_remote.library_remote_path();
        let movie_path = library::get_movie_path_in_library(remote_path, detail);
        let movie_dir_id = self
            .get_or_create_dir_in_library(movie_path.as_str())
            .await?;
        let existing_files = self.list_movie_files_in_library(movie_dir_id).await?;
        let media_file =
            select_largest_media_file(media_files, format!("movie {}", detail.title).as_str())?;

        if should_skip_existing_media(&existing_files, media_file) {
            return Ok(None);
        }

        let name_prefix = format!(
            "{}.{}.",
            detail.title,
            library::get_year_from_date(detail.release_date.as_str()),
        );
        let saved_filename = self
            .transfer_media_file(&movie_path, movie_dir_id, name_prefix.as_str(), media_file)
            .await?;
        self.cleanup_replaced_movie_files(movie_path.as_str(), &existing_files, &saved_filename)
            .await?;
        Ok(Some(ImportedMedia::Movie {
            title: detail.title.to_owned(),
            year: library::get_year_from_date(detail.release_date.as_str()).to_owned(),
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
        log_time!(format!("transfer tv {}", library::get_tv_base_name(detail)));

        let remote_path = self.library_remote.library_remote_path();
        let tv_path = library::get_tv_path_in_library(remote_path, detail);
        let tv_dir_id = self.get_or_create_dir_in_library(tv_path.as_str()).await?;
        let season_dir_ids = self.library_remote.list_library_dir_ids(tv_dir_id).await?;

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
            library::get_tv_base_name(detail),
            season_number
        ));
        let start_time = std::time::Instant::now();

        let season_dir = format!("Season {:02}", season_number);
        let (season_dir_id, existing_episode_files) = self
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
            library::get_year_from_date(args.detail.first_air_date.as_str()),
            args.season_number,
            args.episode_number
        );
        let saved_filename = self
            .transfer_media_file(
                args.season_full_path,
                args.season_dir_id,
                name_prefix.as_str(),
                media_file,
            )
            .await?;

        match saved_filename {
            Some(name) => {
                self.cleanup_replaced_episode_files(
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

    async fn cleanup_replaced_movie_files(
        &self,
        movie_path: &str,
        existing_files: &[MediaFile],
        saved_filename: &Option<String>,
    ) -> AppResult<()> {
        self.cleanup_replaced_files(movie_path, Some(existing_files), saved_filename.as_deref())
            .await
    }

    async fn cleanup_replaced_episode_files(
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

    async fn resolve_season_target(
        &mut self,
        detail: &TvDetail,
        season_number: &u32,
        tv_dir_id: i64,
        season_dir: &str,
        season_dir_ids: &HashMap<String, i64>,
    ) -> AppResult<(i64, HashMap<u32, Vec<MediaFile>>)> {
        match existing_season_dir_id(season_dir, season_dir_ids) {
            Some(id) => Ok((id, self.list_episode_files_in_library(id).await?)),
            None => {
                let id = self
                    .library_remote
                    .mkdir_library_dir(tv_dir_id, season_dir)
                    .await?;
                info!(
                    "Tv series {} season {} folder {} created in library, id: {}",
                    detail.name, season_number, season_dir, id
                );
                Ok((id, HashMap::new()))
            }
        }
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

        self.library_remote
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

    async fn transfer_media_file(
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
            .local
            .local_path_for_remote(remote_child_path(parent_path, file_name).as_str());
        self.library_remote
            .download_library_file(file_id, local_file_path.as_str())
            .await?;
        info!("Subtitle file {} downloaded", local_file_path);
        Ok(true)
    }

    async fn transfer_raw_file(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        etag: &Etag,
    ) -> AppResult<Option<i64>> {
        Ok(match &etag {
            Etag::Md5(etag) => {
                self.library_remote
                    .fast_upload_md5(parent_dir_id, file_name, etag, size)
                    .await?
            }
            Etag::Sha1(sha1) => {
                self.library_remote
                    .fast_upload_sha1(parent_dir_id, file_name, sha1, size)
                    .await?
            }
        })
    }

    async fn transfer_raw_file_with_logging(
        &self,
        parent_dir_id: i64,
        file_name: &str,
        size: u64,
        etag: &Etag,
    ) -> AppResult<Option<i64>> {
        self.transfer_raw_file(parent_dir_id, file_name, size, etag)
            .await
            .inspect_err(|e| {
                error!("Failed to transfer file {}, error: {}", file_name, e);
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::media::Metadata,
        library::import::{
            Genre, Season,
            inner::{Etag, RawFile},
        },
    };

    fn create_media_file(name: &str, size: u64) -> MediaFile {
        MediaFile {
            metadata: Box::new(Metadata {
                extension: ".mkv".into(),
                ..Default::default()
            }),
            video: RawFile {
                id: Some(1),
                name: name.into(),
                etag: Etag::Md5("etag".into()),
                size,
                path: "/remote/path".into(),
            },
            subtitles: Vec::new(),
        }
    }

    fn create_subtitle(name: &str) -> RawFile {
        RawFile {
            id: Some(2),
            name: name.into(),
            etag: Etag::Md5("etag".into()),
            size: 10,
            path: "/remote/path".into(),
        }
    }

    fn create_tv_detail() -> TvDetail {
        TvDetail {
            id: 1,
            name: "Test Show".into(),
            first_air_date: "2024-01-01".into(),
            number_of_episodes: 10,
            number_of_seasons: 1,
            origin_country: vec![],
            original_language: "en".into(),
            original_name: "Test Show".into(),
            genres: vec![Genre {
                id: 1,
                name: "Drama".into(),
            }],
            seasons: vec![Season {
                id: 11,
                name: "Season 1".into(),
                season_number: 1,
                episode_count: 8,
            }],
        }
    }

    #[test]
    fn should_skip_existing_media_only_when_existing_is_not_smaller() {
        let incoming = create_media_file("incoming.mkv", 100);
        let smaller_existing = vec![create_media_file("existing.mkv", 99)];
        let larger_existing = vec![create_media_file("existing.mkv", 101)];

        assert!(!should_skip_existing_media(&[], &incoming));
        assert!(!should_skip_existing_media(&smaller_existing, &incoming));
        assert!(should_skip_existing_media(&larger_existing, &incoming));
    }

    #[test]
    fn build_imported_tv_result_merges_existing_episode_presence() {
        let detail = create_tv_detail();
        let state = SeasonTransferState {
            has_failed: false,
            total_size: 2048,
            episodes: vec![1, 3],
        };
        let existing_episode_files =
            HashMap::from([(2, vec![create_media_file("S01E02.mkv", 100)])]);

        let imported = build_imported_tv_result(
            &detail,
            1,
            state,
            &existing_episode_files,
            Duration::from_secs(3),
        );

        assert!(matches!(
            imported,
            ImportedMedia::Tv {
                season: 1,
                max_episode_number: 3,
                total_size: 2048,
                number_of_episodes: 8,
                missing_episodes,
                ..
            } if missing_episodes.is_empty()
        ));
    }

    #[test]
    fn renamed_subtitle_file_name_tracks_video_rename() {
        let subtitle = create_subtitle("Show.S01E01.zh.srt");

        let renamed = renamed_subtitle_file_name(
            &subtitle,
            "Show.S01E01.mkv",
            "Test.Show.2024.S01E01.1080p.mkv",
            ".mkv",
        );

        assert_eq!(renamed, "Test.Show.2024.S01E01.1080p.zh.srt");
    }

    #[test]
    fn build_local_cleanup_paths_includes_strm_and_subtitles() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.subtitles = vec![
            create_subtitle("Show.S01E01.zh.srt"),
            create_subtitle("Show.S01E01.en.ass"),
        ];

        let paths = build_local_cleanup_paths("/local/show", &media);

        assert_eq!(
            paths,
            vec![
                "/local/show/Show.S01E01.strm".to_string(),
                "/local/show/Show.S01E01.zh.srt".to_string(),
                "/local/show/Show.S01E01.en.ass".to_string(),
            ]
        );
    }

    #[test]
    fn files_pending_cleanup_returns_only_replaced_files() {
        let kept = create_media_file("kept.mkv", 100);
        let replaced = create_media_file("replaced.mkv", 90);
        let existing = vec![kept, replaced];

        let files = files_pending_cleanup(Some(&existing), Some("kept.mkv"));

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].video.name, "replaced.mkv");
    }

    #[test]
    fn files_pending_cleanup_returns_empty_without_inputs() {
        let existing = vec![create_media_file("kept.mkv", 100)];

        assert!(files_pending_cleanup(None, Some("kept.mkv")).is_empty());
        assert!(files_pending_cleanup(Some(&existing), None).is_empty());
    }

    #[test]
    fn collect_library_file_ids_includes_video_and_subtitles() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.video.id = Some(11);
        media.subtitles = vec![
            RawFile {
                id: Some(21),
                ..create_subtitle("Show.S01E01.zh.srt")
            },
            RawFile {
                id: None,
                ..create_subtitle("Show.S01E01.en.ass")
            },
        ];

        let ids = collect_library_file_ids(&[&media]);

        assert_eq!(ids, vec![11, 21]);
    }

    #[test]
    fn existing_season_dir_id_returns_matching_entry() {
        let season_dir_ids = HashMap::from([
            ("Season 01".to_string(), 101),
            ("Season 02".to_string(), 202),
        ]);

        assert_eq!(
            existing_season_dir_id("Season 01", &season_dir_ids),
            Some(101)
        );
        assert_eq!(existing_season_dir_id("Season 03", &season_dir_ids), None);
    }

    #[test]
    fn remote_child_path_joins_parent_and_name() {
        assert_eq!(
            remote_child_path("/remote/show", "episode01.mkv"),
            "/remote/show/episode01.mkv"
        );
    }

    #[test]
    fn build_subtitle_transfer_plan_renames_each_subtitle() {
        let mut media = create_media_file("Show.S01E01.mkv", 100);
        media.subtitles = vec![
            create_subtitle("Show.S01E01.zh.srt"),
            create_subtitle("Show.S01E01.en.ass"),
        ];

        let plan = build_subtitle_transfer_plan(&media, "Test.Show.2024.S01E01.1080p.mkv");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].1, "Test.Show.2024.S01E01.1080p.zh.srt");
        assert_eq!(plan[1].1, "Test.Show.2024.S01E01.1080p.en.ass");
    }
}
