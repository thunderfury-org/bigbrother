use crate::application::import_ports::ShareSource;
use crate::domain::import::{
    Pan189File, ShareUrl,
    share_collect::{
        collect_pan115_directory_entries, collect_pan123_directory_entries,
        collect_pan189_directory_entries, collect_quark_directory_entries,
    },
    share_walk::ShareTraversal,
    source::{parse_fslink_to_raw_files, parse_json_to_raw_files, parse_json_to_raw_files_with_context},
};
use crate::domain::share::RawFile;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct ShareCrawler<S> {
    share_source: S,
}

impl<S: ShareSource> ShareCrawler<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<RawFile>> {
        match url {
            ShareUrl::Pan123(url) => {
                let (share_key, share_password) = parse_pan123_share_parts(url);
                self.raw_files_from_pan123_share(share_key.as_str(), share_password.as_str())
                    .await
            }
            ShareUrl::Pan189(url) => {
                let share_code = parse_pan189_share_code(url);
                if share_code.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share code from URL: {url}"
                    )));
                }
                self.raw_files_from_pan189_share(&share_code).await
            }
            ShareUrl::Pan115(url) => {
                let (share_code, receive_code) = parse_pan115_share_parts(url);
                if share_code.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share code from URL: {url}"
                    )));
                }
                self.raw_files_from_pan115_share(&share_code, &receive_code)
                    .await
            }
            ShareUrl::Quark(url) => {
                let (share_id, password) = parse_quark_share_parts(url);
                if share_id.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share id from URL: {url}"
                    )));
                }
                self.raw_files_from_quark_share(&share_id, &password).await
            }
        }
    }

    #[allow(dead_code)]
    pub fn raw_files_from_fslink(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
        parse_fslink_to_raw_files(fslink)
    }

    #[allow(dead_code)]
    pub fn raw_files_from_json(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
        parse_json_to_raw_files(json)
    }

    async fn raw_files_from_pan123_share(
        &self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<RawFile>> {
        let mut traversal = ShareTraversal::new((0, String::new()));

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self
                .share_source
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }

    async fn raw_files_from_pan189_share(&self, share_code: &str) -> AppResult<Vec<RawFile>> {
        let share_info = self.share_source.get_pan189_share_info(share_code).await?;

        let mut traversal =
            ShareTraversal::new((share_info.file_id, share_info.file_name.to_owned()));
        let mut cas_files = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source
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
                    .share_source
                    .download_pan189_share_file(share_info.share_id, &candidate.file)
                    .await
                    .map_err(|e| {
                        AppError::InvalidParameter(format!(
                            "检测到天翼 CAS 秒传分享，需要使用自己的天翼云盘账号登录后读取 CAS 内容；请确认 pan189.username / pan189.password 可正常登录且账号未触发设备校验后重试: {e}"
                        ))
                    })?;
                cas_raw_files.extend(parse_json_to_raw_files_with_context(
                    json,
                    &cas_context_path(&candidate.file.name),
                )?);
            }
            Ok(cas_raw_files)
        } else {
            Ok(raw_files)
        }
    }

    async fn raw_files_from_pan115_share(
        &self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<RawFile>> {
        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_source
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }

    async fn raw_files_from_quark_share(
        &self,
        share_id: &str,
        password: &str,
    ) -> AppResult<Vec<RawFile>> {
        let share_info = self
            .share_source
            .get_quark_share_info(share_id, password)
            .await?;

        // Phase 1: BFS traversal, collect file info
        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));
        let mut file_infos: Vec<(String, String, String, u64, String)> = Vec::new();

        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let (folders, files) = self
                .share_source
                .list_quark_share_files(share_id, password, &share_info.stoken, &parent_id)
                .await?;

            for file in &files {
                file_infos.push((
                    file.fid.clone(),
                    file.share_fid_token.clone(),
                    file.name.clone(),
                    file.size,
                    parent_path.clone(),
                ));
            }

            traversal.extend(collect_quark_directory_entries(
                &folders,
                &files,
                &parent_path,
            ));
        }

        // Phase 2: Batch fetch md5
        let md5_pairs: Vec<(String, String)> = file_infos
            .iter()
            .map(|(fid, token, _, _, _)| (fid.clone(), token.clone()))
            .collect();
        let md5_map = self
            .share_source
            .batch_get_quark_file_md5s(share_id, password, &share_info.stoken, &md5_pairs)
            .await?;

        // Phase 3: Build RawFiles with md5
        let raw_files: Vec<RawFile> = file_infos
            .into_iter()
            .map(|(fid, _token, name, size, path)| {
                let md5 = md5_map.get(&fid).cloned().unwrap_or_default();
                RawFile {
                    id: None,
                    name,
                    etag: md5.as_str().into(),
                    size,
                    path,
                }
            })
            .collect();

        Ok(raw_files)
    }
}

// --- Helpers ---

use crate::domain::import::source::{
    parse_pan115_share_parts, parse_pan123_share_parts, parse_pan189_share_code,
    parse_quark_share_parts,
};

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
