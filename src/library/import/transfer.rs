use std::{
    collections::{BTreeMap, HashMap},
    io,
    path::Path,
};

use tracing::info;

use super::{
    ImportSummary, Importer,
    inner::{Media, MediaFile, RawFile},
};
use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::{AppError, AppResult},
};

impl Importer {
    pub(super) async fn transfer_media_files(&mut self, media_files: &[MediaFile]) -> AppResult<ImportSummary> {
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

        self.summary.cost = self.start_time.elapsed();
        Ok(self.summary.clone())
    }

    async fn transfer_movie(&mut self, detail: &MovieDetail, media_files: &[&MediaFile]) -> AppResult<()> {
        let movie_path = self.get_movie_path_in_library(detail);
        let movie_dir_id = self.get_or_create_dir_in_library(movie_path.as_str()).await?;
        let existing_files = self.list_movie_files_in_library(movie_dir_id).await?;
        let media_file = media_files
            .iter()
            .max_by(|a, b| a.video.size.cmp(&b.video.size))
            .ok_or_else(|| AppError::Error(format!("no video file found when transfer movie {}", detail.title)))?;

        if !existing_files.is_empty() {
            // existing files found, check if need overwrite
            if self.need_overwrite_existing_files(&existing_files, media_file) {
                // existing file size is smaller than new file, need overwrite
                // delete existing files
                self.delete_files_in_library(&existing_files).await?;
                self.delete_files_in_local(&movie_path, &existing_files).await?;
            } else {
                // do not need overwrite existing files, skip
                self.summary.skipped += media_files.iter().map(|f| f.file_count()).sum::<usize>();
                return Ok(());
            }
        }

        let name_prefix = format!(
            "{}.{}.",
            detail.title,
            self.get_year_from_date(detail.release_date.as_str()),
        );
        self.transfer_video_files(&movie_path, movie_dir_id, name_prefix.as_str(), media_file)
            .await?;
        Ok(())
    }

    async fn transfer_tv(
        &mut self,
        detail: &TvDetail,
        files: &BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>,
    ) -> AppResult<()> {
        let tv_path = self.get_tv_path_in_library(detail);
        let tv_dir_id = self.get_or_create_dir_in_library(tv_path.as_str()).await?;
        let season_dir_ids = self.state.pan123.list_dir_ids(tv_dir_id).await?;

        for (season_number, season_files) in files {
            let season_dir = format!("Season {:02}", season_number);
            let season_full_path = format!("{}/{}", tv_path, season_dir);
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

            for (episode_number, files) in season_files {
                let media_file = files
                    .iter()
                    .max_by(|a, b| a.video.size.cmp(&b.video.size))
                    .ok_or_else(|| {
                        AppError::Error(format!(
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
                        self.delete_files_in_local(&season_full_path, existing_files).await?;
                    } else {
                        // existing file size is larger than new file, skip
                        self.summary.skipped += files.iter().map(|f| f.file_count()).sum::<usize>();
                        continue;
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
                self.transfer_video_files(&season_full_path, season_dir_id, name_prefix.as_str(), media_file)
                    .await?;
            }
        }

        Ok(())
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
                return Err(AppError::Error(format!("Failed to delete local file, error: {}", e)));
            }

            for s in &f.subtitles {
                let local_file_path = format!("{}/{}", local_parent_path, s.name);
                info!("Deleting local file {}", local_file_path);
                if let Err(e) = tokio::fs::remove_file(local_file_path.as_str()).await
                    && e.kind() != io::ErrorKind::NotFound
                {
                    return Err(AppError::Error(format!("Failed to delete local file, error: {}", e)));
                }
            }
        }

        Ok(())
    }

    async fn transfer_video_files(
        &mut self,
        parent_path: &str,
        parent_dir_id: i64,
        name_prefix: &str,
        media_file: &MediaFile,
    ) -> AppResult<()> {
        let video_file_name = self.format_video_file_name(name_prefix, media_file);
        let res = self
            .state
            .pan123
            .fast_upload(
                parent_dir_id,
                video_file_name.as_str(),
                media_file.video.etag.as_str(),
                media_file.video.size,
            )
            .await?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", video_file_name, id);
                self.summary.success += 1;
                self.summary.total_size += media_file.video.size;

                // create strm file
                self.create_strm_file(
                    format!("{}/{}", parent_path, video_file_name,).as_str(),
                    media_file.metadata.extension.as_str(),
                    id,
                )
                .await?;

                // save subtitle files
                let subtitle_file_name_replace_from = media_file
                    .video
                    .name
                    .trim_end_matches(media_file.metadata.extension.as_str());
                let subtitle_file_name_replace_to =
                    video_file_name.trim_end_matches(media_file.metadata.extension.as_str());
                for subtitle in &media_file.subtitles {
                    self.transfer_subtitle_file(
                        parent_path,
                        parent_dir_id,
                        subtitle,
                        subtitle_file_name_replace_from,
                        subtitle_file_name_replace_to,
                    )
                    .await?;
                }
            }
            None => {
                self.summary.failed += media_file.file_count();
            }
        }
        Ok(())
    }

    async fn create_strm_file(&self, remote_file_path: &str, extension: &str, file_id: i64) -> AppResult<()> {
        let strm_file_content = format!(
            "{}/d{}?file_id={}",
            self.state.config.get_media_server_config().get_advertise_base_url(),
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
        &mut self,
        parent_path: &str,
        parent_dir_id: i64,
        raw_file: &RawFile,
        file_name_replace_from: &str,
        file_name_replace_to: &str,
    ) -> AppResult<()> {
        let file_name = raw_file.name.replace(file_name_replace_from, file_name_replace_to);
        let res = self
            .state
            .pan123
            .fast_upload(parent_dir_id, file_name.as_str(), raw_file.etag.as_str(), raw_file.size)
            .await?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", file_name, id);
                self.summary.success += 1;
                self.summary.total_size += raw_file.size;

                // download subtitle file
                let local_file_path = format!("{}/{}", parent_path, file_name).replace(
                    self.state.config.get_library_config().remote_path.as_str(),
                    self.state.config.get_library_config().local_path.as_str(),
                );
                self.state.pan123.download_file(id, local_file_path.as_str()).await?;
                info!("Subtitle file {} downloaded", local_file_path);
            }
            None => {
                self.summary.failed += 1;
            }
        }
        Ok(())
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
