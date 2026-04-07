use std::collections::{BTreeMap, HashMap, HashSet};

use tracing::{error, info};

use super::{
    ImportedMedia, Importer,
    inner::{Etag, Media, MediaFile, RawFile, TransferEpisodeArgs},
    library,
};
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::{AppError, AppResult},
    log_time,
};

impl Importer {
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

        let remote_path = self.remote.library_remote_path();
        let movie_path = library::get_movie_path_in_library(remote_path, detail);
        let movie_dir_id = self
            .get_or_create_dir_in_library(movie_path.as_str())
            .await?;
        let existing_files = self.list_movie_files_in_library(movie_dir_id).await?;
        let media_file = media_files
            .iter()
            .max_by(|a, b| a.video.size.cmp(&b.video.size))
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "no video file found when transfer movie {}",
                    detail.title
                ))
            })?;

        if !existing_files.is_empty() {
            // existing files found, check if need overwrite
            if !need_overwrite_existing_files(&existing_files, media_file) {
                // do not need overwrite existing files, skip
                return Ok(None);
            }
        }

        let name_prefix = format!(
            "{}.{}.",
            detail.title,
            library::get_year_from_date(detail.release_date.as_str()),
        );
        let saved_filename = self
            .transfer_media_file(&movie_path, movie_dir_id, name_prefix.as_str(), media_file)
            .await?;
        if let Some(name) = &saved_filename
            && !existing_files.is_empty()
        {
            let files = existing_files
                .iter()
                .filter(|f| f.video.name != *name)
                .collect::<Vec<_>>();
            if !files.is_empty() {
                // delete existing files
                self.delete_files_in_library(&files).await?;
                self.delete_files_in_local(movie_path.as_str(), &files)
                    .await?;
            }
        }
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

        let remote_path = self.remote.library_remote_path();
        let tv_path = library::get_tv_path_in_library(remote_path, detail);
        let tv_dir_id = self.get_or_create_dir_in_library(tv_path.as_str()).await?;
        let season_dir_ids = self.remote.list_library_dir_ids(tv_dir_id).await?;

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
        let (season_dir_id, existing_episode_files) = match season_dir_ids.get(&season_dir) {
            Some(id) => (*id, self.list_episode_files_in_library(*id).await?),
            None => {
                // create season folder if not exists
                let id = self
                    .remote
                    .mkdir_library_dir(tv_dir_id, season_dir.as_str())
                    .await?;
                info!(
                    "Tv series {} season {} folder {} created in library, id: {}",
                    detail.name, season_number, season_dir, id
                );
                (id, HashMap::new())
            }
        };

        let mut has_failed = false;
        let mut total_size = 0u64;
        let mut episodes = Vec::new();

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
            if let Some((success, size)) = res {
                if success {
                    total_size += size;
                    episodes.push(*episode_number);
                } else {
                    has_failed = true;
                }
            }
        }

        let max_episode_number = self.get_max_episode_number(&episodes, &existing_episode_files);

        Ok(ImportedMedia::Tv {
            name: detail.name.to_owned(),
            year: library::get_year_from_date(detail.first_air_date.as_str()).to_owned(),
            season: *season_number,
            missing_episodes: self.get_missing_episodes(
                max_episode_number,
                &episodes,
                &existing_episode_files,
            ),
            episodes,
            max_episode_number,
            total_size,
            number_of_episodes: self.get_number_of_episodes_in_season(detail, season_number),
            cost: start_time.elapsed(),
            _has_failed: has_failed,
        })
    }

    fn get_number_of_episodes_in_season(&self, detail: &TvDetail, season_number: &u32) -> u32 {
        for season in &detail.seasons {
            if &season.season_number == season_number {
                return season.episode_count;
            }
        }
        0
    }

    fn get_max_episode_number(
        &self,
        episodes: &[u32],
        existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    ) -> u32 {
        std::cmp::max(
            *episodes.iter().max().unwrap_or(&0),
            *existing_episode_files.keys().max().unwrap_or(&0),
        )
    }

    fn get_missing_episodes(
        &self,
        max_episode_number: u32,
        episodes: &[u32],
        existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    ) -> Vec<u32> {
        let mut existing_episodes: HashSet<u32> = episodes.iter().cloned().collect();
        for episode in existing_episode_files.keys() {
            existing_episodes.insert(*episode);
        }

        let mut missing_episodes = Vec::new();
        for episode_number in 1..=max_episode_number {
            if !existing_episodes.contains(&episode_number) {
                missing_episodes.push(episode_number);
            }
        }
        missing_episodes
    }

    async fn transfer_episode(
        &self,
        args: &TransferEpisodeArgs<'_>,
    ) -> AppResult<Option<(bool, u64)>> {
        let media_file = args
            .files
            .iter()
            .max_by(|a, b| a.video.size.cmp(&b.video.size))
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "no video file found when transfer tv series {} season {} episode {}",
                    args.detail.name, args.season_number, args.episode_number
                ))
            })?;

        if let Some(existing_files) = args.existing_episode_files.get(&args.episode_number)
            && !existing_files.is_empty()
        {
            // episode file already exists in library
            if !need_overwrite_existing_files(existing_files, media_file) {
                // existing file size is larger than new file, skip
                return Ok(None);
            }
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
                if let Some(existing_files) = args.existing_episode_files.get(&args.episode_number)
                    && !existing_files.is_empty()
                {
                    let files = existing_files
                        .iter()
                        .filter(|f| f.video.name != name)
                        .collect::<Vec<_>>();
                    if !files.is_empty() {
                        // delete existing files
                        self.delete_files_in_library(&files).await?;
                        self.delete_files_in_local(args.season_full_path, &files)
                            .await?;
                    }
                }

                Ok(Some((true, media_file.video.size)))
            }
            // transfer failed
            None => Ok(Some((false, 0))),
        }
    }

    async fn delete_files_in_library(&self, files: &[&MediaFile]) -> AppResult<()> {
        let mut file_ids = Vec::new();
        for f in files {
            info!(
                "Deleting file {} from library, file id: {:?}",
                f.video.name, f.video.id
            );
            if let Some(id) = f.video.id {
                file_ids.push(id);
            }
            for s in &f.subtitles {
                info!("Deleting file {} from library, file id: {:?}", s.name, s.id);
                if let Some(id) = s.id {
                    file_ids.push(id);
                }
            }
        }

        self.remote.trash_library_files(file_ids.as_slice()).await?;
        Ok(())
    }

    async fn delete_files_in_local(
        &self,
        remote_parent_path: &str,
        files: &[&MediaFile],
    ) -> AppResult<()> {
        let local_parent_path = self.remote.local_path_for_remote(remote_parent_path);

        for f in files {
            let local_file_path = format!(
                "{}/{}.strm",
                local_parent_path,
                f.video.name.trim_end_matches(f.metadata.extension.as_str())
            );
            info!("Deleting local file {}", local_file_path);
            self.remote
                .remove_local_file_if_exists(local_file_path.as_str())
                .await?;

            for s in &f.subtitles {
                let local_file_path = format!("{}/{}", local_parent_path, s.name);
                info!("Deleting local file {}", local_file_path);
                self.remote
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

        if !media_file.subtitles.is_empty() {
            // save subtitle files first, in case video file transfer failed
            let subtitle_file_name_replace_from = media_file
                .video
                .name
                .trim_end_matches(media_file.metadata.extension.as_str());
            let subtitle_file_name_replace_to =
                video_file_name.trim_end_matches(media_file.metadata.extension.as_str());
            for subtitle in &media_file.subtitles {
                let success = self
                    .transfer_subtitle_file(
                        parent_path,
                        parent_dir_id,
                        subtitle,
                        subtitle_file_name_replace_from,
                        subtitle_file_name_replace_to,
                    )
                    .await?;
                if !success {
                    // subtitle file transfer failed, skip the whole media file transfer
                    return Ok(None);
                }
            }
        }

        self.transfer_video_file(
            parent_path,
            parent_dir_id,
            video_file_name.as_str(),
            media_file,
        )
        .await
    }

    async fn transfer_video_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        video_file_name: &str,
        media_file: &MediaFile,
    ) -> AppResult<Option<String>> {
        let res = self
            .transfer_raw_file(
                parent_dir_id,
                video_file_name,
                media_file.video.size,
                &media_file.video.etag,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to transfer file {}, error: {}", video_file_name, e);
            })?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", video_file_name, id);

                // create strm file
                self.create_strm_file(
                    format!("{}/{}", parent_path, video_file_name,).as_str(),
                    media_file.metadata.extension.as_str(),
                    id,
                )
                .await?;

                Ok(Some(video_file_name.to_owned()))
            }
            None => {
                info!("File {} not saved in library", video_file_name);

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
        let local_file_path = self.remote.local_strm_path(remote_file_path, extension);
        self.remote
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
        file_name_replace_from: &str,
        file_name_replace_to: &str,
    ) -> AppResult<bool> {
        let file_name = raw_file
            .name
            .replace(file_name_replace_from, file_name_replace_to);
        let res = self
            .transfer_raw_file(
                parent_dir_id,
                file_name.as_str(),
                raw_file.size,
                &raw_file.etag,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to transfer file {}, error: {}", file_name, e);
            })?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", file_name, id);

                // download subtitle file
                let local_file_path = self
                    .remote
                    .local_path_for_remote(format!("{}/{}", parent_path, file_name).as_str());
                self.remote
                    .download_library_file(id, local_file_path.as_str())
                    .await?;
                info!("Subtitle file {} downloaded", local_file_path);

                Ok(true)
            }
            None => {
                info!("File {} not saved in library", file_name);

                Ok(false)
            }
        }
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
                self.remote
                    .fast_upload_md5(parent_dir_id, file_name, etag, size)
                    .await?
            }
            Etag::Sha1(sha1) => {
                self.remote
                    .fast_upload_sha1(parent_dir_id, file_name, sha1, size)
                    .await?
            }
        })
    }
}

fn format_video_file_name(name_prefix: &str, file: &MediaFile) -> String {
    if file.video.name.starts_with(name_prefix) {
        file.video.name.to_owned()
    } else {
        let mut parts = vec![];
        if !file.metadata.resolution.is_empty() {
            parts.push(file.metadata.resolution.as_str());
        }
        if !file.metadata.frame_rate.is_empty() {
            parts.push(file.metadata.frame_rate.as_str());
        }
        if !file.metadata.quality.is_empty() {
            parts.push(file.metadata.quality.as_str());
        }
        if !file.metadata.hdr.is_empty() {
            parts.push(file.metadata.hdr.as_str());
        }
        if !file.metadata.video_codec.is_empty() {
            parts.push(file.metadata.video_codec.as_str());
        }
        if !file.metadata.audio_codec.is_empty() {
            parts.push(file.metadata.audio_codec.as_str());
        }

        let prefix = format!("{}{}", name_prefix, parts.join("."));
        if file.metadata.release_group.is_empty() {
            format!(
                "{}{}",
                prefix.trim_end_matches("."),
                file.metadata.extension
            )
        } else {
            format!(
                "{}-{}{}",
                prefix.trim_end_matches("."),
                file.metadata.release_group,
                file.metadata.extension
            )
        }
    }
}

fn need_overwrite_existing_files(existing_files: &[MediaFile], media_file: &MediaFile) -> bool {
    existing_files
        .iter()
        .all(|f| f.video.size < media_file.video.size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::media::Metadata;

    fn create_mock_media_file(size: u64) -> MediaFile {
        MediaFile {
            metadata: Box::new(Metadata::default()),
            video: RawFile {
                id: None,
                name: "test.mkv".to_string(),
                etag: "etag".into(),
                size,
                path: "/path".to_string(),
            },
            subtitles: Vec::new(),
        }
    }

    fn create_media_file_with_metadata(name: &str, metadata: Metadata) -> MediaFile {
        MediaFile {
            metadata: Box::new(metadata),
            video: RawFile {
                id: None,
                name: name.to_string(),
                etag: "etag".into(),
                size: 1000,
                path: "/path".to_string(),
            },
            subtitles: Vec::new(),
        }
    }

    #[test]
    fn test_format_video_file_name_already_has_prefix() {
        // If the file name already starts with the prefix, it should be returned as-is
        let prefix = "The Matrix.1999.";
        let file = create_media_file_with_metadata(
            "The Matrix.1999.BluRay.1080p.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "1080p".to_string(),
                quality: "BluRay".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "The Matrix.1999.BluRay.1080p.mkv");
    }

    #[test]
    fn test_format_video_file_name_with_all_metadata() {
        let prefix = "Breaking Bad.2008.S01E01.";
        let file = create_media_file_with_metadata(
            "original_name.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                frame_rate: "60fps".to_string(),
                quality: "WEB-DL".to_string(),
                hdr: "HDR10".to_string(),
                video_codec: "H265".to_string(),
                audio_codec: "DTS".to_string(),
                release_group: "RARBG".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(
            result,
            "Breaking Bad.2008.S01E01.2160p.60fps.WEB-DL.HDR10.H265.DTS-RARBG.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_no_release_group() {
        let prefix = "Inception.2010.";
        let file = create_media_file_with_metadata(
            "original.mp4",
            Metadata {
                extension: ".mp4".to_string(),
                resolution: "1080p".to_string(),
                frame_rate: "24fps".to_string(),
                quality: "BluRay".to_string(),
                video_codec: "H264".to_string(),
                audio_codec: "AAC".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Inception.2010.1080p.24fps.BluRay.H264.AAC.mp4");
    }

    #[test]
    fn test_format_video_file_name_minimal_metadata() {
        let prefix = "Movie.2020.";
        let file = create_media_file_with_metadata(
            "file.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "720p".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Movie.2020.720p.mkv");
    }

    #[test]
    fn test_format_video_file_name_no_metadata() {
        let prefix = "Show.2021.";
        let file = create_media_file_with_metadata(
            "video.avi",
            Metadata {
                extension: ".avi".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Show.2021.avi");
    }

    #[test]
    fn test_format_video_file_name_hdr_variants() {
        // Test with HDR10+
        let prefix = "HDR Movie.2022.";
        let file = create_media_file_with_metadata(
            "original.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                quality: "WEB-DL".to_string(),
                hdr: "HDR10+".to_string(),
                video_codec: "H265".to_string(),
                release_group: "NTb".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "HDR Movie.2022.2160p.WEB-DL.HDR10+.H265-NTb.mkv");

        // Test with Dolby Vision
        let file_dv = create_media_file_with_metadata(
            "original.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "2160p".to_string(),
                quality: "BluRay".to_string(),
                hdr: "DV".to_string(),
                video_codec: "H265".to_string(),
                audio_codec: "Atmos".to_string(),
                release_group: "GROUP".to_string(),
                ..Default::default()
            },
        );
        let result_dv = format_video_file_name(prefix, &file_dv);
        assert_eq!(
            result_dv,
            "HDR Movie.2022.2160p.BluRay.DV.H265.Atmos-GROUP.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_special_characters_in_prefix() {
        let prefix = "Star Wars: Episode IV.1977.";
        let file = create_media_file_with_metadata(
            "file.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                resolution: "1080p".to_string(),
                quality: "BluRay".to_string(),
                video_codec: "H264".to_string(),
                release_group: "YTS".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(
            result,
            "Star Wars: Episode IV.1977.1080p.BluRay.H264-YTS.mkv"
        );
    }

    #[test]
    fn test_format_video_file_name_partial_metadata() {
        // Test with only some metadata fields populated
        let prefix = "Series.2023.S02E05.";
        let file = create_media_file_with_metadata(
            "ep.mkv",
            Metadata {
                extension: ".mkv".to_string(),
                frame_rate: "30fps".to_string(),
                video_codec: "H264".to_string(),
                release_group: "AMZN".to_string(),
                ..Default::default()
            },
        );
        let result = format_video_file_name(prefix, &file);
        assert_eq!(result, "Series.2023.S02E05.30fps.H264-AMZN.mkv");
    }

    #[test]
    fn test_format_video_file_name_different_extensions() {
        let prefix = "Video.2024.";

        // Test .mp4
        let file_mp4 = create_media_file_with_metadata(
            "file.mp4",
            Metadata {
                extension: ".mp4".to_string(),
                resolution: "1080p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_mp4),
            "Video.2024.1080p.mp4"
        );

        // Test .avi
        let file_avi = create_media_file_with_metadata(
            "file.avi",
            Metadata {
                extension: ".avi".to_string(),
                resolution: "720p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_avi),
            "Video.2024.720p.avi"
        );

        // Test .webm
        let file_webm = create_media_file_with_metadata(
            "file.webm",
            Metadata {
                extension: ".webm".to_string(),
                resolution: "480p".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            format_video_file_name(prefix, &file_webm),
            "Video.2024.480p.webm"
        );
    }

    #[test]
    fn test_need_overwrite_existing_files() {
        // Case 1: New file is larger than all existing files
        let existing_files_1 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_1 = create_mock_media_file(300);
        assert!(need_overwrite_existing_files(
            &existing_files_1,
            &new_file_1
        ));

        // Case 2: New file is smaller than an existing file
        let existing_files_2 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_2 = create_mock_media_file(50);
        assert!(!need_overwrite_existing_files(
            &existing_files_2,
            &new_file_2
        ));

        // Case 3: New file is the same size as an existing file
        let existing_files_3 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_3 = create_mock_media_file(200);
        assert!(!need_overwrite_existing_files(
            &existing_files_3,
            &new_file_3
        ));

        // Case 4: No existing files
        let existing_files_4 = vec![];
        let new_file_4 = create_mock_media_file(100);
        assert!(need_overwrite_existing_files(
            &existing_files_4,
            &new_file_4
        ));
    }
}
