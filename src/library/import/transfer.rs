use std::{collections::HashMap, os, path::Path};

use tracing::info;

use crate::{
    client::tmdb::{MovieDetail, TvDetail},
    error::AppResult,
};

use super::{ImportSummary, Importer, Media, MediaFile};

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

    async fn transfer_movie(&mut self, detail: &MovieDetail, files: &[&MediaFile]) -> AppResult<()> {
        // list existing movies in library
        Ok(())
    }

    async fn transfer_tv(
        &mut self,
        detail: &TvDetail,
        files: &HashMap<u32, HashMap<u32, Vec<&MediaFile>>>,
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
                if existing_episode_files.contains_key(episode_number) {
                    // episode file already exists in library
                    self.summary.skipped += files.len() as u32;
                    continue;
                }
                // save episode file
                let video_file = files
                    .iter()
                    .filter(|f| f.metadata.is_video())
                    .max_by(|a, b| a.raw.size.cmp(&b.raw.size));
                match video_file {
                    None => {
                        self.summary.skipped += files.len() as u32;
                    }
                    Some(video) => {
                        let name_prefix = format!(
                            "{}.{}.S{:02}E{:02}.",
                            detail.name,
                            self.get_year_from_date(detail.first_air_date.as_str()),
                            season_number,
                            episode_number
                        );
                        self.transfer_video_files(&season_full_path, season_dir_id, name_prefix.as_str(), video, files)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn transfer_video_files(
        &mut self,
        full_path: &str,
        dir_id: i64,
        name_prefix: &str,
        video_file: &MediaFile,
        all_files: &[&MediaFile],
    ) -> AppResult<()> {
        let video_file_name = self.format_video_file_name(name_prefix, video_file);
        let res = self
            .state
            .pan123
            .fast_upload(
                dir_id,
                video_file_name.as_str(),
                video_file.raw.etag.as_str(),
                video_file.raw.size,
            )
            .await?;
        match res {
            Some(id) => {
                info!("File {} saved in library, file id: {}", video_file_name, id);
                self.summary.success += 1;
                self.summary.total_size += video_file.raw.size;

                // create strm file
                self.create_strm_file(
                    format!("{}/{}", full_path, video_file_name,).as_str(),
                    video_file.metadata.extension.as_str(),
                    id,
                )
                .await?;

                // todo save subtitle files
                let subtitle_files = all_files
                    .iter()
                    .filter(|f| f.metadata.is_subtitle())
                    .collect::<Vec<_>>();
                if !subtitle_files.is_empty() {
                    self.summary.success += subtitle_files.len() as u32;
                    self.summary.total_size += subtitle_files.iter().map(|f| f.raw.size).sum::<u64>();
                }
            }
            None => {
                self.summary.failed += 1;
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

    fn format_video_file_name(&self, name_prefix: &str, file: &MediaFile) -> String {
        if file.raw.name.starts_with(name_prefix) {
            file.raw.name.to_owned()
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
}
