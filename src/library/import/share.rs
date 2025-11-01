use reqwest::Url;

use crate::error::AppResult;

use super::{ImportSummary, Importer, MediaFile, RawFile};

impl Importer {
    pub async fn import_from_share_url(&mut self, url: &Url) -> AppResult<ImportSummary> {
        let share_key = url
            .path_segments()
            .map(|s| s.last().unwrap_or_default())
            .unwrap_or_default();
        let share_password = url
            .query_pairs()
            .find(|(k, _)| k == "pwd")
            .map(|(_, v)| v.to_string())
            .unwrap_or_default();

        let media_files = self.list_files_from_share(share_key, &share_password).await?;
        self.transfer_media_files(&media_files).await
    }

    async fn list_files_from_share(&mut self, share_key: &str, share_password: &str) -> AppResult<Vec<MediaFile>> {
        let mut all_files = Vec::new();
        let mut stack = vec![(0, String::new())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let files = self
                .state
                .pan123
                .list_share_file(share_key, share_password, parent_id)
                .await?;

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

                    all_files.push(MediaFile {
                        metadata,
                        raw: RawFile {
                            id: file.file_id,
                            name: file.file_name.to_owned(),
                            etag: file.etag.to_owned(),
                            size: file.size,
                            path: parent_path.to_owned(),
                        },
                    });
                }
            }
        }

        Ok(all_files)
    }
}
