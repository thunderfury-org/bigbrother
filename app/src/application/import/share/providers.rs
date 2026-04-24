use std::{collections::HashSet, path::Path};

use crate::domain::import::{
    Pan189File,
    inner::{Etag, Media, MediaFile, RawFile},
    paths::{get_movie_path_in_library, get_tv_path_in_library},
    share_collect::{
        collect_pan115_directory_entries, collect_pan123_directory_entries,
        collect_pan189_directory_entries,
    },
    share_walk::ShareTraversal,
    source::{ResourceJson, parse_files_from_json},
};
use tracing::info;

use super::ShareImportUseCase;
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::error::{AppError, AppResult};

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
        let mut cas_files = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source()
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;
            cas_files.extend(
                files
                    .iter()
                    .filter(|file| is_cas_file(&file.name))
                    .cloned()
                    .map(|file| CasFileCandidate {
                        file,
                        parent_path: parent_path.to_owned(),
                    }),
            );

            traversal.extend(collect_pan189_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        let raw_files = traversal.into_raw_files();
        let only_contains_cas_files =
            !raw_files.is_empty() && raw_files.iter().all(|file| is_cas_file(&file.name));

        let media_files = if only_contains_cas_files {
            let mut cas_raw_files = Vec::new();
            let cas_files = self.filter_existing_pan189_cas_files(cas_files).await?;
            for candidate in &cas_files {
                let json = self
                    .share_source()
                    .download_pan189_share_file(share_info.share_id, &candidate.file)
                    .await
                    .map_err(|e| {
                        AppError::InvalidParameter(format!(
                            "检测到天翼 CAS 秒传分享，但需要配置自己的天翼云盘网页登录 Cookie 才能读取 CAS 内容；请配置 pan189.cookie 后重试: {e}"
                        ))
                    })?;
                let resource = parse_files_from_json(json)?;
                cas_raw_files.extend(resource_to_raw_files(&resource));
            }
            self.metadata_lookup_mut().build_media_files(cas_raw_files)
        } else {
            self.metadata_lookup_mut().build_media_files(raw_files)
        };

        Ok(media_files)
    }

    async fn filter_existing_pan189_cas_files(
        &mut self,
        cas_files: Vec<CasFileCandidate>,
    ) -> AppResult<Vec<CasFileCandidate>> {
        let preview_raw_files = cas_files
            .iter()
            .filter_map(|candidate| candidate.preview_raw_file())
            .collect::<Vec<_>>();
        let preview_files = self
            .metadata_lookup_mut()
            .build_media_files(preview_raw_files);
        if preview_files.is_empty() {
            return Ok(cas_files);
        }

        let media_groups = self
            .transfer_mut()
            .workflow_mut()
            .group_media_files(&preview_files)
            .await?;
        let mut skipped = HashSet::new();
        for media in media_groups {
            match media {
                Media::Movie { detail, files } => {
                    let remote_path = self.transfer_mut().workflow().local().remote_library_path();
                    let movie_path = get_movie_path_in_library(remote_path, &detail);
                    let Some(movie_dir_id) = self
                        .transfer_mut()
                        .workflow()
                        .library_gateway()
                        .get_library_dir_id_by_path(&movie_path)
                        .await?
                    else {
                        continue;
                    };
                    let existing_files = self
                        .transfer_mut()
                        .workflow_mut()
                        .list_movie_files_in_library(movie_dir_id)
                        .await?;
                    if existing_files.is_empty() {
                        continue;
                    }
                    skipped.extend(
                        files
                            .iter()
                            .map(|file| CasPreviewKey::from_media_file(file)),
                    );
                }
                Media::Tv { detail, files } => {
                    let remote_path = self.transfer_mut().workflow().local().remote_library_path();
                    let tv_path = get_tv_path_in_library(remote_path, &detail);
                    let Some(tv_dir_id) = self
                        .transfer_mut()
                        .workflow()
                        .library_gateway()
                        .get_library_dir_id_by_path(&tv_path)
                        .await?
                    else {
                        continue;
                    };
                    let season_dir_ids = self
                        .transfer_mut()
                        .workflow()
                        .library_gateway()
                        .list_library_dir_ids(tv_dir_id)
                        .await?;
                    for (season_number, season_files) in files {
                        let season_dir = format!("Season {:02}", season_number);
                        let Some(season_dir_id) = season_dir_ids.get(&season_dir).copied() else {
                            continue;
                        };
                        let existing_episode_files = self
                            .transfer_mut()
                            .workflow_mut()
                            .list_episode_files_in_library(season_dir_id)
                            .await?;
                        for (episode_number, episode_files) in season_files {
                            if existing_episode_files.contains_key(&episode_number) {
                                skipped.extend(
                                    episode_files
                                        .iter()
                                        .map(|file| CasPreviewKey::from_media_file(file)),
                                );
                            }
                        }
                    }
                }
            }
        }

        if skipped.is_empty() {
            return Ok(cas_files);
        }

        let original_len = cas_files.len();
        let filtered = cas_files
            .into_iter()
            .filter(|candidate| {
                candidate
                    .preview_key()
                    .is_none_or(|key| !skipped.contains(&key))
            })
            .collect::<Vec<_>>();
        info!(
            "Skipped {} pan189 CAS files because matching media already exists in library",
            original_len - filtered.len()
        );
        Ok(filtered)
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

#[derive(Clone)]
struct CasFileCandidate {
    file: Pan189File,
    parent_path: String,
}

impl CasFileCandidate {
    fn preview_name(&self) -> Option<String> {
        self.file.name.strip_suffix(".cas").map(ToOwned::to_owned)
    }

    fn preview_key(&self) -> Option<CasPreviewKey> {
        self.preview_name().map(|name| CasPreviewKey {
            path: self.parent_path.to_owned(),
            name,
        })
    }

    fn preview_raw_file(&self) -> Option<RawFile> {
        self.preview_name().map(|name| RawFile {
            id: None,
            name,
            etag: Etag::Md5(String::new()),
            size: 0,
            path: self.parent_path.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CasPreviewKey {
    path: String,
    name: String,
}

impl CasPreviewKey {
    fn from_media_file(file: &MediaFile) -> Self {
        Self {
            path: file.video.path.to_owned(),
            name: file.video.name.to_owned(),
        }
    }
}

fn is_cas_file(name: &str) -> bool {
    name.to_lowercase().ends_with(".cas")
}

fn resource_to_raw_files(resource: &ResourceJson) -> Vec<crate::domain::import::inner::RawFile> {
    let mut raw_files = Vec::new();

    for file in &resource.files {
        let full_path = format!("{}/{}", &resource.common_path, &file.path);
        let path = Path::new(full_path.as_str());
        let parent_path = path
            .parent()
            .map(|p| p.to_str().unwrap_or_default())
            .unwrap_or_default();
        let name = path
            .file_name()
            .map(|p| p.to_str().unwrap_or_default())
            .unwrap_or_default();

        raw_files.push(crate::domain::import::inner::RawFile {
            id: None,
            name: name.to_owned(),
            etag: file.etag.as_str().into(),
            size: file.size,
            path: parent_path.to_owned(),
        });
    }

    raw_files
}
