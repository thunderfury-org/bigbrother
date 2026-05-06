use std::path::Path;

use crate::domain::import::{
    Pan189File,
    inner::{MediaFile, RawFile},
    share_collect::{
        collect_pan115_directory_entries, collect_pan123_directory_entries,
        collect_pan189_directory_entries,
    },
    share_walk::ShareTraversal,
    source::{ResourceJson, parse_files_from_json},
};

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
        let raw_files = self
            .raw_files_from_pan123_share(share_key, share_password)
            .await?;
        Ok(self.metadata_lookup_mut().build_media_files(raw_files))
    }

    pub(crate) async fn raw_files_from_pan123_share(
        &mut self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<RawFile>> {
        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_source()
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }

    pub(super) async fn list_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let raw_files = self.raw_files_from_pan189_share(share_code).await?;
        Ok(self.metadata_lookup_mut().build_media_files(raw_files))
    }

    pub(crate) async fn raw_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<RawFile>> {
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
                    .map(|file| CasFileCandidate { file }),
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

        if only_contains_cas_files {
            let mut cas_raw_files = Vec::new();
            for candidate in &cas_files {
                let json = self
                    .share_source()
                    .download_pan189_share_file(share_info.share_id, &candidate.file)
                    .await
                    .map_err(|e| {
                        AppError::InvalidParameter(format!(
                            "检测到天翼 CAS 秒传分享，需要使用自己的天翼云盘账号登录后读取 CAS 内容；请确认 pan189.username / pan189.password 可正常登录且账号未触发设备校验后重试: {e}"
                        ))
                    })?;
                let resource = parse_files_from_json(json)?;
                cas_raw_files.extend(resource_to_raw_files(
                    &resource,
                    &cas_context_path(&candidate.file.name),
                ));
            }
            Ok(cas_raw_files)
        } else {
            Ok(raw_files)
        }
    }

    pub(super) async fn list_files_from_pan115_share(
        &mut self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let raw_files = self
            .raw_files_from_pan115_share(share_code, receive_code)
            .await?;
        Ok(self.metadata_lookup_mut().build_media_files(raw_files))
    }

    pub(crate) async fn raw_files_from_pan115_share(
        &mut self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<RawFile>> {
        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_source()
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }
}

#[derive(Clone)]
struct CasFileCandidate {
    file: Pan189File,
}

fn is_cas_file(name: &str) -> bool {
    name.to_lowercase().ends_with(".cas")
}

fn cas_context_path(name: &str) -> String {
    if name.to_lowercase().ends_with(".cas") {
        name[..name.len() - ".cas".len()].to_owned()
    } else {
        name.to_owned()
    }
}

fn resource_to_raw_files(
    resource: &ResourceJson,
    fallback_common_path: &str,
) -> Vec<crate::domain::import::inner::RawFile> {
    let mut raw_files = Vec::new();
    let common_path = if resource.common_path.trim().is_empty() {
        fallback_common_path
    } else {
        resource.common_path.as_str()
    };

    for file in &resource.files {
        let full_path = format!("{}/{}", common_path, &file.path);
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
