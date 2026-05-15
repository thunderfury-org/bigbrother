use url::Url;

use crate::{
    domain::share::RawFile,
    error::{AppError, AppResult},
};

use super::{
    ShareClient,
    collect::collect_pan115_directory_entries,
    traversal::ShareTraversal,
    url::{ShareUrl, parse_pan115_share_parts, parse_share_url},
};

#[derive(Clone)]
pub struct Pan115ShareService<S> {
    share_source: S,
}

impl<S: ShareClient> Pan115ShareService<S> {
    pub fn new(share_source: S) -> Self {
        Self { share_source }
    }

    pub async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
        let Some(ShareUrl::Pan115(url)) = parse_share_url(url) else {
            return Err(AppError::InvalidParameter(format!(
                "unsupported pan115 share url: {url}"
            )));
        };
        let (share_code, receive_code) = parse_pan115_share_parts(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {url}"
            )));
        }

        let mut traversal = ShareTraversal::new(("0".to_string(), String::new()));

        while let Some((cid, parent_path)) = traversal.next_dir() {
            let entries = self
                .share_source
                .list_pan115_share_files(&share_code, &receive_code, &cid)
                .await?;
            traversal.extend(collect_pan115_directory_entries(&entries, &parent_path));
        }

        Ok(traversal.into_raw_files())
    }
}
