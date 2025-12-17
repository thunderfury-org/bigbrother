use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
    path::Path,
};

use tracing::{error, info};

use super::{
    ImportedMedia, Importer,
    inner::{Media, MediaFile, RawFile},
};
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::{AppError, AppResult},
    log_time,
};

impl Importer {
    pub(super) async fn transfer_media_files(&mut self, media_files: &[MediaFile]) -> AppResult<Vec<ImportedMedia>> {
        let results = Vec::with_capacity(media_files.len());

        let medias = self.group_media_files(media_files).await?;
        for media in &medias {
            match media {
                Media::Movie { detail, files } => {
                    self.transfer_movie(detail, files).await?;
                }
                Media::Tv { detail, files } => {
                    self.transfer_tv(detail, files).await?;
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
        log_time!(format!("transfer movie {}", self.get_movie_base_name(detail)));
        let start_time = std::time::Instant::now();

        let movie_path = self.get_movie_path_in_library(detail);
        let movie_dir_id = self.get_or_create_dir_in_library(movie_path.as_str()).await?;
        let existing_files = self.list_movie_files_in_library(movie_dir_id).await?;
        let media_file = media_files
            .iter()
            .max_by(|a, b| a.video.size.cmp(&b.video.size))
            .ok_or_else(|| AppError::NotFound(format!("no video file found when transfer movie {}", detail.title)))?;

        if !existing_files.is_empty() {
            // existing files found, check if need overwrite
            if self.need_overwrite_existing_files(&existing_files, media_file) {
                // existing file size is smaller than new file, need overwrite
                // delete existing files
                self.delete_files_in_library(&existing_files).await?;
                self.delete_files_in_local(&movie_path, &existing_files).await?;
            } else {
                // do not need overwrite existing files, skip
                return Ok(None);
            }
        }

        let name_prefix = format!(
            "{}.{}.",
            detail.title,
            self.get_year_from_date(detail.release_date.as_str()),
        );
        let success = self
            .transfer_media_file(&movie_path, movie_dir_id, name_prefix.as_str(), media_file)
            .await?;
        Ok(Some(ImportedMedia::Movie {
            title: detail.title.to_owned(),
            year: self.get_year_from_date(detail.release_date.as_str()).to_owned(),
            size: media_file.video.size,
            cost: start_time.elapsed(),
            has_failed: !success,
        }))
    }

    async fn transfer_tv(
        &mut self,
        detail: &TvDetail,
        files: &BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>,
    ) -> AppResult<Vec<ImportedMedia>> {
        log_time!(format!("transfer tv {}", self.get_tv_base_name(detail)));

        let tv_path = self.get_tv_path_in_library(detail);
        let tv_dir_id = self.get_or_create_dir_in_library(tv_path.as_str()).await?;
        let season_dir_ids = self.state.pan123.list_dir_ids(tv_dir_id).await?;

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
            self.get_tv_base_name(detail),
            season_number
        ));
        let start_time = std::time::Instant::now();

        let season_dir = format!("Season {:02}", season_number);
        let (season_dir_id, existing_episode_files) = match season_dir_ids.get(&season_dir) {
            Some(id) => (*id, self.list_episode_files_in_library(*id).await?),
            None => {
                // create season folder if not exists
                let id = self.state.pan123.mkdir(tv_dir_id, season_dir.as_str()).await?;
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
                .transfer_episode(
                    detail,
                    season_number,
                    episode_number,
                    files,
                    &season_full_path,
                    season_dir_id,
                    &existing_episode_files,
                )
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
            year: self.get_year_from_date(detail.first_air_date.as_str()).to_owned(),
            season: *season_number,
            missing_episodes: self.get_missing_episodes(max_episode_number, &episodes, &existing_episode_files),
            episodes,
            max_episode_number,
            total_size,
            number_of_episodes: self.get_number_of_episodes_in_season(detail, season_number),
            cost: start_time.elapsed(),
            has_failed,
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

    fn get_max_episode_number(&self, episodes: &[u32], existing_episode_files: &HashMap<u32, Vec<MediaFile>>) -> u32 {
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
        detail: &TvDetail,
        season_number: &u32,
        episode_number: &u32,
        files: &[&MediaFile],
        season_full_path: &str,
        season_dir_id: i64,
        existing_episode_files: &HashMap<u32, Vec<MediaFile>>,
    ) -> AppResult<Option<(bool, u64)>> {
        let media_file = files
            .iter()
            .max_by(|a, b| a.video.size.cmp(&b.video.size))
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "no video file found when transfer tv series {} season {} episode {}",
                    detail.name, season_number, episode_number
                ))
            })?;
        if let Some(existing_files) = existing_episode_files.get(episode_number)
            && !existing_files.is_empty()
        {
            // episode file already exists in library
            if self.need_overwrite_existing_files(existing_files, media_file) {
                // existing file size is smaller than new file, need overwrite
                // delete existing files
                self.delete_files_in_library(existing_files).await?;
                self.delete_files_in_local(season_full_path, existing_files).await?;
            } else {
                // existing file size is larger than new file, skip
                return Ok(None);
            }
        }

        // save episode file
        let name_prefix = format!(
            "{}.{}.S{:02}E{:02}.",
            detail.name,
            self.get_year_from_date(detail.first_air_date.as_str()),
            season_number,
            episode_number
        );
        let success = self
            .transfer_media_file(season_full_path, season_dir_id, name_prefix.as_str(), media_file)
            .await?;
        Ok(Some((success, media_file.video.size)))
    }

    async fn delete_files_in_library(&self, files: &[MediaFile]) -> AppResult<()> {
        let mut file_ids = Vec::new();
        for f in files {
            info!("Deleting file {} from library, file id: {:?}", f.video.name, f.video.id);
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

        self.state.pan123.trash_files(file_ids.as_slice()).await?;
        Ok(())
    }

    async fn delete_files_in_local(&self, remote_parent_path: &str, files: &[MediaFile]) -> AppResult<()> {
        let local_parent_path = remote_parent_path.replace(
            self.state.config.get_library_config().remote_path.as_str(),
            self.state.config.get_library_config().local_path.as_str(),
        );

        for f in files {
            let local_file_path = format!(
                "{}/{}.strm",
                local_parent_path,
                f.video.name.trim_end_matches(f.metadata.extension.as_str())
            );
            info!("Deleting local file {}", local_file_path);
            if let Err(e) = tokio::fs::remove_file(local_file_path.as_str()).await
                && e.kind() != io::ErrorKind::NotFound
            {
                return Err(AppError::Internal(format!("Failed to delete local file, error: {}", e)));
            }

            for s in &f.subtitles {
                let local_file_path = format!("{}/{}", local_parent_path, s.name);
                info!("Deleting local file {}", local_file_path);
                if let Err(e) = tokio::fs::remove_file(local_file_path.as_str()).await
                    && e.kind() != io::ErrorKind::NotFound
                {
                    return Err(AppError::Internal(format!("Failed to delete local file, error: {}", e)));
                }
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
    ) -> AppResult<bool> {
        let video_file_name = self.format_video_file_name(name_prefix, media_file);

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
                    return Ok(false);
                }
            }
        }

        self.transfer_video_file(parent_path, parent_dir_id, video_file_name.as_str(), media_file)
            .await
    }

    async fn transfer_video_file(
        &self,
        parent_path: &str,
        parent_dir_id: i64,
        video_file_name: &str,
        media_file: &MediaFile,
    ) -> AppResult<bool> {
        let res = self
            .state
            .pan123
            .fast_upload(
                parent_dir_id,
                video_file_name,
                media_file.video.etag.as_str(),
                media_file.video.size,
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

                Ok(true)
            }
            None => {
                info!("File {} not saved in library", video_file_name);

                Ok(false)
            }
        }
    }

    async fn create_strm_file(&self, remote_file_path: &str, extension: &str, file_id: i64) -> AppResult<()> {
        let strm_file_content = format!(
            "{}{}?file_id={}",
            self.state.config.get_media_server_config().get_strm_download_url(),
            remote_file_path,
            file_id
        );

        let local_file_path = remote_file_path
            .replace(
                self.state.config.get_library_config().remote_path.as_str(),
                self.state.config.get_library_config().local_path.as_str(),
            )
            .trim_end_matches(extension)
            .to_owned()
            + ".strm";

        tokio::fs::create_dir_all(Path::new(&local_file_path).parent().unwrap()).await?;
        tokio::fs::write(local_file_path.as_str(), strm_file_content.as_bytes()).await?;
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
        let file_name = raw_file.name.replace(file_name_replace_from, file_name_replace_to);
        let res = self
            .state
            .pan123
            .fast_upload(parent_dir_id, file_name.as_str(), raw_file.etag.as_str(), raw_file.size)
            .await
            .inspect_err(|e| {
                error!("Failed to transfer file {}, error: {}", file_name, e);
            })?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", file_name, id);

                // download subtitle file
                let local_file_path = format!("{}/{}", parent_path, file_name).replace(
                    self.state.config.get_library_config().remote_path.as_str(),
                    self.state.config.get_library_config().local_path.as_str(),
                );
                self.state.pan123.download_file(id, local_file_path.as_str()).await?;
                info!("Subtitle file {} downloaded", local_file_path);

                Ok(true)
            }
            None => {
                info!("File {} not saved in library", file_name);

                Ok(false)
            }
        }
    }

    fn format_video_file_name(&self, name_prefix: &str, file: &MediaFile) -> String {
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
            if file.metadata.release_group.is_empty() {
                format!("{}{}{}", name_prefix, parts.join("."), file.metadata.extension)
            } else {
                format!(
                    "{}{}-{}{}",
                    name_prefix,
                    parts.join("."),
                    file.metadata.release_group,
                    file.metadata.extension
                )
            }
        }
    }

    fn need_overwrite_existing_files(&self, existing_files: &[MediaFile], media_file: &MediaFile) -> bool {
        existing_files.iter().all(|f| f.video.size < media_file.video.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Metadata;

    fn create_mock_media_file(size: u64) -> MediaFile {
        MediaFile {
            metadata: Box::new(Metadata::default()),
            video: RawFile {
                id: None,
                name: "test.mkv".to_string(),
                etag: "etag".to_string(),
                size,
                path: "/path".to_string(),
            },
            subtitles: Vec::new(),
        }
    }

    #[test]
    fn test_need_overwrite_existing_files() {
        let importer = Importer::default();

        // Case 1: New file is larger than all existing files
        let existing_files_1 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_1 = create_mock_media_file(300);
        assert!(importer.need_overwrite_existing_files(&existing_files_1, &new_file_1));

        // Case 2: New file is smaller than an existing file
        let existing_files_2 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_2 = create_mock_media_file(50);
        assert!(!importer.need_overwrite_existing_files(&existing_files_2, &new_file_2));

        // Case 3: New file is the same size as an existing file
        let existing_files_3 = vec![create_mock_media_file(100), create_mock_media_file(200)];
        let new_file_3 = create_mock_media_file(200);
        assert!(!importer.need_overwrite_existing_files(&existing_files_3, &new_file_3));

        // Case 4: No existing files
        let existing_files_4 = vec![];
        let new_file_4 = create_mock_media_file(100);
        assert!(importer.need_overwrite_existing_files(&existing_files_4, &new_file_4));
    }
}
