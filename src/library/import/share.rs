use reqwest::Url;

use super::{
    ImportSummary, Importer,
    inner::{MediaFile, RawFile},
};
use crate::error::{AppError, AppResult};

pub enum ShareUrl<'a> {
    Pan123(&'a Url),
    Pan189(&'a Url),
}

impl<'a> ShareUrl<'a> {
    pub fn from(url: &'a Url) -> Option<Self> {
        if url
            .host_str()
            .is_some_and(|h| h.starts_with("www.123") && h.ends_with(".com"))
            && url.path().starts_with("/s/")
        {
            Some(Self::Pan123(url))
        } else if url.host_str().is_some_and(|h| h == "cloud.189.cn")
            && (url.path().starts_with("/t/") || url.path() == "/web/share")
        {
            Some(Self::Pan189(url))
        } else {
            None
        }
    }

    pub fn get_url(&self) -> &Url {
        match self {
            Self::Pan123(url) => url,
            Self::Pan189(url) => url,
        }
    }
}

impl Importer {
    pub async fn import_from_share_url(&mut self, url: &ShareUrl<'_>) -> AppResult<ImportSummary> {
        match url {
            ShareUrl::Pan123(url) => self.import_pan123_share(url).await,
            ShareUrl::Pan189(url) => self.import_pan189_share(url).await,
        }
    }

    async fn import_pan123_share(&mut self, url: &Url) -> AppResult<ImportSummary> {
        let share_key = url
            .path_segments()
            .map(|mut s| s.next_back().unwrap_or_default())
            .unwrap_or_default();
        let share_password = url
            .query_pairs()
            .find(|(k, _)| k == "pwd")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();

        let media_files = self.list_files_from_pan123_share(share_key, &share_password).await?;
        self.transfer_media_files(&media_files).await
    }

    async fn import_pan189_share(&mut self, url: &Url) -> AppResult<ImportSummary> {
        let share_code = url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| {
                if url.path().starts_with("/t/") {
                    url.path_segments()
                        .map(|mut s| s.next_back().unwrap_or_default())
                        .unwrap_or_default()
                        .to_owned()
                } else {
                    String::new()
                }
            });
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self.list_files_from_pan189_share(&share_code).await?;
        self.transfer_media_files(&media_files).await
    }

    async fn list_files_from_pan123_share(
        &mut self,
        share_key: &str,
        share_password: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut all_files = Vec::new();
        let mut stack = vec![(0, String::new())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let files = self
                .state
                .pan123
                .list_share_files(share_key, share_password, parent_id)
                .await?;

            let mut media_files_in_dir = Vec::new();
            for file in &files {
                if file.is_dir() {
                    // Directory
                    stack.push((file.file_id, format!("{}/{}", parent_path, file.file_name)));
                } else {
                    // Regular file
                    self.summary.total += 1;
                    let metadata = self.parse_media_metadata(&file.file_name, &parent_path);
                    if metadata.unknown_type() {
                        self.summary.skipped += 1;
                        continue;
                    }

                    media_files_in_dir.push((
                        metadata,
                        RawFile {
                            id: Some(file.file_id),
                            name: file.file_name.to_owned(),
                            etag: file.etag.to_owned(),
                            size: file.size,
                            path: parent_path.to_owned(),
                        },
                    ));
                }
            }

            all_files.extend(self.convert_share_raw_file_to_media_file(media_files_in_dir));
        }

        Ok(all_files)
    }

    async fn list_files_from_pan189_share(&mut self, share_code: &str) -> AppResult<Vec<MediaFile>> {
        let share_info = self.state.pan189.get_share_info(share_code).await?;

        let mut all_files = Vec::new();
        let mut stack = vec![(share_info.file_id, share_info.file_name.to_owned())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let (folders, files) = self
                .state
                .pan189
                .list_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;

            for folder in &folders {
                stack.push((folder.id.to_owned(), format!("{}/{}", parent_path, folder.name)));
            }

            let mut media_files_in_dir = Vec::new();
            for file in &files {
                // Regular file
                self.summary.total += 1;
                let metadata = self.parse_media_metadata(&file.name, &parent_path);
                if metadata.unknown_type() {
                    self.summary.skipped += 1;
                    continue;
                }

                media_files_in_dir.push((
                    metadata,
                    RawFile {
                        id: None,
                        name: file.name.to_owned(),
                        etag: file.md5.to_lowercase(),
                        size: file.size,
                        path: parent_path.to_owned(),
                    },
                ));
            }
            all_files.extend(self.convert_share_raw_file_to_media_file(media_files_in_dir));
        }

        Ok(all_files)
    }
}
