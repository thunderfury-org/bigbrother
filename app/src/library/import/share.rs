use reqwest::Url;
use tracing::info;

use super::{
    ImportClient, ImportedMedia, Importer,
    group::group_video_and_subtitle_files,
    inner::{MediaFile, RawFile},
};
use crate::error::{AppError, AppResult};

pub enum ShareUrl<'a> {
    Pan123(&'a Url),
    Pan189(&'a Url),
    Pan115(&'a Url),
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
        } else if url
            .host_str()
            .is_some_and(|h| h == "115.com" || h == "115cdn.com")
            && url.path().starts_with("/s/")
        {
            Some(Self::Pan115(url))
        } else {
            None
        }
    }

    pub fn get_url(&self) -> &Url {
        match self {
            Self::Pan123(url) => url,
            Self::Pan189(url) => url,
            Self::Pan115(url) => url,
        }
    }
}

impl<C, M> Importer<C, M>
where
    C: ImportClient,
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
                .remote
                .list_pan123_share_files(share_key, share_password, parent_id)
                .await?;

            let mut media_files_in_dir = Vec::new();
            for file in &files {
                if file.is_dir {
                    // Directory
                    stack.push((file.file_id, format!("{}/{}", parent_path, file.file_name)));
                } else {
                    // Regular file

                    let metadata = self.parse_media_metadata(&file.file_name, &parent_path);
                    if metadata.unknown_type() {
                        continue;
                    }

                    media_files_in_dir.push((
                        metadata,
                        RawFile {
                            id: Some(file.file_id),
                            name: file.file_name.to_owned(),
                            etag: file.etag.as_str().into(),
                            size: file.size,
                            path: parent_path.to_owned(),
                        },
                    ));
                }
            }

            all_files.extend(group_video_and_subtitle_files(media_files_in_dir));
        }

        Ok(all_files)
    }

    async fn list_files_from_pan189_share(
        &mut self,
        share_code: &str,
    ) -> AppResult<Vec<MediaFile>> {
        let share_info = self.remote.get_pan189_share_info(share_code).await?;

        let mut all_files = Vec::new();
        let mut stack = vec![(share_info.file_id, share_info.file_name.to_owned())];

        while let Some((parent_id, parent_path)) = stack.pop() {
            let (folders, files) = self
                .remote
                .list_pan189_share_files(share_info.share_id, share_info.share_mode, &parent_id)
                .await?;

            for folder in &folders {
                stack.push((
                    folder.id.to_owned(),
                    format!("{}/{}", parent_path, folder.name),
                ));
            }

            let mut media_files_in_dir = Vec::new();
            for file in &files {
                // Regular file

                let metadata = self.parse_media_metadata(&file.name, &parent_path);
                if metadata.unknown_type() {
                    continue;
                }

                media_files_in_dir.push((
                    metadata,
                    RawFile {
                        id: None,
                        name: file.name.to_owned(),
                        etag: file.md5.as_str().into(),
                        size: file.size,
                        path: parent_path.to_owned(),
                    },
                ));
            }
            all_files.extend(group_video_and_subtitle_files(media_files_in_dir));
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
                .remote
                .list_pan115_share_files(share_code, receive_code, &cid)
                .await?;

            let mut media_files_in_dir = Vec::new();
            for entry in &entries {
                if entry.is_file() {
                    // Regular file
                    let metadata = self.parse_media_metadata(&entry.name, &parent_path);
                    if metadata.unknown_type() {
                        continue;
                    }

                    media_files_in_dir.push((
                        metadata,
                        RawFile {
                            id: None,
                            name: entry.name.to_owned(),
                            etag: entry.sha.as_deref().unwrap_or_default().into(),
                            size: entry.size,
                            path: parent_path.to_owned(),
                        },
                    ));
                } else if let Some(cid) = &entry.cid {
                    // Directory
                    stack.push((cid.to_owned(), format!("{}/{}", parent_path, entry.name)));
                }
            }

            all_files.extend(group_video_and_subtitle_files(media_files_in_dir));
        }

        Ok(all_files)
    }
}

fn parse_pan123_share_parts(url: &Url) -> (String, String) {
    let share_key = url
        .path_segments()
        .map(|mut segments| segments.next_back().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    let share_password = url
        .query_pairs()
        .find(|(key, _)| key == "pwd")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_key, share_password)
}

fn parse_pan189_share_code(url: &Url) -> String {
    url.query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| {
            if url.path().starts_with("/t/") {
                url.path_segments()
                    .map(|mut segments| segments.next_back().unwrap_or_default())
                    .unwrap_or_default()
                    .to_owned()
            } else {
                String::new()
            }
        })
}

fn parse_pan115_share_parts(url: &Url) -> (String, String) {
    let share_code = url
        .path_segments()
        .map(|mut segments| segments.next_back().unwrap_or_default())
        .unwrap_or_default()
        .to_owned();
    let receive_code = url
        .query_pairs()
        .find(|(key, _)| key == "password")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    (share_code, receive_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shareurl_from_pan115_with_password() {
        let url = Url::parse("https://115cdn.com/s/swfoexi3no3?password=j7b2").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_some());
        assert!(matches!(share_url.unwrap(), ShareUrl::Pan115(_)));
    }

    #[test]
    fn test_shareurl_from_pan115_with_rc() {
        let url = Url::parse("https://115.com/s/swfoexi3no3?rc=j7b2").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_some());
        assert!(matches!(share_url.unwrap(), ShareUrl::Pan115(_)));
    }

    #[test]
    fn test_shareurl_from_pan115_without_password() {
        let url = Url::parse("https://115.com/s/swfoexi3no3").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_some());
        assert!(matches!(share_url.unwrap(), ShareUrl::Pan115(_)));
    }

    #[test]
    fn test_shareurl_from_pan123() {
        let url = Url::parse("https://www.123pan.com/s/abc123?pwd=test").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_some());
        assert!(matches!(share_url.unwrap(), ShareUrl::Pan123(_)));
    }

    #[test]
    fn test_shareurl_from_pan189() {
        let url = Url::parse("https://cloud.189.cn/t/abc123").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_some());
        assert!(matches!(share_url.unwrap(), ShareUrl::Pan189(_)));
    }

    #[test]
    fn test_shareurl_from_invalid_url() {
        let url = Url::parse("https://example.com/s/abc123").unwrap();
        let share_url = ShareUrl::from(&url);

        assert!(share_url.is_none());
    }

    #[test]
    fn test_parse_pan123_share_parts() {
        let url = Url::parse("https://www.123pan.com/s/share123?pwd=pass456").unwrap();

        let (share_key, share_password) = parse_pan123_share_parts(&url);

        assert_eq!(share_key, "share123");
        assert_eq!(share_password, "pass456");
    }

    #[test]
    fn test_parse_pan189_share_code_prefers_query_code() {
        let url = Url::parse("https://cloud.189.cn/web/share?code=abc123").unwrap();

        let share_code = parse_pan189_share_code(&url);

        assert_eq!(share_code, "abc123");
    }

    #[test]
    fn test_parse_pan189_share_code_from_path() {
        let url = Url::parse("https://cloud.189.cn/t/pathcode").unwrap();

        let share_code = parse_pan189_share_code(&url);

        assert_eq!(share_code, "pathcode");
    }

    #[test]
    fn test_parse_pan115_share_parts() {
        let url = Url::parse("https://115.com/s/share115?password=recv").unwrap();

        let (share_code, receive_code) = parse_pan115_share_parts(&url);

        assert_eq!(share_code, "share115");
        assert_eq!(receive_code, "recv");
    }
}
