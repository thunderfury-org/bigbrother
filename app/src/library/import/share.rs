use reqwest::Url;
use tracing::info;

pub use super::source::ShareUrl;

use super::{
    ImportedMedia, Importer, LibraryGateway, ShareSource,
    inner::{MediaFile, RawFile},
    source::{parse_pan115_share_parts, parse_pan123_share_parts, parse_pan189_share_code},
};
use crate::error::{AppError, AppResult};

impl<L, S, M> Importer<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: super::MetadataCatalog,
{
    pub async fn import_from_share_url(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from share URL: {}", url.get_url());
        match url {
            ShareUrl::Pan123(url) => self.import_pan123_share(url).await,
            ShareUrl::Pan189(url) => self.import_pan189_share(url).await,
            ShareUrl::Pan115(url) => self.import_pan115_share(url).await,
        }
    }

    async fn import_pan123_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_key, share_password) = parse_pan123_share_parts(url);

        let media_files = self
            .list_files_from_pan123_share(share_key.as_str(), share_password.as_str())
            .await?;
        info!("found {} media files from pan123 share", media_files.len());
        self.transfer_media_files(&media_files).await
    }

    async fn import_pan189_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let share_code = parse_pan189_share_code(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self.list_files_from_pan189_share(&share_code).await?;
        info!("found {} media files from pan189 share", media_files.len());
        self.transfer_media_files(&media_files).await
    }

    async fn import_pan115_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_code, receive_code) = parse_pan115_share_parts(url);

        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self
            .list_files_from_pan115_share(&share_code, &receive_code)
            .await?;
        info!("found {} media files from pan115 share", media_files.len());
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
                .share_remote
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            let mut raw_files = Vec::new();
            for file in &files {
                if file.is_dir {
                    stack.push((file.file_id, format!("{}/{}", parent_path, file.file_name)));
                } else {
                    raw_files.push(RawFile {
                        id: Some(file.file_id),
                        name: file.file_name.to_owned(),
                        etag: file.etag.as_str().into(),
                        size: file.size,
                        path: parent_path.to_owned(),
                    });
                }
            }

            all_files.extend(self.build_media_files(raw_files));
        }

        Ok(all_files)
    }

    async fn list_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let share_info = self.share_remote.get_pan189_share_info(share_code).await?;

        let mut all_files = Vec::new();
        let mut stack = vec![(share_info.file_id, share_info.file_name.to_owned())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let (folders, files) = self
                .share_remote
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;

            for folder in &folders {
                stack.push((
                    folder.id.to_owned(),
                    format!("{}/{}", parent_path, folder.name),
                ));
            }

            let mut raw_files = Vec::new();
            for file in &files {
                raw_files.push(RawFile {
                    id: None,
                    name: file.name.to_owned(),
                    etag: file.md5.as_str().into(),
                    size: file.size,
                    path: parent_path.to_owned(),
                });
            }
            all_files.extend(self.build_media_files(raw_files));
        }

        Ok(all_files)
    }

    async fn list_files_from_pan115_share(
        &mut self,
        share_code: &str,
        receive_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let mut all_files = Vec::new();
        let mut stack = vec![("0".to_string(), String::new())];

        while let Some((cid, parent_path)) = stack.pop() {
            let entries = self
                .share_remote
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            let mut raw_files = Vec::new();
            for entry in &entries {
                if entry.is_file() {
                    raw_files.push(RawFile {
                        id: None,
                        name: entry.name.to_owned(),
                        etag: entry.sha.as_deref().unwrap_or_default().into(),
                        size: entry.size,
                        path: parent_path.to_owned(),
                    });
                } else if let Some(cid) = &entry.cid {
                    stack.push((cid.to_owned(), format!("{}/{}", parent_path, entry.name)));
                }
            }

            all_files.extend(self.build_media_files(raw_files));
        }

        Ok(all_files)
    }
}
